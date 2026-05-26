//! Keymap engine — port of keymaps/default/keymap.c + ansi.h custom keycodes
//!
//! DO NOT MODIFY — 5-layer keymap aligned to physical matrix from keyboard.json.
//! Matrix positions are PCB-specific (column [0,1] is empty). Changing key
//! positions or layer indices without the physical matrix reference breaks
//! all key mappings. Verified working at v3.8.x.
//!
//! 5 layers: Mac base(0), Mac Fn(1), Win base(2), Win Fn(3), Function(4)
//! Physical matrix positions from keyboards/nuphy/air96_v2/keyboard.json LAYOUT:
//!   Row 0: ESC, [0,1]=empty, F-keys at 2-13, Print at 14
//!   Row 1: GRV..BSPC at 0-13, [1,14]=empty, HOME(15), PGUP(16), NUM(17)..
//!   Row 2: TAB..BSLS at 0-13, DEL(14), END(15), PGDN(16), P7(17)..
//!   Row 3: CAPS..QUOT at 0-11, [3,12]=empty, ENT(13), P4(17)..
//!   Row 4: LSFT..RSFT at 0-13 with gaps, UP(15), P1(17)..
//!   Row 5: LCTL..RGHT at 0-16 with gaps, P0(17), PDOT(19)

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
// Consumer keys
pub const KC_BRID: u16        = 0x5C18;
pub const KC_BRIU: u16        = 0x5C19;
pub const KC_MPRV: u16        = 0x5C1A;
pub const KC_MPLY: u16        = 0x5C1B;
pub const KC_MNXT: u16        = 0x5C1C;
pub const KC_MUTE: u16        = 0x5C1D;
pub const KC_VOLD: u16        = 0x5C1E;
pub const KC_VOLU: u16        = 0x5C1F;
// RGB matrix controls
pub const KC_RGB_SPD: u16     = 0x5C30;
pub const KC_RGB_SPI: u16     = 0x5C31;
pub const KC_RGB_VAI: u16     = 0x5C32;
pub const KC_RGB_VAD: u16     = 0x5C33;
pub const KC_RGB_MOD: u16     = 0x5C34;
pub const KC_RGB_HUI: u16     = 0x5C35;

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

pub fn is_custom(kc: u16) -> bool {
    kc >= 0x5C00
}

pub fn lnk_to_channel(kc: u16) -> LinkMode {
    match kc {
        KC_LNK_USB  => LinkMode::Usb,
        KC_LNK_RF   => LinkMode::Rf24,
        KC_LNK_BLE1 => LinkMode::Bt1,
        KC_LNK_BLE2 => LinkMode::Bt2,
        KC_LNK_BLE3 => LinkMode::Bt3,
        _ => LinkMode::Usb,
    }
}

// ── HID keycode aliases ────────────────────────────────────────────────
pub const KC_NO: u16    = 0x0000;
// Layer toggle
pub const MO: u16 = 0x5C20;
pub fn mo_layer(kc: u16) -> Option<usize> {
    if kc & 0xFFF0 == MO { Some((kc & 0x0F) as usize) } else { None }
}

// ═══════════════════════════════════════════════════════════════════════
// KEYMAP: 6 rows × 21 cols, aligned to keyboard.json LAYOUT
//
// Column index → physical pin:
//   0=A4 1=A5 2=A6 3=A7 4=B0 5=B1 6=B10 7=B11 8=B12 9=B13 10=B14
//   11=B15 12=A8 13=A9 14=A10 15=A15 16=B3 17=C10 18=C11 19=C12 20=D2
//
// Empty positions are KC_NO (0x0000).
// ═══════════════════════════════════════════════════════════════════════

/// Helper: empty row
const NR: u16 = KC_NO;

// ═══════════════════════════════════════════════════════════════════
// LAYER 0: Mac base
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_MAC: [[u16; 21]; 6] = [
    // Row 0: ESC, __, BRID, BRIU, MAC_TASK, SEARCH, VOICE, DND, MPRV, MPLY, MNXT, MUTE, VOLD, VOLU, MAC_PRTA
    [0x29, NR, KC_BRID, KC_BRIU, KC_MAC_TASK, KC_MAC_SEARCH, KC_MAC_VOICE, KC_MAC_DND,
     KC_MPRV, KC_MPLY, KC_MNXT, KC_MUTE, KC_VOLD, KC_VOLU, KC_MAC_PRTA,
     NR, NR, NR, NR, NR, NR],
    // Row 1: GRV, 1-0, MINS, EQL, BSPC, __, HOME, PGUP, NUM, PSLS, PAST, PMNS
    [0x35, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x2A,
     NR, 0x4A, 0x4B, 0x53, 0x54, 0x55, 0x56],
    // Row 2: TAB, Q-P, LBRC, RBRC, BSLS, DEL, END, PGDN, P7, P8, P9, PPLS
    [0x2B, 0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x31,
     0x4C, 0x4D, 0x4E, 0x5F, 0x60, 0x61, 0x57],
    // Row 3: CAPS, A-L, SCLN, QUOT, __, ENT, __, __, __, P4, P5, P6
    [0x39, 0x04, 0x16, 0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34,
     NR, 0x28,
     NR, NR, NR, 0x5C, 0x5D, 0x5E, NR],
    // Row 4: LSFT, __, Z-M, COMM, DOT, SLSH, __, RSFT, __, UP, __, P1, P2, P3, PENT
    [0xE1, NR, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38,
     NR, 0xE5,
     NR, 0x52, NR, 0x59, 0x5A, 0x5B, 0x58],
    // Row 5: LCTL, LALT, LGUI, __, __, __, SPC, __, __, RGUI, MO(1), __, __, RCTL, LEFT, DOWN, RGHT, P0, __, PDOT
    [0xE0, 0xE2, 0xE3, NR, NR, NR, 0x2C, NR, NR, 0xE7, MO | 1, NR, NR, 0xE4,
     0x50, 0x51, 0x4F, 0x62, NR, 0x63, NR],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 1: Mac Fn
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_MAC_FN: [[u16; 21]; 6] = [
    // Row 0: __, __, F1-F12, MAC_PRT, __(INS is at [2,14])
    [NR, NR, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, KC_MAC_PRT,
     NR, NR, NR, NR, NR, NR],
    // Row 1: __, LNK_BLE1, LNK_BLE2, LNK_BLE3, LNK_RF, __... (transparent for rest)
    [NR, KC_LNK_BLE1, KC_LNK_BLE2, KC_LNK_BLE3, KC_LNK_RF, NR, NR, NR, NR, NR, NR, NR, NR, NR,
     NR, NR, NR, NR, NR, NR, NR],
    // Row 2: __... DEV_RESET at 11, SLEEP_MODE at 12, RGB_MOD at 13, INS at 14
    [NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, KC_DEV_RESET, KC_SLEEP_MODE, KC_RGB_MOD,
     0x49, NR, NR, NR, NR, NR, NR],
    // Row 3: all transparent
    [NR; 21],
    // Row 4: MO(4) at 0,8,12; RGB_SPI at 9; RGB_VAI at 15 (fn+UP)
    [MO | 4, NR, NR, NR, NR, NR, NR, NR, MO | 4, KC_RGB_SPI, NR, NR, MO | 4,
     NR, NR, KC_RGB_VAI, NR, NR, NR, NR, NR],
    // Row 5: fn+LEFT=SPD at 14, fn+DOWN=VAD at 15, fn+RIGHT=SPI at 16, HUI at 17
    [NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR,
     KC_RGB_SPD, KC_RGB_VAD, KC_RGB_SPI, KC_RGB_HUI, NR, NR, NR],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 2: Win base
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_WIN: [[u16; 21]; 6] = [
    // Row 0: ESC, __, F1-F12, PSCR
    [0x29, NR, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
     NR, NR, NR, NR, NR, NR],
    // Row 1: same as Mac base
    [0x35, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x2A,
     NR, 0x4A, 0x4B, 0x53, 0x54, 0x55, 0x56],
    // Row 2: same as Mac base
    [0x2B, 0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x31,
     0x4C, 0x4D, 0x4E, 0x5F, 0x60, 0x61, 0x57],
    // Row 3: same as Mac base
    [0x39, 0x04, 0x16, 0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34,
     NR, 0x28,
     NR, NR, NR, 0x5C, 0x5D, 0x5E, NR],
    // Row 4: same as Mac base
    [0xE1, NR, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38,
     NR, 0xE5,
     NR, 0x52, NR, 0x59, 0x5A, 0x5B, 0x58],
    // Row 5: Win layout — LGUI before LALT, RALT instead of RGUI
    [0xE0, 0xE3, 0xE2, NR, NR, NR, 0x2C, NR, NR, 0xE6, MO | 3, NR, NR, 0xE4,
     0x50, 0x51, 0x4F, 0x62, NR, 0x63, NR],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 3: Win Fn
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_WIN_FN: [[u16; 21]; 6] = [
    // Row 0: __, __, BRID, BRIU, __, __, __, __, MPRV, MPLY, MNXT, MUTE, VOLD, VOLU, MAC_PRTA
    [NR, NR, KC_BRID, KC_BRIU, NR, NR, NR, NR, KC_MPRV, KC_MPLY, KC_MNXT, KC_MUTE, KC_VOLD, KC_VOLU, KC_MAC_PRTA,
     NR, NR, NR, NR, NR, NR],
    // Row 1: same as Mac Fn (link keys on 1-4)
    [NR, KC_LNK_BLE1, KC_LNK_BLE2, KC_LNK_BLE3, KC_LNK_RF, NR, NR, NR, NR, NR, NR, NR, NR, NR,
     NR, NR, NR, NR, NR, NR, NR],
    // Row 2: same as Mac Fn (DEV_RESET, SLEEP_MODE, RGB_MOD, INS)
    [NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, KC_DEV_RESET, KC_SLEEP_MODE, KC_RGB_MOD,
     0x49, NR, NR, NR, NR, NR, NR],
    // Row 3: all transparent
    [NR; 21],
    // Row 4: same as Mac Fn (MO(4), MO(4) at 8, RGB_SPI at 9, fn+UP=VAI)
    [MO | 4, NR, NR, NR, NR, NR, NR, NR, MO | 4, KC_RGB_SPI, NR, NR, MO | 4,
     NR, NR, KC_RGB_VAI, NR, NR, NR, NR, NR],
    // Row 5: fn+LEFT=SPD at 14, fn+DOWN=VAD at 15, fn+RIGHT=SPI at 16, HUI at 17
    [NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR,
     KC_RGB_SPD, KC_RGB_VAD, KC_RGB_SPI, KC_RGB_HUI, NR, NR, NR],
];

// ═══════════════════════════════════════════════════════════════════
// LAYER 4: Function (side LED + RGB overrides)
// ═══════════════════════════════════════════════════════════════════
pub const LAYER_FN: [[u16; 21]; 6] = [
    [NR; 21],
    [NR; 21],
    [NR; 21],
    [NR; 21],
    // Row 4: RGB_TEST at 4; SIDE_SPD at 9, SIDE_SPI at 10; SIDE_VAI at 15
    [NR, NR, NR, NR, KC_RGB_TEST, NR, NR, NR, NR, KC_SIDE_SPD, KC_SIDE_SPI, NR, NR,
     NR, NR, KC_SIDE_VAI, NR, NR, NR, NR, NR],
    // Row 5: MO(4) at 11; SIDE_MOD at 15, SIDE_VAD at 16, SIDE_HUI at 17
    [NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, NR, MO | 4, NR, NR, NR,
     KC_SIDE_MOD, KC_SIDE_VAD, KC_SIDE_HUI, NR, NR, NR],
];

// ── Keymap lookup ──────────────────────────────────────────────────────
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

pub fn resolve_keycode(active_layers: &[usize], row: usize, col: usize) -> u16 {
    for &layer in active_layers.iter().rev() {
        let kc = get_keycode(layer, row, col);
        if kc != KC_NO { return kc; }
    }
    KC_NO
}
