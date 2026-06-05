/// RAW HID descriptor for VIA (vendor-defined usage page 0xFF60).
/// 32-byte IN/OUT reports, matching QMK's RAW_EPSIZE.
pub const RAW_HID_DESC: &[u8] = &[
    0x06, 0x60, 0xFF, // USAGE_PAGE (Vendor Defined 0xFF60)
    0x09, 0x61,       // USAGE (0x61)
    0xA1, 0x01,       // COLLECTION (Application)
    0x09, 0x62,       //   USAGE (0x62)
    0x15, 0x00,       //   LOGICAL_MINIMUM (0)
    0x26, 0xFF, 0x00, //   LOGICAL_MAXIMUM (255)
    0x95, 0x20,       //   REPORT_COUNT (32)
    0x75, 0x08,       //   REPORT_SIZE (8)
    0x81, 0x02,       //   INPUT (Data,Var,Abs)
    0x09, 0x63,       //   USAGE (0x63)
    0x15, 0x00,       //   LOGICAL_MINIMUM (0)
    0x26, 0xFF, 0x00, //   LOGICAL_MAXIMUM (255)
    0x95, 0x20,       //   REPORT_COUNT (32)
    0x75, 0x08,       //   REPORT_SIZE (8)
    0x91, 0x02,       //   OUTPUT (Data,Var,Abs)
    0xC0,             // END_COLLECTION
];

use crate::led::rgb::RgbMatrix;

/// VIA protocol version (QMK protocol 12 = 0x000C)
const VIA_PROTOCOL_VERSION: u16 = 0x000C;

/// VIA command IDs
const ID_GET_PROTOCOL_VERSION: u8 = 0x01;
const ID_GET_KEYBOARD_VALUE: u8 = 0x02;
const ID_SET_KEYBOARD_VALUE: u8 = 0x03;
const ID_DYNAMIC_KEYMAP_GET_KEYCODE: u8 = 0x04;
const ID_DYNAMIC_KEYMAP_SET_KEYCODE: u8 = 0x05;
const ID_DYNAMIC_KEYMAP_RESET: u8 = 0x06;
const ID_CUSTOM_SET_VALUE: u8 = 0x07;
const ID_CUSTOM_GET_VALUE: u8 = 0x08;
const ID_CUSTOM_SAVE: u8 = 0x09;
const ID_EEPROM_RESET: u8 = 0x0A;
const ID_BOOTLOADER_JUMP: u8 = 0x0B;
const ID_DYNAMIC_KEYMAP_MACRO_GET_COUNT: u8 = 0x0C;
const ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
const ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER: u8 = 0x0E;
const ID_DYNAMIC_KEYMAP_MACRO_SET_BUFFER: u8 = 0x0F;
const ID_DYNAMIC_KEYMAP_MACRO_RESET: u8 = 0x10;
const ID_DYNAMIC_KEYMAP_GET_LAYER_COUNT: u8 = 0x11;
const ID_DYNAMIC_KEYMAP_GET_BUFFER: u8 = 0x12;
const ID_DYNAMIC_KEYMAP_SET_BUFFER: u8 = 0x13;

/// Keyboard value IDs
const ID_UPTIME: u8 = 0x01;
const ID_LAYOUT_OPTIONS: u8 = 0x02;
const ID_SWITCH_MATRIX_STATE: u8 = 0x03;
const ID_FIRMWARE_VERSION: u8 = 0x04;

/// RGB Matrix custom channel (matches QMK id_qmk_rgb_matrix_channel = 3)
const CHANNEL_RGB_MATRIX: u8 = 3;

/// RGB Matrix value IDs
const RGB_MATRIX_BRIGHTNESS: u8 = 1;
const RGB_MATRIX_EFFECT: u8 = 2;
const RGB_MATRIX_EFFECT_SPEED: u8 = 3;
const RGB_MATRIX_COLOR: u8 = 4;

/// Number of layers for dynamic keymap
pub const DYNAMIC_KEYMAP_LAYER_COUNT: usize = 8;

/// Matrix dimensions
pub const VIA_MATRIX_ROWS: usize = 6;
pub const VIA_MATRIX_COLS: usize = 21;

/// Macro count and buffer size
pub const DYNAMIC_KEYMAP_MACRO_COUNT: usize = 0;
pub const DYNAMIC_KEYMAP_MACRO_BUFFER_SIZE: usize = 0;

/// Handle a VIA command. The 32-byte buffer is modified in-place.
/// `rgb` is the live RGB matrix state; channel 3 reads/writes it directly so
/// the VIA UI shows what the firmware is actually running.
/// `save_pending` is set on `ID_CUSTOM_SAVE` so the main loop can flush the
/// updated `rgb.*` into flash on the next idle tick (deferred, non-blocking).
/// Returns true if a response should be sent.
pub fn via_command(data: &mut [u8; 32], rgb: &mut RgbMatrix, save_pending: &mut bool) -> bool {
    let cmd = data[0];
    match cmd {
        ID_GET_PROTOCOL_VERSION => {
            data[0] = cmd;
            data[1] = (VIA_PROTOCOL_VERSION >> 8) as u8;
            data[2] = (VIA_PROTOCOL_VERSION & 0xFF) as u8;
            true
        }
        ID_GET_KEYBOARD_VALUE => {
            via_get_keyboard_value(data)
        }
        ID_SET_KEYBOARD_VALUE => {
            via_set_keyboard_value(data)
        }
        ID_DYNAMIC_KEYMAP_GET_KEYCODE => {
            via_dynamic_keymap_get_keycode(data)
        }
        ID_DYNAMIC_KEYMAP_SET_KEYCODE => {
            via_dynamic_keymap_set_keycode(data)
        }
        ID_DYNAMIC_KEYMAP_RESET => {
            via_dynamic_keymap_reset();
            true
        }
        ID_DYNAMIC_KEYMAP_GET_LAYER_COUNT => {
            data[0] = cmd;
            data[1] = DYNAMIC_KEYMAP_LAYER_COUNT as u8;
            true
        }
        ID_DYNAMIC_KEYMAP_GET_BUFFER => {
            via_dynamic_keymap_get_buffer(data)
        }
        ID_DYNAMIC_KEYMAP_SET_BUFFER => {
            via_dynamic_keymap_set_buffer(data)
        }
        ID_DYNAMIC_KEYMAP_MACRO_GET_COUNT => {
            data[0] = cmd;
            data[1] = DYNAMIC_KEYMAP_MACRO_COUNT as u8;
            true
        }
        ID_DYNAMIC_KEYMAP_MACRO_GET_BUFFER_SIZE => {
            data[0] = cmd;
            data[1] = (DYNAMIC_KEYMAP_MACRO_BUFFER_SIZE >> 8) as u8;
            data[2] = (DYNAMIC_KEYMAP_MACRO_BUFFER_SIZE & 0xFF) as u8;
            true
        }
        ID_EEPROM_RESET => {
            crate::config::eeprom::reset_to_defaults();
            via_dynamic_keymap_reset();
            true
        }
        ID_BOOTLOADER_JUMP => {
            // Set DFU magic and reset
            unsafe { crate::keyboard::matrix::Matrix::enter_bootloader(); }
            true // unreachable
        }
        ID_CUSTOM_GET_VALUE => {
            via_custom_get_value(data, rgb)
        }
        ID_CUSTOM_SET_VALUE => {
            via_custom_set_value(data, rgb)
        }
        ID_CUSTOM_SAVE => {
            // Acknowledge and flag the main loop to flush the (now-mirrored
            // rgb.* fields) into flash on the next idle tick. Same path as
            // firmware-side config changes.
            *save_pending = true;
            true
        }
        _ => {
            data[0] = 0xFF; // unhandled
            true
        }
    }
}

fn via_get_keyboard_value(data: &mut [u8; 32]) -> bool {
    let cmd = data[0];
    let value_id = data[1];
    match value_id {
        ID_UPTIME => {
            // uptime in ms — not critical, return 0
            data[0] = cmd;
            data[1] = value_id;
            data[2] = 0; data[3] = 0; data[4] = 0; data[5] = 0;
            true
        }
        ID_LAYOUT_OPTIONS => {
            data[0] = cmd;
            data[1] = value_id;
            data[2] = 0; data[3] = 0; data[4] = 0; data[5] = 0;
            true
        }
        ID_FIRMWARE_VERSION => {
            data[0] = cmd;
            data[1] = value_id;
            // Version as uint32: major*65536 + minor*256 + patch
            // Build-time derived from Cargo.toml — keeps VIA in sync with releases.
            let ver: u32 = {
                let v = env!("CARGO_PKG_VERSION");
                let mut parts = v.split('.');
                let major: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let minor: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let patch: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                (major << 16) | (minor << 8) | patch
            };
            data[2] = ((ver >> 24) & 0xFF) as u8;
            data[3] = ((ver >> 16) & 0xFF) as u8;
            data[4] = ((ver >> 8) & 0xFF) as u8;
            data[5] = (ver & 0xFF) as u8;
            true
        }
        ID_SWITCH_MATRIX_STATE => {
            // Return raw matrix state — not implemented, return zeros
            data[0] = cmd;
            data[1] = value_id;
            for i in 2..32 { data[i] = 0; }
            true
        }
        _ => {
            data[0] = 0xFF;
            true
        }
    }
}

fn via_set_keyboard_value(data: &mut [u8; 32]) -> bool {
    let value_id = data[1];
    match value_id {
        ID_LAYOUT_OPTIONS => {
            // Store layout options — accept but don't apply
            true
        }
        _ => {
            data[0] = 0xFF;
            true
        }
    }
}

/// EEPROM layout for dynamic keymap:
/// Base address is immediately after the saved UserConfig (magic + 12 fields
/// = 13 bytes, rounded up to 16 for halfword alignment in `config::eeprom::save`).
/// Keymap size: DYNAMIC_KEYMAP_LAYER_COUNT * ROWS * COLS * 2 = 8*6*21*2 = 2016 B,
/// leaving 2048 - 16 - 2016 = 16 B headroom in the 2KB config page.
const DYNAMIC_KEYMAP_EEPROM_ADDR: usize = 16; // right after UserConfig (16 B)

fn eeprom_addr_for_key(layer: usize, row: usize, col: usize) -> usize {
    DYNAMIC_KEYMAP_EEPROM_ADDR
        + layer * VIA_MATRIX_ROWS * VIA_MATRIX_COLS * 2
        + row * VIA_MATRIX_COLS * 2
        + col * 2
}

/// Write a block of bytes to the dynamic keymap area. Performs a single page
/// erase + full page write via `eeprom::save_keymap`, so a full 2KB buffer
/// flush costs ~100ms (50ms page erase + 50ms halfword writes). This is what
/// the QMK reference does on flash-backed EEPROM — there's no way to avoid
/// the page erase for a single byte change on STM32F0 without a second page.
fn write_keymap_block(offset: usize, bytes: &[u8]) {
    // Read current 2KB, modify the target range, write back.
    // `save_keymap` already does this; it expects a full 2016-byte buffer.
    let mut buf = [0xFFu8; 2016];
    for i in 0..2016 {
        buf[i] = crate::config::eeprom::read_byte(DYNAMIC_KEYMAP_EEPROM_ADDR + i);
    }
    buf[offset..offset + bytes.len()].copy_from_slice(bytes);
    crate::config::eeprom::save_keymap(&buf);
}

fn via_dynamic_keymap_get_keycode(data: &mut [u8; 32]) -> bool {
    let layer = data[1] as usize;
    let row = data[2] as usize;
    let col = data[3] as usize;
    if layer >= DYNAMIC_KEYMAP_LAYER_COUNT || row >= VIA_MATRIX_ROWS || col >= VIA_MATRIX_COLS {
        data[0] = 0xFF;
        return true;
    }
    let addr = eeprom_addr_for_key(layer, row, col);
    let kc_hi = crate::config::eeprom::read_byte(addr);
    let kc_lo = crate::config::eeprom::read_byte(addr + 1);
    data[0] = data[0];
    data[4] = kc_hi;
    data[5] = kc_lo;
    true
}

fn via_dynamic_keymap_set_keycode(data: &mut [u8; 32]) -> bool {
    let layer = data[1] as usize;
    let row = data[2] as usize;
    let col = data[3] as usize;
    let kc_hi = data[4];
    let kc_lo = data[5];
    if layer >= DYNAMIC_KEYMAP_LAYER_COUNT || row >= VIA_MATRIX_ROWS || col >= VIA_MATRIX_COLS {
        data[0] = 0xFF;
        return true;
    }
    let off = eeprom_addr_for_key(layer, row, col) - DYNAMIC_KEYMAP_EEPROM_ADDR;
    let bytes = [kc_hi, kc_lo];
    write_keymap_block(off, &bytes);
    true
}

fn via_dynamic_keymap_get_buffer(data: &mut [u8; 32]) -> bool {
    let offset = ((data[1] as usize) << 8) | (data[2] as usize);
    let size = data[3] as usize;
    let total = DYNAMIC_KEYMAP_LAYER_COUNT * VIA_MATRIX_ROWS * VIA_MATRIX_COLS * 2;
    if offset + size > total || size > 28 {
        data[0] = 0xFF;
        return true;
    }
    data[0] = data[0];
    // data[1..3] already contain offset+size (echo back)
    for i in 0..size {
        data[4 + i] = crate::config::eeprom::read_byte(DYNAMIC_KEYMAP_EEPROM_ADDR + offset + i);
    }
    true
}

fn via_dynamic_keymap_set_buffer(data: &mut [u8; 32]) -> bool {
    let offset = ((data[1] as usize) << 8) | (data[2] as usize);
    let size = data[3] as usize;
    let total = DYNAMIC_KEYMAP_LAYER_COUNT * VIA_MATRIX_ROWS * VIA_MATRIX_COLS * 2;
    if offset + size > total || size > 28 {
        data[0] = 0xFF;
        return true;
    }
    write_keymap_block(offset, &data[4..4 + size]);
    true
}

fn via_dynamic_keymap_reset() {
    // Write the default keymap layers to EEPROM in a single page write.
    // Layers 5..7 (beyond the 5 defined in keymap.rs) get KC_NO (0x0000),
    // matching QMK's `dynamic_keymap_reset()` which uses keycode_at_keymap_location_raw
    // (returns 0 for undefined layers).
    let mut buf = [0x00u8; 2016];
    let layers: [&[[u16; VIA_MATRIX_COLS]; VIA_MATRIX_ROWS]; 5] = [
        &crate::keyboard::keymap::LAYER_MAC,
        &crate::keyboard::keymap::LAYER_MAC_FN,
        &crate::keyboard::keymap::LAYER_WIN,
        &crate::keyboard::keymap::LAYER_WIN_FN,
        &crate::keyboard::keymap::LAYER_FN,
    ];
    for (layer_idx, layer) in layers.iter().enumerate() {
        for row in 0..VIA_MATRIX_ROWS {
            for col in 0..VIA_MATRIX_COLS {
                let kc = layer[row][col];
                let off = eeprom_addr_for_key(layer_idx, row, col) - DYNAMIC_KEYMAP_EEPROM_ADDR;
                buf[off] = (kc >> 8) as u8;
                buf[off + 1] = (kc & 0xFF) as u8;
            }
        }
    }
    // Layers 5..7 already 0x0000 (KC_NO) from the [0x00; 2016] init.
    crate::config::eeprom::save_keymap(&buf);
}

/// Resolve a keycode from the dynamic keymap (EEPROM) for the given layer/row/col.
/// Falls back to the static keymap if EEPROM is not initialized.
pub fn dynamic_keymap_get_keycode(layer: usize, row: usize, col: usize) -> u16 {
    if layer >= DYNAMIC_KEYMAP_LAYER_COUNT || row >= VIA_MATRIX_ROWS || col >= VIA_MATRIX_COLS {
        return 0;
    }
    let addr = eeprom_addr_for_key(layer, row, col);
    let hi = crate::config::eeprom::read_byte(addr);
    let lo = crate::config::eeprom::read_byte(addr + 1);
    let kc = ((hi as u16) << 8) | (lo as u16);
    // If EEPROM is uninitialized (0xFF 0xFF), fall back to static keymap
    if kc == 0xFFFF {
        return crate::keyboard::keymap::get_keycode(layer, row, col);
    }
    kc
}

fn via_custom_get_value(data: &mut [u8; 32], rgb: &RgbMatrix) -> bool {
    let channel = data[1];
    let value_id = data[2];
    if channel == CHANNEL_RGB_MATRIX {
        match value_id {
            RGB_MATRIX_BRIGHTNESS => {
                data[3] = rgb.val;
                true
            }
            RGB_MATRIX_EFFECT => {
                data[3] = rgb.mode;
                true
            }
            RGB_MATRIX_EFFECT_SPEED => {
                data[3] = rgb.speed;
                true
            }
            RGB_MATRIX_COLOR => {
                data[3] = rgb.hue;
                data[4] = rgb.sat;
                true
            }
            _ => { data[0] = 0xFF; true }
        }
    } else {
        data[0] = 0xFF;
        true
    }
}

fn via_custom_set_value(data: &mut [u8; 32], rgb: &mut RgbMatrix) -> bool {
    let channel = data[1];
    let value_id = data[2];
    if channel == CHANNEL_RGB_MATRIX {
        match value_id {
            RGB_MATRIX_BRIGHTNESS => {
                rgb.val = data[3];
            }
            RGB_MATRIX_EFFECT => {
                rgb.mode = data[3];
            }
            RGB_MATRIX_EFFECT_SPEED => {
                rgb.speed = data[3];
            }
            RGB_MATRIX_COLOR => {
                rgb.hue = data[3];
                rgb.sat = data[4];
            }
            _ => { data[0] = 0xFF; return true; }
        }
        // The animation step in the main loop reads these fields every frame,
        // so the change takes effect within one animation tick (~16 ms).
        // No need to set dirty flags here. The next ID_CUSTOM_SAVE will
        // signal `save_pending` and the deferred save block (which reads
        // dev.rgb.*) will persist them.
        true
    } else {
        data[0] = 0xFF;
        true
    }
}
