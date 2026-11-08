//! QMK action engine — 1:1 port of what the C VIA build compiles from
//! quantum/action.c, action_tapping.c, action_layer.c, action_util.c
//! (one-shot), keymap_common.c, keycode_config.c, mousekey.c, quantum.c
//! (process_record_quantum) and the enabled process_* features: key lock,
//! VIA macros, space cadet, magic, grave escape, tri layer.
//!
//! Sources were run through the C build's preprocessor flags, so only the
//! compiled branches are ported (no permissive hold / retro tapping, one-shot
//! timeouts off, TAPPING_TERM 200, TAPPING_TOGGLE 5, TAP_CODE_DELAY 8).

use crate::hal::Hal;
use crate::kb::Kb;
use crate::keyboard::keymap::*;

const TAPPING_TERM: u16 = 200;
const TAPPING_TOGGLE: u8 = 5;
const TAP_CODE_DELAY: u32 = 8;
const TAP_HOLD_CAPS_DELAY: u32 = 80;
const DYNAMIC_KEYMAP_MACRO_DELAY: u32 = 8;
const WAITING_BUFFER_SIZE: u8 = 8;
const MAX_LAYER: i8 = 8;

// enum action_kind_id
const ACT_LMODS: u16 = 0b0000;
const ACT_RMODS: u16 = 0b0001;
const ACT_LMODS_TAP: u16 = 0b0010;
const ACT_RMODS_TAP: u16 = 0b0011;
const ACT_USAGE: u16 = 0b0100;
const ACT_MOUSEKEY: u16 = 0b0101;
const ACT_LAYER: u16 = 0b1000;
const ACT_LAYER_MODS: u16 = 0b1001;
const ACT_LAYER_TAP: u16 = 0b1010;
const ACT_LAYER_TAP_EXT: u16 = 0b1011;
const ACT_SWAP_HANDS: u16 = 0b0110;

const MODS_ONESHOT: u8 = 0x00;
const MODS_TAP_TOGGLE: u8 = 0x01;
const OP_TAP_TOGGLE: u8 = 0xF0;
const OP_ON_OFF: u8 = 0xF1;
const OP_OFF_ON: u8 = 0xF2;
const OP_SET_CLEAR: u8 = 0xF3;
const OP_ONESHOT: u8 = 0xF4;
const OP_SH_TAP_TOGGLE: u8 = 0xF0;
const OP_BIT_AND: u8 = 0;
const OP_BIT_OR: u8 = 1;
const OP_BIT_XOR: u8 = 2;
const OP_BIT_SET: u8 = 3;
const ON_PRESS: u16 = 1;
const ON_RELEASE: u16 = 2;
const PAGE_SYSTEM: u8 = 0;
const PAGE_CONSUMER: u8 = 1;

const ONESHOT_PRESSED: u8 = 0b01;
const ONESHOT_OTHER_KEY_PRESSED: u8 = 0b10;
const ONESHOT_START: u8 = 0b11;

// 5-bit mod encoding (MOD_LCTL..MOD_RGUI)
const MOD_LCTL: u8 = 0x01;
const MOD_LALT: u8 = 0x04;
const MOD_LGUI: u8 = 0x08;
const MOD_RCTL: u8 = 0x11;
const MOD_RALT: u8 = 0x14;
const MOD_RGUI: u8 = 0x18;

// keymap_config_t bits
pub const KC_SWAP_CONTROL_CAPSLOCK: u16 = 1 << 0;
pub const KC_CAPSLOCK_TO_CONTROL: u16 = 1 << 1;
pub const KC_SWAP_LALT_LGUI: u16 = 1 << 2;
pub const KC_SWAP_RALT_RGUI: u16 = 1 << 3;
pub const KC_NO_GUI: u16 = 1 << 4;
pub const KC_SWAP_GRAVE_ESC: u16 = 1 << 5;
pub const KC_SWAP_BACKSLASH_BACKSPACE: u16 = 1 << 6;
pub const KC_NKRO: u16 = 1 << 7;
pub const KC_SWAP_LCTL_LGUI: u16 = 1 << 8;
pub const KC_SWAP_RCTL_RGUI: u16 = 1 << 9;
pub const KC_ONESHOT_ENABLE: u16 = 1 << 10;
pub const KC_SWAP_ESCAPE_CAPSLOCK: u16 = 1 << 11;
/// `eeconfig_init_quantum()`: one-shot and autocorrect enabled.
pub const KEYMAP_CONFIG_DEFAULT: u16 = 0x1400;

/// `keyevent_t`. `is_key` false = TICK_EVENT (QMK's NOEVENT).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub row: u8,
    pub col: u8,
    pub pressed: bool,
    pub time: u16,
    pub is_key: bool,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Tap {
    pub interrupted: bool,
    pub count: u8,
}

/// `keyrecord_t`.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct KeyRecord {
    pub event: KeyEvent,
    pub tap: Tap,
}

/// `action_t`.
#[derive(Clone, Copy, Debug)]
struct Action(u16);

impl Action {
    fn kind(self) -> u16 {
        self.0 >> 12
    }
    fn key_code(self) -> u8 {
        self.0 as u8
    }
    fn key_mods(self) -> u8 {
        ((self.0 >> 8) & 0x0F) as u8
    }
    fn lt_code(self) -> u8 {
        self.0 as u8
    }
    fn lt_val(self) -> u8 {
        ((self.0 >> 8) & 0x1F) as u8
    }
    fn usage_code(self) -> u16 {
        self.0 & 0x3FF
    }
    fn usage_page(self) -> u8 {
        ((self.0 >> 10) & 0x3) as u8
    }
    fn bitop_bits(self) -> u32 {
        (self.0 & 0x0F) as u32
    }
    fn bitop_xbit(self) -> bool {
        self.0 & 0x10 != 0
    }
    fn bitop_part(self) -> u8 {
        ((self.0 >> 5) & 0x7) as u8
    }
    fn bitop_on(self) -> u16 {
        (self.0 >> 8) & 0x3
    }
    fn bitop_op(self) -> u8 {
        ((self.0 >> 10) & 0x3) as u8
    }
    fn lm_mods(self) -> u8 {
        self.0 as u8
    }
    fn lm_layer(self) -> u8 {
        ((self.0 >> 8) & 0xF) as u8
    }
}

/// `report_mouse_t` (buttons, x, y, v, h).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct MouseReport {
    pub buttons: u8,
    pub x: i8,
    pub y: i8,
    pub v: i8,
    pub h: i8,
}

impl MouseReport {
    pub fn bytes(&self) -> [u8; 5] {
        [self.buttons, self.x as u8, self.y as u8, self.v as u8, self.h as u8]
    }
}

/// File statics of the ported QMK sources.
pub struct ActionState {
    /// `keymap_config.raw` (nkro bit lives in `Kb::nkro`).
    pub keymap_config: u16,
    pub oneshot_mods: u8,
    pub oneshot_locked_mods: u8,
    pub oneshot_layer_data: u8,
    source_layers_cache: [[u8; 21]; 6],
    disable_action_cache: bool,
    tapping_key: KeyRecord,
    waiting_buffer: [KeyRecord; WAITING_BUFFER_SIZE as usize],
    waiting_buffer_head: u8,
    waiting_buffer_tail: u8,
    key_lock_state: [u64; 4],
    key_lock_watching: bool,
    sc_last: u8,
    sc_timer: u16,
    grave_esc_was_shifted: bool,
    pub mouse_report: MouseReport,
    mousekey_accel: u8,
    mousekey_repeat: u8,
    mousekey_wheel_repeat: u8,
    last_timer_c: u16,
    last_timer_w: u16,
    last_tick: u16,
}

impl Default for ActionState {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionState {
    pub const fn new() -> Self {
        Self {
            keymap_config: KEYMAP_CONFIG_DEFAULT,
            oneshot_mods: 0,
            oneshot_locked_mods: 0,
            oneshot_layer_data: 0,
            source_layers_cache: [[0; 21]; 6],
            disable_action_cache: false,
            tapping_key: KeyRecord { event: KeyEvent { row: 0, col: 0, pressed: false, time: 0, is_key: false }, tap: Tap { interrupted: false, count: 0 } },
            waiting_buffer: [KeyRecord { event: KeyEvent { row: 0, col: 0, pressed: false, time: 0, is_key: false }, tap: Tap { interrupted: false, count: 0 } }; 8],
            waiting_buffer_head: 0,
            waiting_buffer_tail: 0,
            key_lock_state: [0; 4],
            key_lock_watching: false,
            sc_last: 0,
            sc_timer: 0,
            grave_esc_was_shifted: false,
            mouse_report: MouseReport { buttons: 0, x: 0, y: 0, v: 0, h: 0 },
            mousekey_accel: 0,
            mousekey_repeat: 0,
            mousekey_wheel_repeat: 0,
            last_timer_c: 0,
            last_timer_w: 0,
            last_tick: 0,
        }
    }

    fn kc(&self, bit: u16) -> bool {
        self.keymap_config & bit != 0
    }
    fn set_kc(&mut self, bit: u16, on: bool) {
        if on {
            self.keymap_config |= bit;
        } else {
            self.keymap_config &= !bit;
        }
    }
}

fn same_key(a: &KeyEvent, b: &KeyEvent) -> bool {
    a.row == b.row && a.col == b.col
}

fn is_modifier_keycode(code: u8) -> bool {
    (0xE0..=0xE7).contains(&code)
}

/// `times_inv_sqrt2()`.
fn times_inv_sqrt2(x: i8) -> i8 {
    let n = x as i16 * 181;
    let d: i16 = 256;
    (if n < 0 { (n - d / 2) / d } else { (n + d / 2) / d }) as i8
}

const MK_DELAY: u8 = 1;
const MK_INTERVAL: u8 = 20;
const MK_MAX_SPEED: u8 = 10;
const MK_TIME_TO_MAX: u8 = 30;
const MK_WHEEL_DELAY: u8 = 1;
const MK_WHEEL_INTERVAL: u8 = 80;
const MK_WHEEL_MAX_SPEED: u8 = 8;
const MK_WHEEL_TIME_TO_MAX: u8 = 40;
const MOUSEKEY_MOVE_DELTA: u16 = 8;
const MOUSEKEY_WHEEL_DELTA: u16 = 1;

impl Kb {
    // ════════════════════════════════════════════════════════════════════
    // keycode_config.c
    // ════════════════════════════════════════════════════════════════════

    fn keycode_config(&self, keycode: u16) -> u16 {
        let k = |b| self.act.kc(b);
        match keycode {
            KC_CAPS_LOCK | KC_LOCKING_CAPS_LOCK => {
                if k(KC_SWAP_CONTROL_CAPSLOCK) || k(KC_CAPSLOCK_TO_CONTROL) {
                    KC_LEFT_CTRL
                } else if k(KC_SWAP_ESCAPE_CAPSLOCK) {
                    KC_ESCAPE
                } else {
                    keycode
                }
            }
            KC_LEFT_CTRL => {
                if k(KC_SWAP_CONTROL_CAPSLOCK) {
                    return KC_CAPS_LOCK;
                }
                if k(KC_SWAP_LCTL_LGUI) {
                    return if k(KC_NO_GUI) { KC_NO } else { KC_LEFT_GUI };
                }
                KC_LEFT_CTRL
            }
            KC_LEFT_ALT => {
                if k(KC_SWAP_LALT_LGUI) {
                    return if k(KC_NO_GUI) { KC_NO } else { KC_LEFT_GUI };
                }
                KC_LEFT_ALT
            }
            KC_LEFT_GUI => {
                if k(KC_SWAP_LALT_LGUI) {
                    return KC_LEFT_ALT;
                }
                if k(KC_SWAP_LCTL_LGUI) {
                    return KC_LEFT_CTRL;
                }
                if k(KC_NO_GUI) {
                    return KC_NO;
                }
                KC_LEFT_GUI
            }
            KC_RIGHT_CTRL => {
                if k(KC_SWAP_RCTL_RGUI) {
                    return if k(KC_NO_GUI) { KC_NO } else { KC_RIGHT_GUI };
                }
                KC_RIGHT_CTRL
            }
            KC_RIGHT_ALT => {
                if k(KC_SWAP_RALT_RGUI) {
                    return if k(KC_NO_GUI) { KC_NO } else { KC_RIGHT_GUI };
                }
                KC_RIGHT_ALT
            }
            KC_RIGHT_GUI => {
                if k(KC_SWAP_RALT_RGUI) {
                    return KC_RIGHT_ALT;
                }
                if k(KC_SWAP_RCTL_RGUI) {
                    return KC_RIGHT_CTRL;
                }
                if k(KC_NO_GUI) {
                    return KC_NO;
                }
                KC_RIGHT_GUI
            }
            KC_GRAVE => {
                if k(KC_SWAP_GRAVE_ESC) {
                    KC_ESCAPE
                } else {
                    KC_GRAVE
                }
            }
            KC_ESCAPE => {
                if k(KC_SWAP_GRAVE_ESC) {
                    KC_GRAVE
                } else if k(KC_SWAP_ESCAPE_CAPSLOCK) {
                    KC_CAPS_LOCK
                } else {
                    KC_ESCAPE
                }
            }
            KC_BACKSLASH => {
                if k(KC_SWAP_BACKSLASH_BACKSPACE) {
                    KC_BACKSPACE
                } else {
                    KC_BACKSLASH
                }
            }
            KC_BACKSPACE => {
                if k(KC_SWAP_BACKSLASH_BACKSPACE) {
                    KC_BACKSLASH
                } else {
                    KC_BACKSPACE
                }
            }
            _ => keycode,
        }
    }

    fn mod_config(&self, mut m: u8) -> u8 {
        let k = |b| self.act.kc(b);
        if k(KC_SWAP_LALT_LGUI) && (((m & MOD_RALT) == MOD_LALT) ^ ((m & MOD_RGUI) == MOD_LGUI)) {
            m ^= MOD_LALT | MOD_LGUI;
        }
        if k(KC_SWAP_RALT_RGUI) && (((m & MOD_RALT) == MOD_RALT) ^ ((m & MOD_RGUI) == MOD_RGUI)) {
            m ^= MOD_LALT | MOD_LGUI;
        }
        if k(KC_SWAP_LCTL_LGUI) && (((m & MOD_RCTL) == MOD_LCTL) ^ ((m & MOD_RGUI) == MOD_LGUI)) {
            m ^= MOD_LCTL | MOD_LGUI;
        }
        if k(KC_SWAP_RCTL_RGUI) && (((m & MOD_RCTL) == MOD_RCTL) ^ ((m & MOD_RGUI) == MOD_RGUI)) {
            m ^= MOD_LCTL | MOD_LGUI;
        }
        if k(KC_NO_GUI) {
            m &= !MOD_LGUI;
            m &= !MOD_RGUI;
        }
        m
    }

    // ════════════════════════════════════════════════════════════════════
    // keymap_common.c
    // ════════════════════════════════════════════════════════════════════

    /// `keymap_key_to_keycode()` (VIA dynamic keymap).
    pub fn keymap_key_to_keycode(&self, layer: u8, row: u8, col: u8) -> u16 {
        if row < 6 && col < 21 {
            crate::via::dynamic_keymap_get_keycode(layer, row, col)
        } else {
            KC_NO
        }
    }

    fn action_for_key(&self, layer: u8, row: u8, col: u8) -> Action {
        self.action_for_keycode(self.keymap_key_to_keycode(layer, row, col))
    }

    fn action_for_keycode(&self, keycode: u16) -> Action {
        let keycode = self.keycode_config(keycode);
        let a = |kind: u16, param: u16| Action(kind << 12 | param);
        let layer = (keycode & 0x1F) as u16;
        let bitop = |op: u16, on: u16, l: u16| (op << 10) | (on << 8) | ((l / 4) << 5) | ((1 << (l % 4)) & 0x1F);
        match keycode {
            KC_A..=KC_EXSEL | KC_LEFT_CTRL..=KC_RIGHT_GUI => a(ACT_LMODS, keycode),
            KC_SYSTEM_POWER..=KC_SYSTEM_WAKE => a(ACT_USAGE, (PAGE_SYSTEM as u16) << 10 | system_usage(keycode)),
            KC_AUDIO_MUTE..=KC_LAUNCHPAD => a(ACT_USAGE, (PAGE_CONSUMER as u16) << 10 | consumer_usage(keycode)),
            KC_MS_UP..=KC_MS_ACCEL2 => a(ACT_MOUSEKEY, keycode),
            KC_TRNS => Action(1),
            QK_MODS..=QK_MODS_MAX => {
                let m = (self.mod_config(((keycode >> 8) & 0x1F) as u8) & 0x1F) as u16;
                a(ACT_LMODS, m << 8 | self.keycode_config(keycode & 0xFF))
            }
            QK_LAYER_TAP..=QK_LAYER_TAP_MAX => a(ACT_LAYER_TAP, ((keycode >> 8) & 0xF) << 8 | self.keycode_config(keycode & 0xFF)),
            QK_TO..=QK_TO_MAX => a(ACT_LAYER, bitop(OP_BIT_SET as u16, ON_PRESS, layer)),
            QK_MOMENTARY..=QK_MOMENTARY_MAX => a(ACT_LAYER_TAP, layer << 8 | OP_ON_OFF as u16),
            QK_DEF_LAYER..=QK_DEF_LAYER_MAX => a(ACT_LAYER, bitop(OP_BIT_SET as u16, 0, layer)),
            QK_TOGGLE_LAYER..=QK_TOGGLE_LAYER_MAX => a(ACT_LAYER, bitop(OP_BIT_XOR as u16, ON_RELEASE, layer)),
            QK_ONE_SHOT_LAYER..=QK_ONE_SHOT_LAYER_MAX => a(ACT_LAYER_TAP, layer << 8 | OP_ONESHOT as u16),
            QK_ONE_SHOT_MOD..=QK_ONE_SHOT_MOD_MAX => {
                let m = (self.mod_config((keycode & 0x1F) as u8) & 0x1F) as u16;
                a(ACT_LMODS_TAP, m << 8 | MODS_ONESHOT as u16)
            }
            QK_LAYER_TAP_TOGGLE..=QK_LAYER_TAP_TOGGLE_MAX => a(ACT_LAYER_TAP, layer << 8 | OP_TAP_TOGGLE as u16),
            QK_LAYER_MOD..=QK_LAYER_MOD_MAX => {
                let m = self.mod_config((keycode & 0x1F) as u8) as u16;
                let l = (keycode >> 5) & 0xF;
                // C int arithmetic: `mod << 4` can carry into the layer field.
                a(ACT_LAYER_MODS, (l << 8) | if m & 0x10 != 0 { m << 4 } else { m })
            }
            QK_MOD_TAP..=QK_MOD_TAP_MAX => {
                let m = (self.mod_config(((keycode >> 8) & 0x1F) as u8) & 0x1F) as u16;
                a(ACT_LMODS_TAP, m << 8 | self.keycode_config(keycode & 0xFF))
            }
            _ => Action(0),
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // action_layer.c
    // ════════════════════════════════════════════════════════════════════

    pub fn default_layer_set(&mut self, state: u32) {
        self.default_layer_state = state;
    }
    fn default_layer_or(&mut self, s: u32) {
        self.default_layer_state |= s;
    }
    fn default_layer_and(&mut self, s: u32) {
        self.default_layer_state &= s;
    }
    fn default_layer_xor(&mut self, s: u32) {
        self.default_layer_state ^= s;
    }
    pub fn layer_state_set(&mut self, state: u32) {
        self.layer_state = state;
    }
    fn layer_clear(&mut self) {
        self.layer_state_set(0);
    }
    fn layer_move(&mut self, layer: u8) {
        self.layer_state_set(1u32.wrapping_shl(layer as u32));
    }
    pub fn layer_on(&mut self, layer: u8) {
        self.layer_state_set(self.layer_state | 1u32.wrapping_shl(layer as u32));
    }
    pub fn layer_off(&mut self, layer: u8) {
        self.layer_state_set(self.layer_state & !1u32.wrapping_shl(layer as u32));
    }
    fn layer_invert(&mut self, layer: u8) {
        self.layer_state_set(self.layer_state ^ 1u32.wrapping_shl(layer as u32));
    }
    fn layer_or(&mut self, s: u32) {
        self.layer_state_set(self.layer_state | s);
    }
    fn layer_and(&mut self, s: u32) {
        self.layer_state_set(self.layer_state & s);
    }
    fn layer_xor(&mut self, s: u32) {
        self.layer_state_set(self.layer_state ^ s);
    }

    fn update_source_layers_cache(&mut self, row: u8, col: u8, layer: u8) {
        if row < 6 && col < 21 {
            self.act.source_layers_cache[row as usize][col as usize] = layer & 0x7;
        }
    }
    fn read_source_layers_cache(&self, row: u8, col: u8) -> u8 {
        if row < 6 && col < 21 {
            self.act.source_layers_cache[row as usize][col as usize]
        } else {
            0
        }
    }

    fn store_or_get_action(&mut self, pressed: bool, row: u8, col: u8) -> Action {
        if self.act.disable_action_cache {
            return self.layer_switch_get_action(row, col);
        }
        let layer = if pressed {
            let l = self.layer_switch_get_layer(row, col);
            self.update_source_layers_cache(row, col, l);
            l
        } else {
            self.read_source_layers_cache(row, col)
        };
        self.action_for_key(layer, row, col)
    }

    pub fn layer_switch_get_layer(&self, row: u8, col: u8) -> u8 {
        let layers = self.layer_state | self.default_layer_state;
        let mut i = MAX_LAYER - 1;
        while i >= 0 {
            if layers & (1 << i) != 0 && self.action_for_key(i as u8, row, col).0 != 1 {
                return i as u8;
            }
            i -= 1;
        }
        0
    }

    fn layer_switch_get_action(&self, row: u8, col: u8) -> Action {
        self.action_for_key(self.layer_switch_get_layer(row, col), row, col)
    }

    fn update_tri_layer(&mut self, l1: u8, l2: u8, l3: u8) {
        let mask12 = (1u32 << l1) | (1u32 << l2);
        let mask3 = 1u32 << l3;
        let s = self.layer_state;
        self.layer_state_set(if s & mask12 == mask12 { s | mask3 } else { s & !mask3 });
    }

    // ════════════════════════════════════════════════════════════════════
    // action_util.c — mods and one-shot
    // ════════════════════════════════════════════════════════════════════

    pub fn add_mods(&mut self, m: u8) {
        self.real_mods |= m;
    }
    pub fn del_mods(&mut self, m: u8) {
        self.real_mods &= !m;
    }
    pub fn add_weak_mods(&mut self, m: u8) {
        self.weak_mods |= m;
    }
    pub fn del_weak_mods(&mut self, m: u8) {
        self.weak_mods &= !m;
    }

    fn get_oneshot_layer(&self) -> u8 {
        self.act.oneshot_layer_data >> 3
    }
    fn get_oneshot_layer_state(&self) -> u8 {
        self.act.oneshot_layer_data & 0b111
    }
    fn is_oneshot_layer_active(&self) -> bool {
        self.get_oneshot_layer_state() != 0
    }
    fn oneshot_enabled(&self) -> bool {
        self.act.kc(KC_ONESHOT_ENABLE)
    }
    fn set_oneshot_layer(&mut self, layer: u8, state: u8) {
        if self.oneshot_enabled() {
            self.act.oneshot_layer_data = layer << 3 | state;
        }
        self.layer_on(layer);
    }
    fn reset_oneshot_layer(&mut self) {
        self.act.oneshot_layer_data = 0;
    }
    fn clear_oneshot_layer_state(&mut self, state: u8) {
        let start_state = self.act.oneshot_layer_data;
        self.act.oneshot_layer_data &= !state;
        if self.get_oneshot_layer_state() == 0 && start_state != self.act.oneshot_layer_data && self.oneshot_enabled() {
            let l = self.get_oneshot_layer();
            self.layer_off(l);
            self.reset_oneshot_layer();
        }
    }
    fn oneshot_set(&mut self, hal: &mut impl Hal, active: bool) {
        if self.oneshot_enabled() != active {
            self.act.set_kc(KC_ONESHOT_ENABLE, active);
            self.eeconfig_update_keymap(hal);
            self.clear_oneshot_layer_state(ONESHOT_OTHER_KEY_PRESSED);
        }
    }
    fn add_oneshot_mods(&mut self, m: u8) {
        self.act.oneshot_mods |= m;
    }
    fn del_oneshot_mods(&mut self, m: u8) {
        self.act.oneshot_mods &= !m;
    }
    fn del_oneshot_locked_mods(&mut self, m: u8) {
        self.act.oneshot_locked_mods &= !m;
    }
    pub fn clear_oneshot_mods(&mut self) {
        self.act.oneshot_mods = 0;
    }

    /// `get_mods_for_report()`: one-shot mods ride along until a key is down.
    pub fn get_mods_for_report(&mut self) -> u8 {
        let mut mods = self.real_mods | self.weak_mods;
        if self.act.oneshot_mods != 0 {
            mods |= self.act.oneshot_mods;
            if self.has_anykey() {
                self.clear_oneshot_mods();
            }
        }
        mods
    }

    fn register_mods(&mut self, hal: &mut impl Hal, m: u8) {
        if m != 0 {
            self.add_mods(m);
            self.send_keyboard_report(hal);
        }
    }
    fn unregister_mods(&mut self, hal: &mut impl Hal, m: u8) {
        if m != 0 {
            self.del_mods(m);
            self.send_keyboard_report(hal);
        }
    }
    /// `eeconfig_update_keymap(keymap_config.raw)`.
    fn eeconfig_update_keymap(&mut self, hal: &mut impl Hal) {
        let raw = (self.act.keymap_config & !KC_NKRO) | if self.nkro { KC_NKRO } else { 0 };
        hal.eeconfig_update_keymap(raw);
    }

    /// `keyboard_init()`: load keymap_config, force NKRO on and persist it.
    #[inline(never)]
    pub fn keymap_config_init(&mut self, hal: &mut impl Hal) {
        self.act.keymap_config = hal.eeconfig_read_keymap();
        self.nkro = true;
        self.eeconfig_update_keymap(hal);
    }

    // ════════════════════════════════════════════════════════════════════
    // action.c
    // ════════════════════════════════════════════════════════════════════

    /// `register_code()`.
    pub fn register_code(&mut self, hal: &mut impl Hal, code: u8) {
        let c = code as u16;
        if code == 0 {
        } else if (KC_A..=KC_EXSEL).contains(&c) {
            if self.is_key_pressed(code) {
                self.del_key(code);
                self.send_keyboard_report(hal);
            }
            self.add_key(code);
            self.send_keyboard_report(hal);
        } else if is_modifier_keycode(code) {
            self.add_mods(1 << (code & 0x07));
            self.send_keyboard_report(hal);
        } else if (KC_SYSTEM_POWER..=KC_SYSTEM_WAKE).contains(&c) {
            self.host_system_send(hal, system_usage(c));
        } else if (KC_AUDIO_MUTE..=KC_LAUNCHPAD).contains(&c) {
            self.host_consumer_send(hal, consumer_usage(c));
        } else if (KC_MS_UP..=KC_MS_ACCEL2).contains(&c) {
            self.register_mouse(hal, code, true);
        }
    }

    /// `unregister_code()`.
    pub fn unregister_code(&mut self, hal: &mut impl Hal, code: u8) {
        let c = code as u16;
        if code == 0 {
        } else if (KC_A..=KC_EXSEL).contains(&c) {
            self.del_key(code);
            self.send_keyboard_report(hal);
        } else if is_modifier_keycode(code) {
            self.del_mods(1 << (code & 0x07));
            self.send_keyboard_report(hal);
        } else if (KC_SYSTEM_POWER..=KC_SYSTEM_WAKE).contains(&c) {
            self.host_system_send(hal, 0);
        } else if (KC_AUDIO_MUTE..=KC_LAUNCHPAD).contains(&c) {
            self.host_consumer_send(hal, 0);
        } else if (KC_MS_UP..=KC_MS_ACCEL2).contains(&c) {
            self.register_mouse(hal, code, false);
        }
    }

    fn tap_code_delay(&mut self, hal: &mut impl Hal, code: u8, delay: u32) {
        self.register_code(hal, code);
        hal.wait_ms(delay.max(1));
        self.unregister_code(hal, code);
    }

    pub fn tap_code(&mut self, hal: &mut impl Hal, code: u8) {
        let d = if code as u16 == KC_CAPS_LOCK { TAP_HOLD_CAPS_DELAY } else { TAP_CODE_DELAY };
        self.tap_code_delay(hal, code, d);
    }

    fn register_mouse(&mut self, hal: &mut impl Hal, code: u8, pressed: bool) {
        if pressed {
            self.mousekey_on(code);
        } else {
            self.mousekey_off(code);
        }
        self.mousekey_send(hal);
    }

    /// `action_exec()`.
    pub fn action_exec(&mut self, hal: &mut impl Hal, event: KeyEvent) {
        if event.pressed {
            self.clear_weak_mods();
        }
        let record = KeyRecord { event, tap: Tap::default() };
        if event.is_key {
            // pre_process_record_quantum(): get_record_keycode(record, true)
            let _ = self.get_record_keycode(&record, true);
        }
        self.action_tapping_process(hal, record);
    }

    /// A debounced matrix change (`matrix_task()`): `action_exec()` then
    /// `switch_events()` (RGB matrix reactive hits).
    #[inline(never)]
    pub fn matrix_event(&mut self, hal: &mut impl Hal, row: u8, col: u8, pressed: bool) {
        if row as usize >= 6 || col as usize >= 21 {
            return;
        }
        let ev = KeyEvent { row, col, pressed, time: hal.now() as u16, is_key: true };
        self.action_exec(hal, ev);
        self.rgb.handle_key_event(row, col, pressed);
    }

    /// `generate_tick_event()`: run when the matrix didn't change.
    #[inline(never)]
    pub fn generate_tick_event(&mut self, hal: &mut impl Hal) {
        let now = hal.now() as u16;
        if now.wrapping_sub(self.act.last_tick) != 0 {
            self.action_exec(hal, KeyEvent { row: 0, col: 0, pressed: false, time: now, is_key: false });
            self.act.last_tick = now;
        }
    }

    fn process_record(&mut self, hal: &mut impl Hal, record: &mut KeyRecord) {
        if !record.event.is_key {
            return;
        }
        if !self.process_record_quantum(hal, record) {
            if self.is_oneshot_layer_active() && record.event.pressed && self.oneshot_enabled() {
                self.clear_oneshot_layer_state(ONESHOT_OTHER_KEY_PRESSED);
            }
            return;
        }
        let action = self.store_or_get_action(record.event.pressed, record.event.row, record.event.col);
        self.process_action(hal, record, action);
    }

    fn process_action(&mut self, hal: &mut impl Hal, record: &mut KeyRecord, action: Action) {
        let event = record.event;
        let tap_count = record.tap.count;
        let mut do_release_oneshot = false;
        let kind = action.kind();

        if self.is_oneshot_layer_active()
            && event.pressed
            && (kind == ACT_USAGE
                || !(is_modifier_keycode(action.key_code())
                    || ((kind == ACT_LMODS_TAP || kind == ACT_RMODS_TAP) && (action.lt_code() <= MODS_TAP_TOGGLE || tap_count == 0))))
            && self.oneshot_enabled()
        {
            self.clear_oneshot_layer_state(ONESHOT_OTHER_KEY_PRESSED);
            do_release_oneshot = !self.is_oneshot_layer_active();
        }

        match kind {
            ACT_LMODS | ACT_RMODS => {
                let mods = if kind == ACT_LMODS { action.key_mods() } else { action.key_mods() << 4 };
                self.mods_key(hal, event.pressed, mods, action.key_code());
            }
            ACT_LMODS_TAP | ACT_RMODS_TAP => {
                let mods = if kind == ACT_LMODS_TAP { action.key_mods() } else { action.key_mods() << 4 };
                match action.lt_code() {
                    MODS_ONESHOT => {
                        if !self.oneshot_enabled() {
                            self.mods_key(hal, event.pressed, mods, action.key_code());
                        } else if event.pressed {
                            if tap_count == 0 {
                                self.register_mods(hal, mods);
                            } else if tap_count == 1 {
                                self.add_oneshot_mods(mods);
                            }
                        } else if tap_count == 0 {
                            self.unregister_mods(hal, mods);
                            self.del_oneshot_mods(mods);
                            self.del_oneshot_locked_mods(mods);
                        }
                    }
                    MODS_TAP_TOGGLE => {
                        if event.pressed {
                            if tap_count <= TAPPING_TOGGLE {
                                self.register_mods(hal, mods);
                            }
                        } else if tap_count < TAPPING_TOGGLE {
                            self.unregister_mods(hal, mods);
                        }
                    }
                    _ => {
                        if event.pressed {
                            if tap_count > 0 {
                                self.register_code(hal, action.key_code());
                            } else {
                                self.register_mods(hal, mods);
                            }
                        } else if tap_count > 0 {
                            let d = if action.lt_code() as u16 == KC_CAPS_LOCK { TAP_HOLD_CAPS_DELAY } else { TAP_CODE_DELAY };
                            hal.wait_ms(d);
                            self.unregister_code(hal, action.key_code());
                        } else {
                            self.unregister_mods(hal, mods);
                        }
                    }
                }
            }
            ACT_USAGE => match action.usage_page() {
                PAGE_SYSTEM => self.host_system_send(hal, if event.pressed { action.usage_code() } else { 0 }),
                PAGE_CONSUMER => self.host_consumer_send(hal, if event.pressed { action.usage_code() } else { 0 }),
                _ => {}
            },
            ACT_MOUSEKEY => self.register_mouse(hal, action.key_code(), event.pressed),
            ACT_LAYER => {
                let shift = action.bitop_part() as u32 * 4;
                let bits = action.bitop_bits() << shift;
                let mask = if action.bitop_xbit() { !(0xFu32 << shift) } else { 0 };
                if action.bitop_on() == 0 {
                    if !event.pressed {
                        match action.bitop_op() {
                            OP_BIT_AND => self.default_layer_and(bits | mask),
                            OP_BIT_OR => self.default_layer_or(bits | mask),
                            OP_BIT_XOR => self.default_layer_xor(bits | mask),
                            _ => self.default_layer_set(bits | mask),
                        }
                    }
                } else if if event.pressed { action.bitop_on() & ON_PRESS != 0 } else { action.bitop_on() & ON_RELEASE != 0 } {
                    match action.bitop_op() {
                        OP_BIT_AND => self.layer_and(bits | mask),
                        OP_BIT_OR => self.layer_or(bits | mask),
                        OP_BIT_XOR => self.layer_xor(bits | mask),
                        _ => self.layer_state_set(bits | mask),
                    }
                }
            }
            ACT_LAYER_MODS => {
                if event.pressed {
                    self.layer_on(action.lm_layer());
                    self.register_mods(hal, action.lm_mods());
                } else {
                    self.unregister_mods(hal, action.lm_mods());
                    self.layer_off(action.lm_layer());
                }
            }
            ACT_LAYER_TAP | ACT_LAYER_TAP_EXT => match action.lt_code() {
                OP_TAP_TOGGLE => {
                    if event.pressed {
                        if tap_count < TAPPING_TOGGLE {
                            self.layer_invert(action.lt_val());
                        }
                    } else if tap_count <= TAPPING_TOGGLE {
                        self.layer_invert(action.lt_val());
                    }
                }
                OP_ON_OFF => {
                    if event.pressed {
                        self.layer_on(action.lt_val())
                    } else {
                        self.layer_off(action.lt_val())
                    }
                }
                OP_OFF_ON => {
                    if event.pressed {
                        self.layer_off(action.lt_val())
                    } else {
                        self.layer_on(action.lt_val())
                    }
                }
                OP_SET_CLEAR => {
                    if event.pressed {
                        self.layer_move(action.lt_val())
                    } else {
                        self.layer_clear()
                    }
                }
                OP_ONESHOT => {
                    if !self.oneshot_enabled() {
                        if event.pressed {
                            self.layer_on(action.lt_val());
                        } else {
                            self.layer_off(action.lt_val());
                        }
                    } else if event.pressed {
                        self.set_oneshot_layer(action.lt_val(), ONESHOT_START);
                    } else {
                        self.clear_oneshot_layer_state(ONESHOT_PRESSED);
                        if tap_count > 1 {
                            self.clear_oneshot_layer_state(ONESHOT_OTHER_KEY_PRESSED);
                        }
                    }
                }
                _ => {
                    if event.pressed {
                        if tap_count > 0 {
                            self.register_code(hal, action.lt_code());
                        } else {
                            self.layer_on(action.lt_val());
                        }
                    } else if tap_count > 0 {
                        let d = if action.lt_code() as u16 == KC_CAPS_LOCK { TAP_HOLD_CAPS_DELAY } else { TAP_CODE_DELAY };
                        hal.wait_ms(d);
                        self.unregister_code(hal, action.lt_code());
                    } else {
                        self.layer_off(action.lt_val());
                    }
                }
            },
            _ => {}
        }

        if do_release_oneshot && self.get_oneshot_layer_state() & ONESHOT_PRESSED == 0 {
            record.event.pressed = false;
            let l = self.get_oneshot_layer();
            self.layer_on(l);
            self.process_record(hal, record);
            let l = self.get_oneshot_layer();
            self.layer_off(l);
        }
    }

    /// ACT_LMODS/ACT_RMODS (and one-shot mods when one-shot is disabled).
    fn mods_key(&mut self, hal: &mut impl Hal, pressed: bool, mods: u8, code: u8) {
        let real = is_modifier_keycode(code) || code == 0;
        if pressed {
            if mods != 0 {
                if real {
                    self.add_mods(mods);
                } else {
                    self.add_weak_mods(mods);
                }
                self.send_keyboard_report(hal);
            }
            self.register_code(hal, code);
        } else {
            self.unregister_code(hal, code);
            if mods != 0 {
                if real {
                    self.del_mods(mods);
                } else {
                    self.del_weak_mods(mods);
                }
                self.send_keyboard_report(hal);
            }
        }
    }

    fn is_tap_record(&self, record: &KeyRecord) -> bool {
        if !record.event.is_key {
            return false;
        }
        let action = self.layer_switch_get_action(record.event.row, record.event.col);
        match action.kind() {
            ACT_LMODS_TAP | ACT_RMODS_TAP | ACT_LAYER_TAP | ACT_LAYER_TAP_EXT => {
                matches!(action.lt_code(), 0..=0xE7 | OP_TAP_TOGGLE | OP_ONESHOT)
            }
            ACT_SWAP_HANDS => matches!(action.lt_code(), 0..=0xE7 | OP_SH_TAP_TOGGLE),
            _ => false,
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // action_tapping.c
    // ════════════════════════════════════════════════════════════════════

    fn within_tapping_term(&self, e: &KeyEvent) -> bool {
        e.time.wrapping_sub(self.act.tapping_key.event.time) < TAPPING_TERM
    }

    fn action_tapping_process(&mut self, hal: &mut impl Hal, mut record: KeyRecord) {
        if !self.process_tapping(hal, &mut record) && !self.waiting_buffer_enq(record) {
            self.clear_keyboard(hal);
            self.waiting_buffer_clear();
            self.act.tapping_key = KeyRecord::default();
        }
        while self.act.waiting_buffer_tail != self.act.waiting_buffer_head {
            let t = self.act.waiting_buffer_tail as usize;
            let mut r = self.act.waiting_buffer[t];
            let done = self.process_tapping(hal, &mut r);
            self.act.waiting_buffer[t] = r;
            if !done {
                break;
            }
            self.act.waiting_buffer_tail = (self.act.waiting_buffer_tail + 1) % WAITING_BUFFER_SIZE;
        }
    }

    fn process_tapping(&mut self, hal: &mut impl Hal, keyp: &mut KeyRecord) -> bool {
        let event = keyp.event;

        if !self.act.tapping_key.event.is_key {
            if !event.is_key {
            } else if event.pressed && self.is_tap_record(keyp) {
                self.act.tapping_key = *keyp;
                let mut tk = self.act.tapping_key;
                self.process_record_tap_hint(&mut tk);
                self.waiting_buffer_scan_tap(hal);
            } else {
                self.process_record(hal, keyp);
            }
            return true;
        }

        if self.act.tapping_key.event.pressed {
            if self.within_tapping_term(&event) {
                if !event.is_key {
                    return true;
                }
                if self.act.tapping_key.tap.count == 0 {
                    if same_key(&self.act.tapping_key.event, &keyp.event) && !event.pressed {
                        // first tap
                        self.act.tapping_key.tap.count = 1;
                        let mut tk = self.act.tapping_key;
                        self.process_record(hal, &mut tk);
                        self.act.tapping_key = tk;
                        keyp.tap = self.act.tapping_key.tap;
                        return false;
                    } else if !event.pressed && !self.waiting_buffer_typed(&event) {
                        // release of a key pressed before the tapping key
                        let action = self.layer_switch_get_action(event.row, event.col);
                        match action.kind() {
                            ACT_LMODS | ACT_RMODS => {
                                if action.key_mods() != 0 && action.key_code() == 0 {
                                    return false;
                                }
                                if is_modifier_keycode(action.key_code()) {
                                    return false;
                                }
                            }
                            ACT_LMODS_TAP | ACT_RMODS_TAP => {
                                if action.key_mods() != 0 && keyp.tap.count == 0 {
                                    return false;
                                }
                                if is_modifier_keycode(action.key_code()) {
                                    return false;
                                }
                            }
                            ACT_LAYER_TAP | ACT_LAYER_TAP_EXT => {
                                if matches!(action.lt_code(), 0..=0xEF | OP_ON_OFF | OP_OFF_ON | OP_SET_CLEAR) {
                                    return false;
                                }
                            }
                            _ => {}
                        }
                        self.process_record(hal, keyp);
                        return true;
                    } else {
                        if event.pressed {
                            self.act.tapping_key.tap.interrupted = true;
                        }
                        return false;
                    }
                } else if same_key(&self.act.tapping_key.event, &keyp.event) && !event.pressed {
                    // tap release
                    keyp.tap = self.act.tapping_key.tap;
                    self.process_record(hal, keyp);
                    self.act.tapping_key = *keyp;
                    return true;
                } else if self.is_tap_record(keyp) && event.pressed {
                    if self.act.tapping_key.tap.count > 1 {
                        let tk = self.act.tapping_key;
                        let mut r = KeyRecord { tap: tk.tap, event: KeyEvent { pressed: false, time: event.time, ..tk.event } };
                        self.process_record(hal, &mut r);
                    }
                    self.act.tapping_key = *keyp;
                    self.waiting_buffer_scan_tap(hal);
                    return true;
                } else {
                    self.process_record(hal, keyp);
                    return true;
                }
            } else if self.act.tapping_key.tap.count == 0 {
                // after TAPPING_TERM: a hold
                let mut tk = self.act.tapping_key;
                self.process_record(hal, &mut tk);
                self.act.tapping_key = KeyRecord::default();
                return false;
            } else {
                if !event.is_key {
                    return true;
                }
                if same_key(&self.act.tapping_key.event, &keyp.event) && !event.pressed {
                    keyp.tap = self.act.tapping_key.tap;
                    self.process_record(hal, keyp);
                    self.act.tapping_key = KeyRecord::default();
                    return true;
                } else if self.is_tap_record(keyp) && event.pressed {
                    if self.act.tapping_key.tap.count > 1 {
                        let tk = self.act.tapping_key;
                        let mut r = KeyRecord { tap: tk.tap, event: KeyEvent { pressed: false, time: event.time, ..tk.event } };
                        self.process_record(hal, &mut r);
                    }
                    self.act.tapping_key = *keyp;
                    self.waiting_buffer_scan_tap(hal);
                    return true;
                } else {
                    self.process_record(hal, keyp);
                    return true;
                }
            }
        } else if self.within_tapping_term(&event) {
            // tapping key released
            if !event.is_key {
                return true;
            }
            if event.pressed {
                if same_key(&self.act.tapping_key.event, &keyp.event) {
                    if self.within_tapping_term(&event) && !self.act.tapping_key.tap.interrupted && self.act.tapping_key.tap.count > 0 {
                        // sequential tap
                        keyp.tap = self.act.tapping_key.tap;
                        if keyp.tap.count < 15 {
                            keyp.tap.count += 1;
                        }
                        self.process_record(hal, keyp);
                        self.act.tapping_key = *keyp;
                        return true;
                    }
                    self.act.tapping_key = *keyp;
                    true
                } else if self.is_tap_record(keyp) {
                    self.act.tapping_key = *keyp;
                    self.waiting_buffer_scan_tap(hal);
                    true
                } else {
                    self.act.tapping_key.tap.interrupted = true;
                    self.process_record(hal, keyp);
                    true
                }
            } else {
                self.process_record(hal, keyp);
                true
            }
        } else {
            self.act.tapping_key = KeyRecord::default();
            false
        }
    }

    /// `process_record_tap_hint()`: no tap-hint actions are compiled in.
    fn process_record_tap_hint(&self, _record: &mut KeyRecord) {}

    fn waiting_buffer_enq(&mut self, record: KeyRecord) -> bool {
        if !record.event.is_key {
            return true;
        }
        if (self.act.waiting_buffer_head + 1) % WAITING_BUFFER_SIZE == self.act.waiting_buffer_tail {
            return false;
        }
        self.act.waiting_buffer[self.act.waiting_buffer_head as usize] = record;
        self.act.waiting_buffer_head = (self.act.waiting_buffer_head + 1) % WAITING_BUFFER_SIZE;
        true
    }

    fn waiting_buffer_clear(&mut self) {
        self.act.waiting_buffer_head = 0;
        self.act.waiting_buffer_tail = 0;
    }

    fn waiting_buffer_typed(&self, event: &KeyEvent) -> bool {
        let mut i = self.act.waiting_buffer_tail;
        while i != self.act.waiting_buffer_head {
            let w = &self.act.waiting_buffer[i as usize].event;
            if same_key(event, w) && event.pressed != w.pressed {
                return true;
            }
            i = (i + 1) % WAITING_BUFFER_SIZE;
        }
        false
    }

    fn waiting_buffer_scan_tap(&mut self, hal: &mut impl Hal) {
        if self.act.tapping_key.tap.count > 0 || !self.act.tapping_key.event.pressed {
            return;
        }
        let mut i = self.act.waiting_buffer_tail;
        while i != self.act.waiting_buffer_head {
            let c = self.act.waiting_buffer[i as usize];
            if c.event.is_key && same_key(&c.event, &self.act.tapping_key.event) && !c.event.pressed && self.within_tapping_term(&c.event) {
                self.act.tapping_key.tap.count = 1;
                self.act.waiting_buffer[i as usize].tap.count = 1;
                let mut tk = self.act.tapping_key;
                self.process_record(hal, &mut tk);
                self.act.tapping_key = tk;
                return;
            }
            i = (i + 1) % WAITING_BUFFER_SIZE;
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // quantum.c — process_record_quantum()
    // ════════════════════════════════════════════════════════════════════

    fn get_record_keycode(&mut self, record: &KeyRecord, update_layer_cache: bool) -> u16 {
        let e = record.event;
        if !self.act.disable_action_cache {
            let layer = if e.pressed && update_layer_cache {
                let l = self.layer_switch_get_layer(e.row, e.col);
                self.update_source_layers_cache(e.row, e.col, l);
                l
            } else {
                self.read_source_layers_cache(e.row, e.col)
            };
            self.keymap_key_to_keycode(layer, e.row, e.col)
        } else {
            self.keymap_key_to_keycode(self.layer_switch_get_layer(e.row, e.col), e.row, e.col)
        }
    }

    fn process_record_quantum(&mut self, hal: &mut impl Hal, record: &mut KeyRecord) -> bool {
        let mut keycode = self.get_record_keycode(record, true);
        let pressed = record.event.pressed;
        if !(self.process_key_lock(&mut keycode, pressed)
            && self.process_record_via(hal, keycode, pressed)
            && self.process_record_kb(hal, keycode, pressed)
            && self.process_space_cadet(hal, keycode, pressed)
            && self.process_magic(hal, keycode, pressed)
            && self.process_grave_esc(hal, keycode, pressed)
            && self.process_rgb(hal, keycode, pressed)
            && self.process_tri_layer(keycode, pressed))
        {
            return false;
        }
        if pressed {
            match keycode {
                QK_BOOT => {
                    self.shutdown_quantum(hal);
                    hal.bootloader_jump();
                    return false;
                }
                QK_REBOOT => {
                    self.shutdown_quantum(hal);
                    hal.mcu_reset();
                    return false;
                }
                QK_DEBUG_TOGGLE => return false,
                QK_CLEAR_EEPROM => {
                    hal.eeconfig_init();
                    self.shutdown_quantum(hal);
                    hal.mcu_reset();
                    return false;
                }
                QK_ONE_SHOT_TOGGLE => {
                    let on = !self.oneshot_enabled();
                    self.oneshot_set(hal, on);
                }
                QK_ONE_SHOT_ON => self.oneshot_set(hal, true),
                QK_ONE_SHOT_OFF => self.oneshot_set(hal, false),
                _ => {}
            }
        }
        true
    }

    /// `shutdown_quantum()`.
    fn shutdown_quantum(&mut self, hal: &mut impl Hal) {
        self.clear_keyboard(hal);
        hal.wait_ms(250);
    }

    /// `process_key_lock()`.
    fn process_key_lock(&mut self, keycode: &mut u16, pressed: bool) -> bool {
        let tk = if *keycode > QK_ONE_SHOT_MOD && *keycode <= QK_ONE_SHOT_MOD_MAX { *keycode ^ QK_ONE_SHOT_MOD } else { *keycode };
        let standard = tk <= 0xFF;
        let bit = |k: u16| (k / 64) as usize;
        let mask = |k: u16| 1u64 << (k % 64);
        let held = |s: &[u64; 4], k: u16| s[bit(k)] & mask(k) == mask(k);
        if pressed {
            if !(standard || tk == QK_LOCK) {
                self.act.key_lock_watching = false;
                return true;
            }
            if tk == QK_LOCK {
                self.act.key_lock_watching = !self.act.key_lock_watching;
                return false;
            }
            if standard {
                if self.act.key_lock_watching {
                    self.act.key_lock_watching = false;
                    self.act.key_lock_state[bit(tk)] |= mask(tk);
                    *keycode = tk;
                    return true;
                }
                if held(&self.act.key_lock_state, tk) {
                    self.act.key_lock_state[bit(tk)] &= !mask(tk);
                    return false;
                }
            }
            true
        } else {
            !(standard && held(&self.act.key_lock_state, tk))
        }
    }

    /// `cancel_key_lock()`.
    pub fn cancel_key_lock(&mut self) {
        self.act.key_lock_watching = false;
        self.act.key_lock_state[0] &= !1;
    }

    /// `process_record_via()`: dynamic macros.
    fn process_record_via(&mut self, hal: &mut impl Hal, keycode: u16, pressed: bool) -> bool {
        if pressed && (QK_MACRO..=QK_MACRO_MAX).contains(&keycode) {
            let id = (keycode - QK_MACRO) as u8;
            crate::via::dynamic_keymap_macro_send(id, |step| self.send_macro_step(hal, step));
            return false;
        }
        true
    }

    /// One `send_string_with_delay()` step with DYNAMIC_KEYMAP_MACRO_DELAY.
    fn send_macro_step(&mut self, hal: &mut impl Hal, step: crate::via::MacroStep) {
        use crate::via::MacroStep::*;
        let interval = DYNAMIC_KEYMAP_MACRO_DELAY;
        match step {
            Tap(k) => self.tap_code(hal, k),
            Down(k) => self.register_code(hal, k),
            Up(k) => self.unregister_code(hal, k),
            Delay(ms) => hal.wait_ms(ms),
            Char(c) => {
                // send_char_with_delay()
                let (keycode, shifted) = if c < 128 { (crate::via::ASCII_TO_KEYCODE_LUT[c as usize], crate::via::ascii_shifted(c)) } else { (0, false) };
                if shifted {
                    self.register_code(hal, KC_LEFT_SHIFT as u8);
                    hal.wait_ms(interval);
                }
                self.tap_code_delay(hal, keycode, interval);
                hal.wait_ms(interval);
                if shifted {
                    self.unregister_code(hal, KC_LEFT_SHIFT as u8);
                    hal.wait_ms(interval);
                }
                return;
            }
        }
        hal.wait_ms(interval);
    }

    /// `process_space_cadet()`.
    fn process_space_cadet(&mut self, hal: &mut impl Hal, keycode: u16, pressed: bool) -> bool {
        let (hold, tap, key) = match keycode {
            QK_SPACE_CADET_LEFT_SHIFT_PARENTHESIS_OPEN => (KC_LEFT_SHIFT, KC_LEFT_SHIFT, KC_9),
            QK_SPACE_CADET_RIGHT_SHIFT_PARENTHESIS_CLOSE => (KC_RIGHT_SHIFT, KC_RIGHT_SHIFT, KC_0),
            QK_SPACE_CADET_LEFT_CTRL_PARENTHESIS_OPEN => (KC_LEFT_CTRL, KC_LEFT_SHIFT, KC_9),
            QK_SPACE_CADET_RIGHT_CTRL_PARENTHESIS_CLOSE => (KC_RIGHT_CTRL, KC_RIGHT_SHIFT, KC_0),
            QK_SPACE_CADET_LEFT_ALT_PARENTHESIS_OPEN => (KC_LEFT_ALT, KC_LEFT_SHIFT, KC_9),
            QK_SPACE_CADET_RIGHT_ALT_PARENTHESIS_CLOSE => (KC_RIGHT_ALT, KC_RIGHT_SHIFT, KC_0),
            QK_SPACE_CADET_RIGHT_SHIFT_ENTER => (KC_RIGHT_SHIFT, KC_TRNS, KC_ENTER),
            _ => {
                if pressed {
                    self.act.sc_last = 0;
                }
                return true;
            }
        };
        let (hold, tap, key) = (hold as u8, tap as u8, key as u8);
        if pressed {
            self.act.sc_last = hold;
            self.act.sc_timer = hal.now() as u16;
            if is_modifier_keycode(hold) {
                self.register_mods(hal, 1 << (hold & 7));
            }
        } else if self.act.sc_last == hold && (hal.now() as u16).wrapping_sub(self.act.sc_timer) < TAPPING_TERM {
            if hold != tap {
                if is_modifier_keycode(hold) {
                    self.unregister_mods(hal, 1 << (hold & 7));
                }
                if is_modifier_keycode(tap) {
                    self.register_mods(hal, 1 << (tap & 7));
                }
            }
            self.tap_code(hal, key);
            if is_modifier_keycode(tap) {
                self.unregister_mods(hal, 1 << (tap & 7));
            }
        } else if is_modifier_keycode(hold) {
            self.unregister_mods(hal, 1 << (hold & 7));
        }
        false
    }

    /// `process_magic()`: keymap_config switches (persisted).
    fn process_magic(&mut self, hal: &mut impl Hal, keycode: u16, pressed: bool) -> bool {
        if !(pressed && (QK_MAGIC_SWAP_CONTROL_CAPS_LOCK..=QK_MAGIC_TOGGLE_ESCAPE_CAPS_LOCK).contains(&keycode)) {
            return true;
        }
        let raw = hal.eeconfig_read_keymap();
        self.act.keymap_config = raw;
        self.nkro = raw & KC_NKRO != 0;
        match keycode {
            QK_MAGIC_NKRO_ON | QK_MAGIC_NKRO_OFF | QK_MAGIC_TOGGLE_NKRO => {
                self.clear_keyboard(hal);
                self.nkro = match keycode {
                    QK_MAGIC_NKRO_ON => true,
                    QK_MAGIC_NKRO_OFF => false,
                    _ => !self.nkro,
                };
            }
            _ => {}
        }
        let a = &mut self.act;
        let flip = |a: &mut ActionState, b: u16| {
            let on = !a.kc(b);
            a.set_kc(b, on);
        };
        match keycode {
            QK_MAGIC_SWAP_CONTROL_CAPS_LOCK => a.set_kc(KC_SWAP_CONTROL_CAPSLOCK, true),
            QK_MAGIC_SWAP_ESCAPE_CAPS_LOCK => a.set_kc(KC_SWAP_ESCAPE_CAPSLOCK, true),
            QK_MAGIC_CAPS_LOCK_AS_CONTROL_ON => a.set_kc(KC_CAPSLOCK_TO_CONTROL, true),
            QK_MAGIC_SWAP_LALT_LGUI => a.set_kc(KC_SWAP_LALT_LGUI, true),
            QK_MAGIC_SWAP_RALT_RGUI => a.set_kc(KC_SWAP_RALT_RGUI, true),
            QK_MAGIC_SWAP_LCTL_LGUI => a.set_kc(KC_SWAP_LCTL_LGUI, true),
            QK_MAGIC_SWAP_RCTL_RGUI => a.set_kc(KC_SWAP_RCTL_RGUI, true),
            QK_MAGIC_GUI_OFF => a.set_kc(KC_NO_GUI, true),
            QK_MAGIC_SWAP_GRAVE_ESC => a.set_kc(KC_SWAP_GRAVE_ESC, true),
            QK_MAGIC_SWAP_BACKSLASH_BACKSPACE => a.set_kc(KC_SWAP_BACKSLASH_BACKSPACE, true),
            QK_MAGIC_SWAP_ALT_GUI => {
                a.set_kc(KC_SWAP_LALT_LGUI, true);
                a.set_kc(KC_SWAP_RALT_RGUI, true);
            }
            QK_MAGIC_SWAP_CTL_GUI => {
                a.set_kc(KC_SWAP_LCTL_LGUI, true);
                a.set_kc(KC_SWAP_RCTL_RGUI, true);
            }
            QK_MAGIC_UNSWAP_CONTROL_CAPS_LOCK => a.set_kc(KC_SWAP_CONTROL_CAPSLOCK, false),
            QK_MAGIC_UNSWAP_ESCAPE_CAPS_LOCK => a.set_kc(KC_SWAP_ESCAPE_CAPSLOCK, false),
            QK_MAGIC_CAPS_LOCK_AS_CONTROL_OFF => a.set_kc(KC_CAPSLOCK_TO_CONTROL, false),
            QK_MAGIC_UNSWAP_LALT_LGUI => a.set_kc(KC_SWAP_LALT_LGUI, false),
            QK_MAGIC_UNSWAP_RALT_RGUI => a.set_kc(KC_SWAP_RALT_RGUI, false),
            QK_MAGIC_UNSWAP_LCTL_LGUI => a.set_kc(KC_SWAP_LCTL_LGUI, false),
            QK_MAGIC_UNSWAP_RCTL_RGUI => a.set_kc(KC_SWAP_RCTL_RGUI, false),
            QK_MAGIC_GUI_ON => a.set_kc(KC_NO_GUI, false),
            QK_MAGIC_UNSWAP_GRAVE_ESC => a.set_kc(KC_SWAP_GRAVE_ESC, false),
            QK_MAGIC_UNSWAP_BACKSLASH_BACKSPACE => a.set_kc(KC_SWAP_BACKSLASH_BACKSPACE, false),
            QK_MAGIC_UNSWAP_ALT_GUI => {
                a.set_kc(KC_SWAP_LALT_LGUI, false);
                a.set_kc(KC_SWAP_RALT_RGUI, false);
            }
            QK_MAGIC_UNSWAP_CTL_GUI => {
                a.set_kc(KC_SWAP_LCTL_LGUI, false);
                a.set_kc(KC_SWAP_RCTL_RGUI, false);
            }
            QK_MAGIC_TOGGLE_ALT_GUI => {
                flip(a, KC_SWAP_LALT_LGUI);
                let on = a.kc(KC_SWAP_LALT_LGUI);
                a.set_kc(KC_SWAP_RALT_RGUI, on);
            }
            QK_MAGIC_TOGGLE_CTL_GUI => {
                flip(a, KC_SWAP_LCTL_LGUI);
                let on = a.kc(KC_SWAP_LCTL_LGUI);
                a.set_kc(KC_SWAP_RCTL_RGUI, on);
            }
            QK_MAGIC_TOGGLE_BACKSLASH_BACKSPACE => flip(a, KC_SWAP_BACKSLASH_BACKSPACE),
            // EE_HANDS handedness: split keyboards only; the byte is unused here.
            QK_MAGIC_EE_HANDS_LEFT | QK_MAGIC_EE_HANDS_RIGHT => {}
            QK_MAGIC_TOGGLE_GUI => flip(a, KC_NO_GUI),
            QK_MAGIC_TOGGLE_CONTROL_CAPS_LOCK => flip(a, KC_SWAP_CONTROL_CAPSLOCK),
            QK_MAGIC_TOGGLE_ESCAPE_CAPS_LOCK => flip(a, KC_SWAP_ESCAPE_CAPSLOCK),
            _ => {}
        }
        self.eeconfig_update_keymap(hal);
        self.clear_keyboard(hal);
        false
    }

    /// `process_grave_esc()`.
    fn process_grave_esc(&mut self, hal: &mut impl Hal, keycode: u16, pressed: bool) -> bool {
        if keycode != QK_GRAVE_ESCAPE {
            return true;
        }
        let shifted = self.real_mods & (0x22 | 0x88) != 0;
        if pressed {
            self.act.grave_esc_was_shifted = shifted;
            self.add_key(if shifted { KC_GRAVE as u8 } else { KC_ESCAPE as u8 });
        } else {
            let was = self.act.grave_esc_was_shifted;
            self.del_key(if was { KC_GRAVE as u8 } else { KC_ESCAPE as u8 });
        }
        self.send_keyboard_report(hal);
        false
    }

    /// `process_tri_layer()` (lower 1, upper 2, adjust 3).
    fn process_tri_layer(&mut self, keycode: u16, pressed: bool) -> bool {
        let layer = match keycode {
            QK_TRI_LAYER_LOWER => 1,
            QK_TRI_LAYER_UPPER => 2,
            _ => return true,
        };
        if pressed {
            self.layer_on(layer);
        } else {
            self.layer_off(layer);
        }
        self.update_tri_layer(1, 2, 3);
        false
    }

    // ════════════════════════════════════════════════════════════════════
    // mousekey.c
    // ════════════════════════════════════════════════════════════════════

    fn move_unit(&self) -> i8 {
        let a = &self.act;
        let unit: u16 = if a.mousekey_accel & 1 != 0 {
            (MOUSEKEY_MOVE_DELTA * MK_MAX_SPEED as u16) / 4
        } else if a.mousekey_accel & 2 != 0 {
            (MOUSEKEY_MOVE_DELTA * MK_MAX_SPEED as u16) / 2
        } else if a.mousekey_accel & 4 != 0 {
            MOUSEKEY_MOVE_DELTA * MK_MAX_SPEED as u16
        } else if a.mousekey_repeat == 0 {
            MOUSEKEY_MOVE_DELTA
        } else if a.mousekey_repeat >= MK_TIME_TO_MAX {
            MOUSEKEY_MOVE_DELTA * MK_MAX_SPEED as u16
        } else {
            (MOUSEKEY_MOVE_DELTA * MK_MAX_SPEED as u16 * a.mousekey_repeat as u16) / MK_TIME_TO_MAX as u16
        };
        (if unit > 127 { 127 } else if unit == 0 { 1 } else { unit }) as i8
    }

    fn wheel_unit(&self) -> i8 {
        let a = &self.act;
        let unit: u16 = if a.mousekey_accel & 1 != 0 {
            (MOUSEKEY_WHEEL_DELTA * MK_WHEEL_MAX_SPEED as u16) / 4
        } else if a.mousekey_accel & 2 != 0 {
            (MOUSEKEY_WHEEL_DELTA * MK_WHEEL_MAX_SPEED as u16) / 2
        } else if a.mousekey_accel & 4 != 0 {
            MOUSEKEY_WHEEL_DELTA * MK_WHEEL_MAX_SPEED as u16
        } else if a.mousekey_wheel_repeat == 0 {
            MOUSEKEY_WHEEL_DELTA
        } else if a.mousekey_wheel_repeat >= MK_WHEEL_TIME_TO_MAX {
            MOUSEKEY_WHEEL_DELTA * MK_WHEEL_MAX_SPEED as u16
        } else {
            (MOUSEKEY_WHEEL_DELTA * MK_WHEEL_MAX_SPEED as u16 * a.mousekey_wheel_repeat as u16) / MK_WHEEL_TIME_TO_MAX as u16
        };
        (if unit > 127 { 127 } else if unit == 0 { 1 } else { unit }) as i8
    }

    /// `mousekey_task()`: acceleration / repeat while a mouse key is held.
    #[inline(never)]
    pub fn mousekey_task(&mut self, hal: &mut impl Hal) {
        let now = hal.now() as u16;
        let tmpmr = self.act.mouse_report;
        let mr = &mut self.act.mouse_report;
        mr.x = 0;
        mr.y = 0;
        mr.v = 0;
        mr.h = 0;
        let interval = if self.act.mousekey_repeat != 0 { MK_INTERVAL as u16 } else { MK_DELAY as u16 * 10 };
        if (tmpmr.x != 0 || tmpmr.y != 0) && now.wrapping_sub(self.act.last_timer_c) > interval {
            if self.act.mousekey_repeat != u8::MAX {
                self.act.mousekey_repeat += 1;
            }
            let u = self.move_unit();
            if tmpmr.x != 0 {
                self.act.mouse_report.x = u.wrapping_mul(if tmpmr.x > 0 { 1 } else { -1 });
            }
            if tmpmr.y != 0 {
                self.act.mouse_report.y = u.wrapping_mul(if tmpmr.y > 0 { 1 } else { -1 });
            }
            let mr = &mut self.act.mouse_report;
            if mr.x != 0 && mr.y != 0 {
                mr.x = times_inv_sqrt2(mr.x);
                if mr.x == 0 {
                    mr.x = 1;
                }
                mr.y = times_inv_sqrt2(mr.y);
                if mr.y == 0 {
                    mr.y = 1;
                }
            }
        }
        let interval = if self.act.mousekey_wheel_repeat != 0 { MK_WHEEL_INTERVAL as u16 } else { MK_WHEEL_DELAY as u16 * 10 };
        if (tmpmr.v != 0 || tmpmr.h != 0) && now.wrapping_sub(self.act.last_timer_w) > interval {
            if self.act.mousekey_wheel_repeat != u8::MAX {
                self.act.mousekey_wheel_repeat += 1;
            }
            let u = self.wheel_unit();
            if tmpmr.v != 0 {
                self.act.mouse_report.v = u.wrapping_mul(if tmpmr.v > 0 { 1 } else { -1 });
            }
            if tmpmr.h != 0 {
                self.act.mouse_report.h = u.wrapping_mul(if tmpmr.h > 0 { 1 } else { -1 });
            }
            let mr = &mut self.act.mouse_report;
            if mr.v != 0 && mr.h != 0 {
                mr.v = times_inv_sqrt2(mr.v);
                if mr.v == 0 {
                    mr.v = 1;
                }
                mr.h = times_inv_sqrt2(mr.h);
                if mr.h == 0 {
                    mr.h = 1;
                }
            }
        }
        let n = self.act.mouse_report;
        let changed = n.buttons != tmpmr.buttons
            || (n.x != 0 && n.x != tmpmr.x)
            || (n.y != 0 && n.y != tmpmr.y)
            || (n.h != 0 && n.h != tmpmr.h)
            || (n.v != 0 && n.v != tmpmr.v);
        if changed || n.x != 0 || n.y != 0 || n.v != 0 || n.h != 0 {
            self.mousekey_send(hal);
        }
        self.act.mouse_report = tmpmr;
    }

    fn mousekey_on(&mut self, code: u8) {
        let c = code as u16;
        let mr = self.act.mouse_report;
        if mr.x != 0 || mr.y != 0 || mr.h != 0 || mr.v != 0 {
            self.act.mousekey_repeat = 8;
            self.act.mousekey_wheel_repeat = 1;
        }
        let mu = self.move_unit();
        let wu = self.wheel_unit();
        let mr = &mut self.act.mouse_report;
        match c {
            KC_MS_UP => mr.y = mu.wrapping_neg(),
            KC_MS_DOWN => mr.y = mu,
            KC_MS_LEFT => mr.x = mu.wrapping_neg(),
            KC_MS_RIGHT => mr.x = mu,
            KC_MS_WH_UP => mr.v = wu,
            KC_MS_WH_DOWN => mr.v = wu.wrapping_neg(),
            KC_MS_WH_LEFT => mr.h = wu.wrapping_neg(),
            KC_MS_WH_RIGHT => mr.h = wu,
            KC_MS_BTN1..=KC_MS_BTN8 => mr.buttons |= 1 << (c - KC_MS_BTN1),
            KC_MS_ACCEL0 => self.act.mousekey_accel |= 1,
            KC_MS_ACCEL1 => self.act.mousekey_accel |= 2,
            KC_MS_ACCEL2 => self.act.mousekey_accel |= 4,
            _ => {}
        }
    }

    fn mousekey_off(&mut self, code: u8) {
        let c = code as u16;
        let mr = &mut self.act.mouse_report;
        match c {
            KC_MS_UP if mr.y < 0 => mr.y = 0,
            KC_MS_DOWN if mr.y > 0 => mr.y = 0,
            KC_MS_LEFT if mr.x < 0 => mr.x = 0,
            KC_MS_RIGHT if mr.x > 0 => mr.x = 0,
            KC_MS_WH_UP if mr.v > 0 => mr.v = 0,
            KC_MS_WH_DOWN if mr.v < 0 => mr.v = 0,
            KC_MS_WH_LEFT if mr.h < 0 => mr.h = 0,
            KC_MS_WH_RIGHT if mr.h > 0 => mr.h = 0,
            KC_MS_BTN1..=KC_MS_BTN8 => mr.buttons &= !(1 << (c - KC_MS_BTN1)),
            KC_MS_ACCEL0 => self.act.mousekey_accel &= !1,
            KC_MS_ACCEL1 => self.act.mousekey_accel &= !2,
            KC_MS_ACCEL2 => self.act.mousekey_accel &= !4,
            _ => {}
        }
        let mr = self.act.mouse_report;
        if mr.x == 0 && mr.y == 0 {
            self.act.mousekey_repeat = 0;
        }
        if mr.v == 0 && mr.h == 0 {
            self.act.mousekey_wheel_repeat = 0;
        }
    }

    /// `mousekey_send()`.
    pub fn mousekey_send(&mut self, hal: &mut impl Hal) {
        let time = hal.now() as u16;
        let mr = self.act.mouse_report;
        if mr.x != 0 || mr.y != 0 {
            self.act.last_timer_c = time;
        }
        if mr.v != 0 || mr.h != 0 {
            self.act.last_timer_w = time;
        }
        self.host_mouse_send(hal, mr.bytes());
    }

    /// `mousekey_clear()`.
    pub fn mousekey_clear(&mut self) {
        self.act.mouse_report = MouseReport::default();
        self.act.mousekey_repeat = 0;
        self.act.mousekey_wheel_repeat = 0;
        self.act.mousekey_accel = 0;
    }
}
