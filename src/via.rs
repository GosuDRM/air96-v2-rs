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
/// Returns true if a response should be sent.
pub fn via_command(data: &mut [u8; 32]) -> bool {
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
            true
        }
        ID_BOOTLOADER_JUMP => {
            // Set DFU magic and reset
            unsafe { crate::keyboard::matrix::Matrix::enter_bootloader(); }
            true // unreachable
        }
        ID_CUSTOM_GET_VALUE => {
            via_custom_get_value(data)
        }
        ID_CUSTOM_SET_VALUE => {
            via_custom_set_value(data)
        }
        ID_CUSTOM_SAVE => {
            via_custom_save(data)
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
            let ver: u32 = 4 * 65536 + 3 * 256 + 0;
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
/// Base address is after the existing UserConfig (16 bytes).
/// Keymap: DYNAMIC_KEYMAP_LAYER_COUNT * ROWS * COLS * 2 bytes
const DYNAMIC_KEYMAP_EEPROM_ADDR: usize = 64; // after UserConfig

fn eeprom_addr_for_key(layer: usize, row: usize, col: usize) -> usize {
    DYNAMIC_KEYMAP_EEPROM_ADDR
        + layer * VIA_MATRIX_ROWS * VIA_MATRIX_COLS * 2
        + row * VIA_MATRIX_COLS * 2
        + col * 2
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
    let addr = eeprom_addr_for_key(layer, row, col);
    crate::config::eeprom::write_byte(addr, kc_hi);
    crate::config::eeprom::write_byte(addr + 1, kc_lo);
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
    for i in 0..size {
        crate::config::eeprom::write_byte(DYNAMIC_KEYMAP_EEPROM_ADDR + offset + i, data[4 + i]);
    }
    true
}

fn via_dynamic_keymap_reset() {
    // Write the default keymap layers to EEPROM
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
                let addr = eeprom_addr_for_key(layer_idx, row, col);
                crate::config::eeprom::write_byte(addr, (kc >> 8) as u8);
                crate::config::eeprom::write_byte(addr + 1, (kc & 0xFF) as u8);
            }
        }
    }
    // Fill remaining layers with KC_NO (0x0000)
    for layer_idx in 5..DYNAMIC_KEYMAP_LAYER_COUNT {
        for row in 0..VIA_MATRIX_ROWS {
            for col in 0..VIA_MATRIX_COLS {
                let addr = eeprom_addr_for_key(layer_idx, row, col);
                crate::config::eeprom::write_byte(addr, 0);
                crate::config::eeprom::write_byte(addr + 1, 0);
            }
        }
    }
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

fn via_custom_get_value(data: &mut [u8; 32]) -> bool {
    let channel = data[1];
    let value_id = data[2];
    if channel == CHANNEL_RGB_MATRIX {
        // Return current RGB state — caller should pass these in
        // For now, return defaults (VIA will read from EEPROM on next connect)
        match value_id {
            RGB_MATRIX_BRIGHTNESS => {
                data[3] = 255; // default val
                true
            }
            RGB_MATRIX_EFFECT => {
                data[3] = 4; // CYCLE_LEFT_RIGHT
                true
            }
            RGB_MATRIX_EFFECT_SPEED => {
                data[3] = 255; // max speed
                true
            }
            RGB_MATRIX_COLOR => {
                data[3] = 0;   // hue
                data[4] = 255; // sat
                true
            }
            _ => { data[0] = 0xFF; true }
        }
    } else {
        data[0] = 0xFF;
        true
    }
}

fn via_custom_set_value(data: &mut [u8; 32]) -> bool {
    let channel = data[1];
    let _value_id = data[2];
    if channel == CHANNEL_RGB_MATRIX {
        // Accept the value — actual application would need Device access
        // VIA will send a save command after all values are set
        true
    } else {
        data[0] = 0xFF;
        true
    }
}

fn via_custom_save(data: &mut [u8; 32]) -> bool {
    let channel = data[1];
    if channel == CHANNEL_RGB_MATRIX {
        // Save RGB config — actual save would need Device access
        true
    } else {
        data[0] = 0xFF;
        true
    }
}
