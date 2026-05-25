//! Keymap engine — port of keymaps/default/keymap.c + ansi.h custom keycodes
//!
//! 5 layers: Mac base(0), Mac Fn(1), Win base(2), Win Fn(3), Function(4)
//! Custom keycodes: RF_DFU, LNK_USB/RF/BLE1-3, MAC_TASK/SEARCH/VOICE/CONSOLE/DND/PRT/PRTA,
//!                   BRID/BRIU, MPRV/MPLY/MNXT, MUTE/VOLD/VOLU,
//!                   SIDE_VAI/VAD/MOD/HUI/SPI/SPD,
//!                   RGB_SPD/SPI/MOD/VAI/VAD/HUI, DEV_RESET, SLEEP_MODE, BAT_SHOW, RGB_TEST, BAT_NUM

use crate::wireless::uart::LinkMode;

pub const LAYER_COUNT: usize = 5;

// ── Custom keycodes (port of ansi.h enum + QMK built-in RGB/media) ─────
pub const KC_RF_DFU: u16      = 0x5C00;
pub const KC_LNK_USB: u16     = 0x5C01;
pub const KC_LNK_RF: u16      = 0x5C02;
pub const KC_LNK_BLE1: u16    = 0x5C03;
pub const KC_LNK_BLE2: u16    = 0x5C04;
pub const KC_LNK_BLE3: u16    = 0x5C05;
pub const KC_MAC_TASK: u16    = 0x5C06;
pub const KC_MAC_SEARCH: u16  = 0x5C07;
pub const KC_MAC_VOICE: u16   = 0x5C08;
pub const KC_MAC_CONSOLE: u16 = 0x5C09;
pub const KC_MAC_DND: u16     = 0x5C0A;
pub const KC_MAC_PRT: u16     = 0x5C0B;
pub const KC_MAC_PRTA: u16    = 0x5C0C;
pub const KC_SIDE_VAI: u16    = 0x5C0D;
pub const KC_SIDE_VAD: u16    = 0x5C0E;
pub const KC_SIDE_MOD: u16    = 0x5C0F;
pub const KC_SIDE_HUI: u16    = 0x5C10;
pub const KC_SIDE_SPI: u16    = 0x5C11;
pub const KC_SIDE_SPD: u16    = 0x5C12;
pub const KC_DEV_RESET: u16   = 0x5C13;
pub const KC_SLEEP_MODE: u16  = 0x5C14;
pub const KC_BAT_SHOW: u16    = 0x5C15;
pub const KC_RGB_TEST: u16    = 0x5C16;
pub const KC_BAT_NUM: u16     = 0x5C17;
// Consumer keys (wireless: send via NRF consumer report)
pub const KC_BRID: u16        = 0x5C18; // Brightness Down  (usage 0x0070)
pub const KC_BRIU: u16        = 0x5C19; // Brightness Up    (usage 0x006F)
pub const KC_MPRV: u16        = 0x5C1A; // Previous Track   (usage 0x00B6)
pub const KC_MPLY: u16        = 0x5C1B; // Play/Pause       (usage 0x00CD)
pub const KC_MNXT: u16        = 0x5C1C; // Next Track       (usage 0x00B5)
pub const KC_MUTE: u16        = 0x5C1D; // Mute             (usage 0x00E2)
pub const KC_VOLD: u16        = 0x5C1E; // Volume Down      (usage 0x00EA)
pub const KC_VOLU: u16        = 0x5C1F; // Volume Up        (usage 0x00E9)
// RGB matrix controls
pub const KC_RGB_SPD: u16     = 0x5C30; // RGB speed down
pub const KC_RGB_SPI: u16     = 0x5C31; // RGB speed up
pub const KC_RGB_VAI: u16     = 0x5C32; // RGB brightness up
pub const KC_RGB_VAD: u16     = 0x5C33; // RGB brightness down
pub const KC_RGB_MOD: u16     = 0x5C34; // RGB mode cycle
pub const KC_RGB_HUI: u16     = 0x5C35; // RGB hue up

// ── Consumer usage codes mapped to keycodes ────────────────────────────
pub fn consumer_usage(kc: u16) -> u16 {
    match kc {
        KC_BRID => 0x0070,
        KC_BRIU => 0x006F,
        KC_MPRV => 0x00B6,
        KC_MPLY => 0x00CD,
        KC_MNXT => 0x00B5,
        KC_MUTE => 0x00E2,
        KC_VOLD => 0x00EA,
        KC_VOLU => 0x00E9,
        _ => 0,
    }
}

pub fn is_consumer_key(kc: u16) -> bool {
    (KC_BRID..=KC_VOLU).contains(&kc)
}

/// Check if a keycode is a custom (non-HID) keycode
pub fn is_custom(kc: u16) -> bool {
    kc >= 0x5C00
}

/// Get link channel from LNK keycode (port of O1 dedup: keycode - LNK_RF)
pub fn lnk_to_channel(kc: u16) -> LinkMode {
    match kc {
        KC_LNK_USB => LinkMode::Usb,
        KC_LNK_RF  => LinkMode::Rf24,
        KC_LNK_BLE1 => LinkMode::Bt1,
        KC_LNK_BLE2 => LinkMode::Bt2,
        KC_LNK_BLE3 => LinkMode::Bt3,
        _ => LinkMode::Usb,
    }
}

// ── HID keycode aliases ────────────────────────────────────────────────
pub const KC_NO: u16    = 0x0000;
pub const KC_ENTER: u16 = 0x0028;
pub const KC_ESC: u16   = 0x0029;
pub const KC_SPACE: u16 = 0x002C;
pub const KC_LCTRL: u16 = 0x00E0;
pub const KC_LSHIFT: u16 = 0x00E1;
pub const KC_LALT: u16  = 0x00E2;
pub const KC_LGUI: u16  = 0x00E3;
pub const KC_RCTRL: u16 = 0x00E4;
pub const KC_RSHIFT: u16 = 0x00E5;
pub const KC_RALT: u16  = 0x00E6;
pub const KC_RGUI: u16  = 0x00E7;

// Layer toggle
pub const MO: u16 = 0x5C20;
pub fn mo_layer(kc: u16) -> Option<usize> {
    if kc & 0xFF00 == MO & 0xFF00 { Some((kc & 0x0F) as usize) } else { None }
}

// ═══════════════════════════════════════════════════════════════════
// LAYER 0: Mac base
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_MAC: [[u16; 21]; 6] = [
    // Row 0: ESC, BRID, BRIU, MAC_TASK, MAC_SEARCH, MAC_VOICE, MAC_DND, MPRV, MPLY, MNXT, MUTE, VOLD, VOLU, MAC_PRTA, DEL, HOME, END, PGUP, PGDN
    [0x29, KC_BRID, KC_BRIU, KC_MAC_TASK, KC_MAC_SEARCH, KC_MAC_VOICE, KC_MAC_DND, KC_MPRV, KC_MPLY, KC_MNXT, KC_MUTE, KC_VOLD, KC_VOLU, KC_MAC_PRTA, 0x4C, 0x4A, 0x4D, 0x4B, 0x4E, 0, 0],
    // Row 1: GRV, 1-0, MINS, EQL, BSPC, NUM, PSLS, PAST, PMNS
    [0x35, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x2A, 0, 0, 0x53, 0x54, 0x55, 0x56, 0],
    // Row 2: TAB, Q-P, LBRC, RBRC, BSLS, P7, P8, P9, PPLS
    [0x2B, 0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x31, 0, 0, 0x5F, 0x60, 0x61, 0x57, 0],
    // Row 3: CAPS, A-L, SCLN, QUOT, ENT, P4, P5, P6
    [0x39, 0x04, 0x16, 0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x28, 0, 0, 0, 0x5C, 0x5D, 0x5E, 0, 0],
    // Row 4: LSHIFT, Z-M, COMM, DOT, SLSH, RSHIFT, UP, P1, P2, P3, PENT
    [0xE1, 0, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38, 0xE5, 0, 0, 0x52, 0x59, 0x5A, 0x5B, 0x58, 0],
    // Row 5: LCTRL, LALT, LGUI, SPACE, RGUI, MO(1), RCTRL, LEFT, DOWN, RIGHT, P0, PDOT
    [0xE0, 0xE2, 0xE3, 0, 0, 0, 0x2C, 0, 0, 0, 0xE7, MO | 1, 0xE4, 0, 0, 0x50, 0x51, 0x4F, 0x62, 0x63, 0],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 1: Mac Fn
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_MAC_FN: [[u16; 21]; 6] = [
    // Row 0: F1-F12, MAC_PRT, INS (transparent for others)
    [KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_MAC_PRT, 0x49, 0, 0, 0, 0, 0, 0],
    // Row 1: link keys on 1-4
    [KC_NO, KC_LNK_BLE1, KC_LNK_BLE2, KC_LNK_BLE3, KC_LNK_RF, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0, 0, 0, 0, 0, 0],
    // Row 2: DEV_RESET, SLEEP_MODE, BAT_SHOW
    [KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_DEV_RESET, KC_SLEEP_MODE, KC_BAT_SHOW, 0, 0, 0, 0, 0, 0, 0],
    // Row 3
    [KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0, 0, 0, 0, 0, 0, 0],
    // Row 4: MO(4) at 0,7,12; RGB controls at 8-9,14
    [MO | 4, 0, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, MO | 4, KC_RGB_SPD, KC_RGB_SPI, KC_NO, KC_NO, MO | 4, 0, KC_RGB_VAI, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0],
    // Row 5: RGB_MOD, RGB_VAD, RGB_HUI at 15-17
    [KC_NO, KC_NO, KC_NO, 0, 0, 0, KC_NO, 0, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0, KC_RGB_MOD, KC_RGB_VAD, KC_RGB_HUI, KC_NO, KC_NO, 0],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 2: Win base
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_WIN: [[u16; 21]; 6] = [
    // Row 0: ESC, F1-F12, PSCR, DEL, HOME, END, PGUP, PGDN
    [0x29, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x4C, 0x4A, 0x4D, 0x4B, 0x4E, 0, 0],
    // Row 1
    [0x35, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x2A, 0, 0, 0x53, 0x54, 0x55, 0x56, 0],
    // Row 2
    [0x2B, 0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x31, 0, 0, 0x5F, 0x60, 0x61, 0x57, 0],
    // Row 3
    [0x39, 0x04, 0x16, 0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x28, 0, 0, 0, 0x5C, 0x5D, 0x5E, 0, 0],
    // Row 4
    [0xE1, 0, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38, 0xE5, 0, 0, 0x52, 0x59, 0x5A, 0x5B, 0x58, 0],
    // Row 5: LCTRL, LGUI, LALT, SPACE, RALT, MO(3), RCTRL (Win layout: GUI before ALT)
    [0xE0, 0xE3, 0xE2, 0, 0, 0, 0x2C, 0, 0, 0, 0xE6, MO | 3, 0xE4, 0, 0, 0x50, 0x51, 0x4F, 0x62, 0x63, 0],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 3: Win Fn
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_WIN_FN: [[u16; 21]; 6] = [
    // Row 0: BRID, BRIU at 1-2; media keys at 7-12; MAC_PRTA at 13; INS at 14
    [KC_NO, KC_BRID, KC_BRIU, KC_NO, KC_NO, KC_NO, KC_NO, KC_MPRV, KC_MPLY, KC_MNXT, KC_MUTE, KC_VOLD, KC_VOLU, KC_MAC_PRTA, 0x49, 0, 0, 0, 0, 0, 0],
    // Row 1: link keys
    [KC_NO, KC_LNK_BLE1, KC_LNK_BLE2, KC_LNK_BLE3, KC_LNK_RF, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0, 0, 0, 0, 0, 0],
    // Row 2: DEV_RESET, SLEEP_MODE, BAT_SHOW
    [KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_DEV_RESET, KC_SLEEP_MODE, KC_BAT_SHOW, 0, 0, 0, 0, 0, 0, 0],
    // Row 3
    [KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0, 0, 0, 0, 0, 0, 0],
    // Row 4: MO(4) at 0,7,12; RGB controls at 8-9,14
    [MO | 4, 0, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, MO | 4, KC_RGB_SPD, KC_RGB_SPI, KC_NO, KC_NO, MO | 4, 0, KC_RGB_VAI, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0],
    // Row 5: RGB_MOD, RGB_VAD, RGB_HUI at 15-17
    [KC_NO, KC_NO, KC_NO, 0, 0, 0, KC_NO, 0, KC_NO, KC_NO, KC_NO, KC_NO, KC_NO, 0, 0, KC_RGB_MOD, KC_RGB_VAD, KC_RGB_HUI, KC_NO, KC_NO, 0],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 4: Function (side LED + RGB overrides)
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_FN: [[u16; 21]; 6] = [
    [KC_NO; 21],
    [KC_NO; 21],
    [KC_NO; 21],
    [KC_NO; 21],
    // Row 4: RGB_TEST at 4; SIDE_SPD/SPI at 9-10; SIDE_VAI at 15
    [KC_NO, 0, KC_NO, KC_NO, KC_RGB_TEST, KC_NO, KC_NO, KC_NO, KC_NO, KC_SIDE_SPD, KC_SIDE_SPI, KC_NO, KC_NO, 0, 0, KC_SIDE_VAI, KC_NO, KC_NO, KC_NO, KC_NO, 0],
    // Row 5: MO(4) at 11; SIDE_MOD/VAD/HUI at 15-17
    [KC_NO, KC_NO, KC_NO, 0, 0, 0, KC_NO, 0, KC_NO, KC_NO, KC_NO, MO | 4, KC_NO, 0, 0, KC_SIDE_MOD, KC_SIDE_VAD, KC_SIDE_HUI, KC_NO, KC_NO, 0],
];

/// Keymap lookup. Returns KC_NO (0x0000) for transparent keys.
pub fn get_keycode(layer: usize, row: usize, col: usize) -> u16 {
    if row >= 6 || col >= 21 || layer >= LAYER_COUNT { return KC_NO; }
    match layer {
        0 => LAYER_MAC[row][col],
        1 => LAYER_MAC_FN[row][col],
        2 => LAYER_WIN[row][col],
        3 => LAYER_WIN_FN[row][col],
        4 => LAYER_FN[row][col],
        _ => KC_NO,
    }
}

/// Resolve transparent keys: walk down from the active layer until a non-KC_NO key is found.
pub fn resolve_keycode(active_layers: &[usize], row: usize, col: usize) -> u16 {
    for &layer in active_layers.iter().rev() {
        let kc = get_keycode(layer, row, col);
        if kc != KC_NO { return kc; }
    }
    KC_NO
}
