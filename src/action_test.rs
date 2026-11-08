//! QMK action-engine behaviour for keys assigned through VIA (the C VIA
//! firmware runs the full QMK engine: tapping, one-shot, mouse keys, key
//! lock, macros, magic, grave escape, quantum keycodes).

use crate::config::eeprom;
use crate::hal::{Hal, UsbDeviceState};
use crate::kb::*;
use crate::keyboard::keymap::*;
use crate::led::rgb;
use crate::via;

#[derive(Default)]
struct M {
    t: u32,
    nkro: Vec<(u8, [u8; 30])>,
    kb6: Vec<[u8; 8]>,
    consumer: Vec<u16>,
    mouse: Vec<[u8; 5]>,
    keymap_raw: u16,
    dfu: u32,
    resets: u32,
}

impl Hal for M {
    fn now(&self) -> u32 { self.t }
    fn wait_ms(&mut self, ms: u32) { self.t += ms; }
    fn uart_send_bytes(&mut self, _b: &[u8]) {}
    fn uart_read(&mut self) -> Option<u8> { None }
    fn nrf_reset_pin(&mut self, _h: bool) {}
    fn led_power(&mut self, _on: bool) {}
    fn read_dev_mode_pin(&mut self) -> bool { true }
    fn read_sys_mode_pin(&mut self) -> bool { false }
    fn usb_state(&self) -> UsbDeviceState { UsbDeviceState::Configured }
    fn usb_suspended(&self) -> bool { false }
    fn usb_keyboard_leds(&self) -> u8 { 1 }
    fn usb_send_nkro(&mut self, mods: u8, bits: &[u8; 30]) { self.nkro.push((mods, *bits)); }
    fn usb_send_keyboard(&mut self, r: &[u8; 8]) { self.kb6.push(*r); }
    fn usb_send_mouse(&mut self, r: &[u8; 5]) { self.mouse.push(*r); }
    fn usb_send_consumer(&mut self, u: u16) { self.consumer.push(u); }
    fn usb_send_system(&mut self, _u: u16) {}
    fn usb_remote_wakeup_enabled(&self) -> bool { false }
    fn usb_wakeup_host(&mut self) {}
    fn rgb_update_pwm_buffers(&mut self, _r: &mut rgb::RgbMatrix) {}
    fn eeconfig_update(&mut self, _c: &eeprom::UserConfig) {}
    fn eeconfig_init(&mut self) {}
    fn eeconfig_read_keymap(&self) -> u16 { self.keymap_raw }
    fn eeconfig_update_keymap(&mut self, raw: u16) { self.keymap_raw = raw; }
    fn bootloader_jump(&mut self) { self.dfu += 1; }
    fn mcu_reset(&mut self) { self.resets += 1; }
}

/// Windows layout (default layer 2), wired.
fn setup() -> (Kb, M) {
    let mut m = M { t: 1000, keymap_raw: crate::action::KEYMAP_CONFIG_DEFAULT, ..Default::default() };
    via::via_init();
    let mut kb = Kb::new();
    kb.keymap_config_init(&mut m);
    kb.keyboard_post_init_kb(&mut m, None);
    kb.protocol_post_init();
    m.nkro.clear();
    (kb, m)
}

const A: (u8, u8) = (3, 1);
const B: (u8, u8) = (4, 6);
const K: (u8, u8) = (3, 8);
const LSFT: (u8, u8) = (4, 0);

fn assign(pos: (u8, u8), kc: u16) {
    via::dynamic_keymap_set_keycode(2, pos.0, pos.1, kc);
}

/// Advance time with tick events, as QMK's matrix_task does when idle.
fn idle(kb: &mut Kb, m: &mut M, ms: u32) {
    for _ in 0..ms {
        m.t += 1;
        kb.generate_tick_event(m);
        kb.mousekey_task(m);
    }
}

fn down(bits: &[u8; 30], key: u16) -> bool {
    bits[key as usize / 8] & (1 << (key % 8)) != 0
}

fn last(m: &M) -> (u8, [u8; 30]) {
    *m.nkro.last().unwrap()
}

#[test]
fn mod_tap_tap_sends_key_hold_sends_mod() {
    let (mut kb, mut m) = setup();
    assign(A, QK_MOD_TAP | 0x01 << 8 | 0x05); // LCTL_T(KC_B)
    // tap: B pressed and released, no Ctrl
    kb.matrix_event(&mut m, A.0, A.1, true);
    idle(&mut kb, &mut m, 50);
    assert!(m.nkro.is_empty(), "undecided until release or TAPPING_TERM");
    kb.matrix_event(&mut m, A.0, A.1, false);
    assert!(m.nkro.iter().any(|(mods, b)| *mods == 0 && down(b, 0x05)));
    assert!(m.nkro.iter().all(|(mods, _)| *mods == 0));
    assert!(!down(&last(&m).1, 0x05));
    idle(&mut kb, &mut m, 300);
    // hold past TAPPING_TERM: Ctrl
    m.nkro.clear();
    kb.matrix_event(&mut m, A.0, A.1, true);
    idle(&mut kb, &mut m, 199);
    assert!(m.nkro.is_empty());
    idle(&mut kb, &mut m, 2);
    assert_eq!(last(&m).0, 0x01);
    kb.matrix_event(&mut m, A.0, A.1, false);
    assert_eq!(last(&m), (0, [0; 30]));
}

#[test]
fn layer_tap_hold_switches_layer() {
    let (mut kb, mut m) = setup();
    assign(A, QK_LAYER_TAP | 3 << 8 | 0x05); // LT(3, KC_B)
    kb.matrix_event(&mut m, A.0, A.1, true);
    idle(&mut kb, &mut m, 250);
    assert_eq!(kb.layer_state, 1 << 3);
    kb.matrix_event(&mut m, 0, 2, true); // Win Fn layer: BRID
    kb.matrix_event(&mut m, 0, 2, false);
    assert_eq!(m.consumer, vec![0x0070, 0]);
    kb.matrix_event(&mut m, A.0, A.1, false);
    assert_eq!(kb.layer_state, 0);
}

#[test]
fn one_shot_shift_applies_to_next_key_only() {
    let (mut kb, mut m) = setup();
    assign(A, QK_ONE_SHOT_MOD | 0x02); // OSM(MOD_LSFT)
    kb.matrix_event(&mut m, A.0, A.1, true);
    idle(&mut kb, &mut m, 20);
    kb.matrix_event(&mut m, A.0, A.1, false);
    idle(&mut kb, &mut m, 20);
    kb.matrix_event(&mut m, K.0, K.1, true);
    let (mods, bits) = last(&m);
    assert_eq!(mods, 0x02);
    assert!(down(&bits, 0x0E));
    kb.matrix_event(&mut m, K.0, K.1, false);
    kb.matrix_event(&mut m, K.0, K.1, true);
    assert_eq!(last(&m).0, 0, "one-shot consumed");
}

#[test]
fn mouse_keys_click_and_accelerate() {
    let (mut kb, mut m) = setup();
    assign(A, KC_MS_BTN1);
    assign(B, KC_MS_RIGHT);
    kb.matrix_event(&mut m, A.0, A.1, true);
    kb.matrix_event(&mut m, A.0, A.1, false);
    assert_eq!(&m.mouse[m.mouse.len() - 2..], &[[1, 0, 0, 0, 0], [0, 0, 0, 0, 0]]);
    m.mouse.clear();
    kb.matrix_event(&mut m, B.0, B.1, true);
    idle(&mut kb, &mut m, 1000);
    let xs: Vec<i8> = m.mouse.iter().map(|r| r[1] as i8).collect();
    assert_eq!(xs[0], 8, "first step MOUSEKEY_MOVE_DELTA");
    // mousekey_repeat ramps over MOUSEKEY_TIME_TO_MAX (30) repeats: 8*10*n/30
    assert_eq!(&xs[1..4], &[2, 5, 8]);
    assert!(xs.len() > 40 && xs.iter().all(|&x| x > 0));
    assert_eq!(*xs.last().unwrap(), 80, "accelerates to max speed");
    kb.matrix_event(&mut m, B.0, B.1, false);
    let n = m.mouse.len();
    idle(&mut kb, &mut m, 200);
    assert_eq!(m.mouse.len(), n, "stops when released");
}

#[test]
fn key_lock_holds_next_key() {
    let (mut kb, mut m) = setup();
    assign(B, QK_LOCK);
    kb.matrix_event(&mut m, B.0, B.1, true);
    kb.matrix_event(&mut m, B.0, B.1, false);
    kb.matrix_event(&mut m, K.0, K.1, true);
    kb.matrix_event(&mut m, K.0, K.1, false);
    assert!(down(&last(&m).1, 0x0E), "K stays held");
    kb.matrix_event(&mut m, K.0, K.1, true);
    assert!(down(&last(&m).1, 0x0E));
    kb.matrix_event(&mut m, K.0, K.1, false);
    assert!(!down(&last(&m).1, 0x0E), "second press unlocks");
}

#[test]
fn dynamic_macro_types_text() {
    let (mut kb, mut m) = setup();
    via::dynamic_keymap_macro_set_buffer(0, 3, b"aB\0");
    assign(A, QK_MACRO);
    let t0 = m.t;
    kb.matrix_event(&mut m, A.0, A.1, true);
    let seq: Vec<(u8, bool, bool)> = m.nkro.iter().map(|(mods, b)| (*mods, down(b, 0x04), down(b, 0x05))).collect();
    assert_eq!(seq, vec![(0, true, false), (0, false, false), (2, false, false), (2, false, true), (2, false, false), (0, false, false)]);
    assert!(m.t - t0 >= 8 * 6, "TAP_CODE_DELAY / DYNAMIC_KEYMAP_MACRO_DELAY waits");
}

#[test]
fn magic_toggle_nkro_persists_and_switches_to_6kro() {
    let (mut kb, mut m) = setup();
    assign(A, QK_MAGIC_TOGGLE_NKRO);
    kb.matrix_event(&mut m, A.0, A.1, true);
    kb.matrix_event(&mut m, A.0, A.1, false);
    assert!(!kb.nkro);
    assert_eq!(m.keymap_raw & crate::action::KC_NKRO, 0);
    kb.matrix_event(&mut m, K.0, K.1, true);
    assert_eq!(m.kb6.last().unwrap()[2], 0x0E);
}

#[test]
fn magic_swap_alt_gui_remaps_modifiers() {
    let (mut kb, mut m) = setup();
    assign(A, QK_MAGIC_SWAP_ALT_GUI);
    kb.matrix_event(&mut m, A.0, A.1, true);
    kb.matrix_event(&mut m, A.0, A.1, false);
    kb.matrix_event(&mut m, 5, 2, true); // Win layer LALT
    assert_eq!(last(&m).0, 0x08, "LALT sends LGUI");
}

#[test]
fn grave_escape_follows_shift() {
    let (mut kb, mut m) = setup();
    assign(A, QK_GRAVE_ESCAPE);
    kb.matrix_event(&mut m, A.0, A.1, true);
    assert!(down(&last(&m).1, 0x29));
    kb.matrix_event(&mut m, A.0, A.1, false);
    kb.matrix_event(&mut m, LSFT.0, LSFT.1, true);
    kb.matrix_event(&mut m, A.0, A.1, true);
    assert!(down(&last(&m).1, 0x35));
}

#[test]
fn quantum_boot_and_reboot_keys() {
    let (mut kb, mut m) = setup();
    assign(A, QK_BOOT);
    assign(B, QK_REBOOT);
    kb.matrix_event(&mut m, A.0, A.1, true);
    kb.matrix_event(&mut m, B.0, B.1, true);
    assert_eq!((m.dfu, m.resets), (1, 1));
}

#[test]
fn nkro_is_forced_and_persisted_at_init() {
    let (kb, m) = setup();
    assert!(kb.nkro);
    assert_eq!(m.keymap_raw, crate::action::KEYMAP_CONFIG_DEFAULT | crate::action::KC_NKRO);
}
