//! VIA — 1:1 port of `quantum/via.c` and `quantum/dynamic_keymap.c` as built
//! for the C VIA firmware (VIA protocol 12, 8 dynamic layers, 16 macros).
//!
//! EEPROM addresses are the C build's (read from its compiled
//! `dynamic_keymap_key_to_eeprom_address` / `dynamic_keymap_macro_*`):
//! VIA magic 44..47, layout options 47, keymap 48..2064, macros 2064..4096.

use crate::config::eeprom::{ee_read, ee_update};
use crate::keyboard::keymap;
use crate::led::rgb::RgbMatrix;

pub const VIA_PROTOCOL_VERSION: u16 = 0x000C;
pub const VIA_FIRMWARE_VERSION: u32 = 0x0000_0000;
pub const DYNAMIC_KEYMAP_LAYER_COUNT: u8 = 8;
pub const DYNAMIC_KEYMAP_MACRO_COUNT: u8 = 16;
const MATRIX_ROWS: u16 = 6;
const MATRIX_COLS: u16 = 21;

pub const VIA_EEPROM_MAGIC_ADDR: u16 = 44;
pub const VIA_EEPROM_LAYOUT_OPTIONS_ADDR: u16 = VIA_EEPROM_MAGIC_ADDR + 3;
pub const DYNAMIC_KEYMAP_EEPROM_ADDR: u16 = 48;
pub const DYNAMIC_KEYMAP_EEPROM_SIZE: u16 = DYNAMIC_KEYMAP_LAYER_COUNT as u16 * MATRIX_ROWS * MATRIX_COLS * 2;
pub const DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR: u16 = DYNAMIC_KEYMAP_EEPROM_ADDR + DYNAMIC_KEYMAP_EEPROM_SIZE;
pub const DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE: u16 = 4096 - DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR;

// enum via_command_id
pub const ID_GET_PROTOCOL_VERSION: u8 = 0x01;
pub const ID_GET_KEYBOARD_VALUE: u8 = 0x02;
pub const ID_SET_KEYBOARD_VALUE: u8 = 0x03;
pub const ID_DYNAMIC_KEYMAP_GET_KEYCODE: u8 = 0x04;
pub const ID_DYNAMIC_KEYMAP_SET_KEYCODE: u8 = 0x05;
pub const ID_DYNAMIC_KEYMAP_RESET: u8 = 0x06;
pub const ID_CUSTOM_SET_VALUE: u8 = 0x07;
pub const ID_CUSTOM_GET_VALUE: u8 = 0x08;
pub const ID_CUSTOM_SAVE: u8 = 0x09;
pub const ID_EEPROM_RESET: u8 = 0x0A;
pub const ID_BOOTLOADER_JUMP: u8 = 0x0B;
pub const ID_DYNAMIC_KEYMAP_MACRO_GET_COUNT: u8 = 0x0C;
pub const ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
pub const ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER: u8 = 0x0E;
pub const ID_DYNAMIC_KEYMAP_MACRO_SET_BUFFER: u8 = 0x0F;
pub const ID_DYNAMIC_KEYMAP_MACRO_RESET: u8 = 0x10;
pub const ID_DYNAMIC_KEYMAP_GET_LAYER_COUNT: u8 = 0x11;
pub const ID_DYNAMIC_KEYMAP_GET_BUFFER: u8 = 0x12;
pub const ID_DYNAMIC_KEYMAP_SET_BUFFER: u8 = 0x13;
pub const ID_UNHANDLED: u8 = 0xFF;

// enum via_keyboard_value_id
pub const ID_UPTIME: u8 = 0x01;
pub const ID_LAYOUT_OPTIONS: u8 = 0x02;
pub const ID_SWITCH_MATRIX_STATE: u8 = 0x03;
pub const ID_FIRMWARE_VERSION: u8 = 0x04;
pub const ID_DEVICE_INDICATION: u8 = 0x05;

// enum via_channel_id / via_qmk_rgb_matrix_value
pub const ID_QMK_RGB_MATRIX_CHANNEL: u8 = 3;
pub const ID_QMK_RGB_MATRIX_BRIGHTNESS: u8 = 1;
pub const ID_QMK_RGB_MATRIX_EFFECT: u8 = 2;
pub const ID_QMK_RGB_MATRIX_EFFECT_SPEED: u8 = 3;
pub const ID_QMK_RGB_MATRIX_COLOR: u8 = 4;

const RGB_MATRIX_MAXIMUM_BRIGHTNESS: u8 = 255;

/// `QMK_BUILDDATE` (e.g. "2019-11-05-11:29:54"), set by build.rs.
const QMK_BUILDDATE: &[u8] = env!("QMK_BUILDDATE").as_bytes();

fn via_magic() -> [u8; 3] {
    let p = QMK_BUILDDATE;
    [((p[2] & 0x0F) << 4) | (p[3] & 0x0F), ((p[5] & 0x0F) << 4) | (p[6] & 0x0F), ((p[8] & 0x0F) << 4) | (p[9] & 0x0F)]
}

/// `via_eeprom_is_valid()`.
pub fn via_eeprom_is_valid() -> bool {
    let m = via_magic();
    (0..3).all(|i| ee_read(VIA_EEPROM_MAGIC_ADDR + i) == m[i as usize])
}

/// `via_eeprom_set_valid()`.
pub fn via_eeprom_set_valid(valid: bool) {
    let m = if valid { via_magic() } else { [0xFF; 3] };
    ee_update(VIA_EEPROM_MAGIC_ADDR, &m);
}

/// `via_init()`: a new firmware build (different build date) resets VIA data.
#[inline(never)]
pub fn via_init() {
    if !via_eeprom_is_valid() {
        eeconfig_init_via();
    }
}

/// `eeconfig_init_via()`.
pub fn eeconfig_init_via() {
    via_eeprom_set_valid(false);
    via_set_layout_options(0);
    dynamic_keymap_reset();
    dynamic_keymap_macro_reset();
    via_eeprom_set_valid(true);
}

/// `via_get_layout_options()`.
pub fn via_get_layout_options() -> u32 {
    ee_read(VIA_EEPROM_LAYOUT_OPTIONS_ADDR) as u32
}

/// `via_set_layout_options()` (VIA_EEPROM_LAYOUT_OPTIONS_SIZE = 1).
pub fn via_set_layout_options(value: u32) {
    ee_update(VIA_EEPROM_LAYOUT_OPTIONS_ADDR, &[value as u8]);
}

fn key_address(layer: u8, row: u8, col: u8) -> u16 {
    DYNAMIC_KEYMAP_EEPROM_ADDR + layer as u16 * MATRIX_ROWS * MATRIX_COLS * 2 + row as u16 * MATRIX_COLS * 2 + col as u16 * 2
}

/// `dynamic_keymap_get_keycode()`.
pub fn dynamic_keymap_get_keycode(layer: u8, row: u8, col: u8) -> u16 {
    if layer >= DYNAMIC_KEYMAP_LAYER_COUNT || row as u16 >= MATRIX_ROWS || col as u16 >= MATRIX_COLS {
        return keymap::KC_NO;
    }
    let a = key_address(layer, row, col);
    ((ee_read(a) as u16) << 8) | ee_read(a + 1) as u16
}

/// `dynamic_keymap_set_keycode()`.
pub fn dynamic_keymap_set_keycode(layer: u8, row: u8, col: u8, keycode: u16) {
    if layer >= DYNAMIC_KEYMAP_LAYER_COUNT || row as u16 >= MATRIX_ROWS || col as u16 >= MATRIX_COLS {
        return;
    }
    ee_update(key_address(layer, row, col), &keycode.to_be_bytes());
}

/// `dynamic_keymap_reset()`: every layer from the compiled keymap
/// (`keycode_at_keymap_location_raw`: KC_TRNS beyond layer 4).
pub fn dynamic_keymap_reset() {
    const ROW_BYTES: u16 = MATRIX_COLS * 2;
    const LAYER_BYTES: u16 = MATRIX_ROWS * ROW_BYTES;
    crate::config::eeprom::ee_fill(DYNAMIC_KEYMAP_EEPROM_ADDR, DYNAMIC_KEYMAP_EEPROM_SIZE, |off| {
        let (layer, rest) = (off / LAYER_BYTES, off % LAYER_BYTES);
        let (row, col) = (rest / ROW_BYTES, (rest % ROW_BYTES) / 2);
        let kc = keymap::get_keycode(layer as usize, row as usize, col as usize);
        if off % 2 == 0 { (kc >> 8) as u8 } else { kc as u8 }
    });
}

/// `dynamic_keymap_get_buffer()`.
pub fn dynamic_keymap_get_buffer(offset: u16, size: u16, data: &mut [u8]) {
    for i in 0..size {
        data[i as usize] = if (offset as u32 + i as u32) < DYNAMIC_KEYMAP_EEPROM_SIZE as u32 {
            ee_read(DYNAMIC_KEYMAP_EEPROM_ADDR + offset + i)
        } else {
            0
        };
    }
}

/// `dynamic_keymap_set_buffer()`.
pub fn dynamic_keymap_set_buffer(offset: u16, size: u16, data: &[u8]) {
    let n = size.min(DYNAMIC_KEYMAP_EEPROM_SIZE.saturating_sub(offset));
    if offset < DYNAMIC_KEYMAP_EEPROM_SIZE {
        ee_update(DYNAMIC_KEYMAP_EEPROM_ADDR + offset, &data[..n as usize]);
    }
}

/// `dynamic_keymap_macro_get_buffer()`.
pub fn dynamic_keymap_macro_get_buffer(offset: u16, size: u16, data: &mut [u8]) {
    for i in 0..size {
        data[i as usize] = if (offset as u32 + i as u32) < DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE as u32 {
            ee_read(DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR + offset + i)
        } else {
            0
        };
    }
}

/// `dynamic_keymap_macro_set_buffer()`.
pub fn dynamic_keymap_macro_set_buffer(offset: u16, size: u16, data: &[u8]) {
    let n = size.min(DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE.saturating_sub(offset));
    if offset < DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE {
        ee_update(DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR + offset, &data[..n as usize]);
    }
}

/// `dynamic_keymap_macro_reset()`.
pub fn dynamic_keymap_macro_reset() {
    crate::config::eeprom::ee_fill(DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR, DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE, |_| 0);
}

/// One `send_string_with_delay()` step of a dynamic macro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroStep {
    /// A printable character (`send_char_with_delay()`).
    Char(u8),
    Tap(u8),
    Down(u8),
    Up(u8),
    /// `SS_DELAY_CODE`: wait this many ms.
    Delay(u32),
}

/// `dynamic_keymap_macro_send()`: decode macro `id` into steps (the caller
/// performs them with `DYNAMIC_KEYMAP_MACRO_DELAY` between steps).
pub fn dynamic_keymap_macro_send(id: u8, mut step: impl FnMut(MacroStep)) {
    if id >= DYNAMIC_KEYMAP_MACRO_COUNT {
        return;
    }
    let end = DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR + DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE;
    // Check the last byte of the buffer: a non-zero value means a corrupt buffer.
    if ee_read(end - 1) != 0 {
        return;
    }
    let mut p = DYNAMIC_KEYMAP_MACRO_EEPROM_ADDR;
    let mut id = id;
    while id > 0 {
        if p == end {
            return;
        }
        if ee_read(p) == 0 {
            id -= 1;
        }
        p += 1;
    }
    let mut read = || {
        let b = ee_read(p);
        p += 1;
        b
    };
    loop {
        let c = read();
        if c == 0 {
            break;
        }
        if c == 1 {
            // SS_QMK_PREFIX
            let code = read();
            if code == 0 {
                return;
            }
            match code {
                1 | 2 | 3 => {
                    let kc = read();
                    if kc == 0 {
                        return;
                    }
                    step(match code {
                        1 => MacroStep::Tap(kc),
                        2 => MacroStep::Down(kc),
                        _ => MacroStep::Up(kc),
                    });
                }
                4 => {
                    // SS_DELAY_CODE: up to 4 bytes then '|'; send_string() waits
                    // for the leading decimal digits.
                    let mut buf = [0u8; 5];
                    let mut i = 2;
                    loop {
                        let d = read();
                        if d == 0 {
                            return;
                        }
                        if d == b'|' {
                            break;
                        }
                        if i == 6 {
                            return;
                        }
                        buf[i - 2] = d;
                        i += 1;
                    }
                    let ms = buf.iter().take_while(|d| d.is_ascii_digit()).fold(0u32, |a, d| a * 10 + (d - b'0') as u32);
                    step(MacroStep::Delay(ms));
                }
                _ => {
                    // Unknown SS code: send_string ignores it but still waits.
                    step(MacroStep::Delay(0));
                }
            }
        } else {
            step(MacroStep::Char(c));
        }
    }
}

/// `ascii_to_keycode_lut` (send_string_keycodes, US ANSI).
pub const ASCII_TO_KEYCODE_LUT: [u8; 128] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2A, 0x2B, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00, 0x00,
    0x2C, 0x1E, 0x34, 0x20, 0x21, 0x22, 0x24, 0x34, 0x26, 0x27, 0x25, 0x2E, 0x36, 0x2D, 0x37, 0x38,
    0x27, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x33, 0x33, 0x36, 0x2E, 0x37, 0x38,
    0x1F, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12,
    0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x2F, 0x31, 0x30, 0x23, 0x2D,
    0x35, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12,
    0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x2F, 0x31, 0x30, 0x35, 0x4C,
];

/// `ascii_to_shift_lut` (bit per character).
pub const ASCII_TO_SHIFT_LUT: [u8; 16] = [0x00, 0x00, 0x00, 0x00, 0x7E, 0x0F, 0x00, 0xD4, 0xFF, 0xFF, 0xFF, 0xC7, 0x00, 0x00, 0x00, 0x78];

pub fn ascii_shifted(c: u8) -> bool {
    (ASCII_TO_SHIFT_LUT[(c / 8) as usize & 0x0F] >> (c % 8)) & 1 != 0
}

/// What `raw_hid_receive()` needs from the rest of the firmware.
pub struct ViaContext<'a> {
    pub rgb: &'a mut RgbMatrix,
    /// `timer_read32()`.
    pub now: u32,
    /// `matrix_get_row()` for each row (debounced matrix).
    pub matrix: [u32; 6],
    /// Set by `id_custom_save` on the RGB matrix channel (`eeconfig_update_rgb_matrix()`).
    pub rgb_save: bool,
}

/// `raw_hid_receive()`: handles the command in place; the caller sends `data`
/// back (`raw_hid_send(data, length)`).
#[inline(never)]
pub fn raw_hid_receive(data: &mut [u8; 32], ctx: &mut ViaContext) {
    let command_id = data[0];
    let cd = 1; // command_data offset
    match command_id {
        ID_GET_PROTOCOL_VERSION => {
            data[cd] = (VIA_PROTOCOL_VERSION >> 8) as u8;
            data[cd + 1] = VIA_PROTOCOL_VERSION as u8;
        }
        ID_GET_KEYBOARD_VALUE => match data[cd] {
            ID_UPTIME => data[cd + 1..cd + 5].copy_from_slice(&ctx.now.to_be_bytes()),
            ID_LAYOUT_OPTIONS => data[cd + 1..cd + 5].copy_from_slice(&via_get_layout_options().to_be_bytes()),
            ID_SWITCH_MATRIX_STATE => {
                // 21 columns → 3 bytes per row, 28 / 3 = 9 rows per reply.
                let offset = data[cd + 1] as usize;
                let rows = 28 / ((MATRIX_COLS as usize + 7) / 8);
                let mut i = cd + 2;
                let mut row = 0;
                while row < rows && row + offset < MATRIX_ROWS as usize {
                    let v = ctx.matrix[row + offset];
                    data[i] = (v >> 16) as u8;
                    data[i + 1] = (v >> 8) as u8;
                    data[i + 2] = v as u8;
                    i += 3;
                    row += 1;
                }
            }
            ID_FIRMWARE_VERSION => data[cd + 1..cd + 5].copy_from_slice(&VIA_FIRMWARE_VERSION.to_be_bytes()),
            _ => data[0] = ID_UNHANDLED,
        },
        ID_SET_KEYBOARD_VALUE => match data[cd] {
            ID_LAYOUT_OPTIONS => {
                let v = u32::from_be_bytes([data[cd + 1], data[cd + 2], data[cd + 3], data[cd + 4]]);
                via_set_layout_options(v);
            }
            ID_DEVICE_INDICATION => {
                // via_set_device_indication(): rgb_matrix_toggle_noeeprom()
                ctx.rgb.toggle();
            }
            _ => data[0] = ID_UNHANDLED,
        },
        ID_DYNAMIC_KEYMAP_GET_KEYCODE => {
            let kc = dynamic_keymap_get_keycode(data[cd], data[cd + 1], data[cd + 2]);
            data[cd + 3] = (kc >> 8) as u8;
            data[cd + 4] = kc as u8;
        }
        ID_DYNAMIC_KEYMAP_SET_KEYCODE => {
            let kc = ((data[cd + 3] as u16) << 8) | data[cd + 4] as u16;
            dynamic_keymap_set_keycode(data[cd], data[cd + 1], data[cd + 2], kc);
        }
        ID_DYNAMIC_KEYMAP_RESET => dynamic_keymap_reset(),
        ID_CUSTOM_SET_VALUE | ID_CUSTOM_GET_VALUE | ID_CUSTOM_SAVE => via_custom_value_command(data, ctx),
        ID_DYNAMIC_KEYMAP_MACRO_GET_COUNT => data[cd] = DYNAMIC_KEYMAP_MACRO_COUNT,
        ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER_SIZE => {
            data[cd] = (DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE >> 8) as u8;
            data[cd + 1] = DYNAMIC_KEYMAP_MACRO_EEPROM_SIZE as u8;
        }
        ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER => {
            let offset = ((data[cd] as u16) << 8) | data[cd + 1] as u16;
            let size = (data[cd + 2] as u16).min(28);
            let mut out = [0u8; 28];
            dynamic_keymap_macro_get_buffer(offset, size, &mut out);
            data[cd + 3..cd + 3 + size as usize].copy_from_slice(&out[..size as usize]);
        }
        ID_DYNAMIC_KEYMAP_MACRO_SET_BUFFER => {
            let offset = ((data[cd] as u16) << 8) | data[cd + 1] as u16;
            let size = (data[cd + 2] as u16).min(28);
            let bytes: [u8; 28] = data[cd + 3..cd + 31].try_into().unwrap();
            dynamic_keymap_macro_set_buffer(offset, size, &bytes);
        }
        ID_DYNAMIC_KEYMAP_MACRO_RESET => dynamic_keymap_macro_reset(),
        ID_DYNAMIC_KEYMAP_GET_LAYER_COUNT => data[cd] = DYNAMIC_KEYMAP_LAYER_COUNT,
        ID_DYNAMIC_KEYMAP_GET_BUFFER => {
            let offset = ((data[cd] as u16) << 8) | data[cd + 1] as u16;
            let size = (data[cd + 2] as u16).min(28);
            let mut out = [0u8; 28];
            dynamic_keymap_get_buffer(offset, size, &mut out);
            data[cd + 3..cd + 3 + size as usize].copy_from_slice(&out[..size as usize]);
        }
        ID_DYNAMIC_KEYMAP_SET_BUFFER => {
            let offset = ((data[cd] as u16) << 8) | data[cd + 1] as u16;
            let size = (data[cd + 2] as u16).min(28);
            let bytes: [u8; 28] = data[cd + 3..cd + 31].try_into().unwrap();
            dynamic_keymap_set_buffer(offset, size, &bytes);
        }
        // id_eeprom_reset needs VIA_EEPROM_ALLOW_RESET and id_bootloader_jump
        // isn't handled by via.c: both are unhandled in the C build.
        _ => data[0] = ID_UNHANDLED,
    }
}

/// `via_custom_value_command()`: only the RGB matrix channel is handled.
fn via_custom_value_command(data: &mut [u8; 32], ctx: &mut ViaContext) {
    if data[1] != ID_QMK_RGB_MATRIX_CHANNEL {
        // via_custom_value_command_kb() default
        data[0] = ID_UNHANDLED;
        return;
    }
    let value_id = data[2];
    match data[0] {
        ID_CUSTOM_SET_VALUE => {
            let rgb = &mut *ctx.rgb;
            match value_id {
                ID_QMK_RGB_MATRIX_BRIGHTNESS => {
                    // scale8(v, MAX) = v * MAX >> 8 (FASTLED_SCALE8_FIXED is off)
                    let v = ((data[3] as u16 * RGB_MATRIX_MAXIMUM_BRIGHTNESS as u16) >> 8) as u8;
                    rgb.sethsv_noeeprom(rgb.hue, rgb.sat, v);
                }
                ID_QMK_RGB_MATRIX_EFFECT => {
                    if data[3] == 0 {
                        rgb.disable();
                    } else {
                        rgb.enable();
                        rgb.set_mode(data[3]);
                    }
                }
                ID_QMK_RGB_MATRIX_EFFECT_SPEED => rgb.speed = data[3],
                ID_QMK_RGB_MATRIX_COLOR => {
                    let v = rgb.val;
                    rgb.sethsv_noeeprom(data[3], data[4], v);
                }
                _ => {}
            }
        }
        ID_CUSTOM_GET_VALUE => match value_id {
            ID_QMK_RGB_MATRIX_BRIGHTNESS => {
                data[3] = ((ctx.rgb.val as u16 * u8::MAX as u16) / RGB_MATRIX_MAXIMUM_BRIGHTNESS as u16) as u8;
            }
            ID_QMK_RGB_MATRIX_EFFECT => data[3] = if ctx.rgb.enabled { ctx.rgb.mode } else { 0 },
            ID_QMK_RGB_MATRIX_EFFECT_SPEED => data[3] = ctx.rgb.speed,
            ID_QMK_RGB_MATRIX_COLOR => {
                data[3] = ctx.rgb.hue;
                data[4] = ctx.rgb.sat;
            }
            _ => {}
        },
        ID_CUSTOM_SAVE => ctx.rgb_save = true,
        _ => data[0] = ID_UNHANDLED,
    }
}
