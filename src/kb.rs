//! Keyboard core — 1:1 port of `keyboards/air96_v2/ansi/ansi.c` (C firmware
//! v3.2.9) plus the QMK action/report/host glue that file relies on
//! (`quantum/action_util.c`, `action_layer.c`, `tmk_core/protocol/host.c`,
//! `process_keycode/process_rgb.c`).
//!
//! `Kb` holds what the C firmware keeps in globals. The other C files are
//! ported as `impl Kb` blocks in matching modules:
//!   rf.c        → `wireless::uart`
//!   rf_driver.c → `wireless::report`
//!   sleep.c     → `wireless::sleep`
//!   side.c      → `led::side`
//! Function names follow the C source so the two can be read side by side.

use crate::config::eeprom::UserConfig;
use crate::hal::{Hal, UsbDeviceState};
use crate::keyboard::keymap;
use crate::led::rgb::RgbMatrix;
use crate::wireless::uart::*;

// ── ansi.h ──────────────────────────────────────────────────────────────
pub const RF_IDLE: u8 = 0;
pub const RF_PAIRING: u8 = 1;
pub const RF_LINKING: u8 = 2;
pub const RF_CONNECT: u8 = 3;
pub const RF_DISCONNECT: u8 = 4;
pub const RF_SLEEP: u8 = 5;
pub const RF_SNIF: u8 = 6;
pub const RF_INVALID: u8 = 0xFE;
pub const RF_ERR_STATE: u8 = 0xFF;

pub const LINK_RF_24: u8 = 0;
pub const LINK_BT_1: u8 = 1;
pub const LINK_BT_2: u8 = 2;
pub const LINK_BT_3: u8 = 3;
pub const LINK_USB: u8 = 4;

pub const SYS_SW_WIN: u8 = 0xA1;
pub const SYS_SW_MAC: u8 = 0xA2;
pub const RF_LINK_SHOW_TIME: u16 = 300;
pub const HOST_USB_TYPE: u8 = 0;
pub const HOST_BLE_TYPE: u8 = 1;
pub const HOST_RF_TYPE: u8 = 2;

pub const LINK_TIMEOUT: u16 = 100 * 120;
pub const SLEEP_TIME_DELAY: u16 = 100 * 360;
pub const POWER_DOWN_DELAY: u8 = 24;

// ── ansi.c ──────────────────────────────────────────────────────────────
const RF_LONG_PRESS_DELAY: u16 = 60;
const DEV_RESET_PRESS_DELAY: u16 = 60;
const RGB_TEST_PRESS_DELAY: u16 = 60;

// ── QMK rgb_matrix_{increase,decrease}_* (each persists via eeconfig) ──
fn rgb_increase_hue(r: &mut RgbMatrix) { r.increase_hue(); }
fn rgb_decrease_hue(r: &mut RgbMatrix) { r.decrease_hue(); }
fn rgb_increase_sat(r: &mut RgbMatrix) { r.increase_sat(); }
fn rgb_decrease_sat(r: &mut RgbMatrix) { r.decrease_sat(); }
fn rgb_increase_val(r: &mut RgbMatrix) { r.increase_val(); }
fn rgb_decrease_val(r: &mut RgbMatrix) { r.decrease_val(); }
fn rgb_increase_speed(r: &mut RgbMatrix) { r.increase_speed(); }
fn rgb_decrease_speed(r: &mut RgbMatrix) { r.decrease_speed(); }

// ── QMK ─────────────────────────────────────────────────────────────────
pub const NKRO_REPORT_BITS: usize = 30;
pub const KEYBOARD_REPORT_KEYS: usize = 6;
pub const KC_NUM_LOCK: u8 = 0x53;
const KC_SPACE: u8 = 0x2C;
const KC_S: u8 = 0x16;
const KC_3: u8 = 0x20;
const KC_4: u8 = 0x21;
const MOD_LSFT: u8 = 0x02;
const MOD_LGUI: u8 = 0x08;
/// `RGB_MATRIX_DEFAULT_MODE` = `RGB_MATRIX_CYCLE_LEFT_RIGHT` (config.h).
pub const RGB_MATRIX_DEFAULT_MODE: u8 = 12;
pub const RGB_MATRIX_DEFAULT_SPD: u8 = 127;
pub const RGB_MATRIX_MAXIMUM_BRIGHTNESS: u8 = 255;
/// keyboard.json `rgb_matrix.val_steps` / `speed_steps`.
pub const RGB_MATRIX_VAL_STEP: u8 = 52;
pub const RGB_MATRIX_SPD_STEP: u8 = 52;
pub const RGB_MATRIX_HUE_STEP: u8 = 8;
pub const RGB_MATRIX_SAT_STEP: u8 = 16;

/// `user_config_t` (ansi.h).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserConfigT {
    pub default_brightness_flag: u8,
    pub ee_side_mode: u8,
    pub ee_side_light: u8,
    pub ee_side_speed: u8,
    pub ee_side_rgb: u8,
    pub ee_side_colour: u8,
    pub sleep_enable: bool,
}

/// `DEV_INFO_STRUCT` (ansi.h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevInfo {
    pub link_mode: u8,
    pub rf_channel: u8,
    pub ble_channel: u8,
    pub rf_state: u8,
    pub rf_charge: u8,
    pub rf_led: u8,
    pub rf_baterry: u8,
    pub sys_sw_state: u8,
}

/// Which `host_driver_t` is installed (`chibios_driver` or `rf_host_driver`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostDriver {
    Usb,
    Rf,
}

pub struct Kb {
    // ── ansi.c globals ──────────────────────────────────────────────────
    pub user_config: UserConfigT,
    pub dev_info: DevInfo,
    pub f_uart_ack: bool,
    pub f_bat_hold: bool,
    pub f_sys_show: bool,
    pub f_sleep_show: bool,
    pub f_rf_read_data_ok: bool,
    pub f_rf_sts_sysc_ok: bool,
    pub f_rf_new_adv_ok: bool,
    pub f_rf_reset: bool,
    pub f_send_channel: bool,
    pub f_rf_hand_ok: bool,
    pub f_dial_sw_init_ok: bool,
    pub f_goto_sleep: bool,
    pub f_wakeup_prepare: bool,
    pub f_rf_sw_press: bool,
    pub f_dev_reset_press: bool,
    pub f_rgb_test_press: bool,
    pub f_bat_num_show: bool,
    pub numlock_phase: u8,
    pub numlock_settle_start: u32,
    pub numlock_hold_start: u32,
    pub f_numlock_rearm: bool,
    numlock_was_ready: bool,
    pub host_mode: u8,
    pub rf_linking_time: u16,
    pub rf_link_show_time: u16,
    pub rf_blink_cnt: u8,
    pub no_act_time: u16,
    pub dev_reset_press_delay: u16,
    pub rf_sw_press_delay: u16,
    pub rgb_test_press_delay: u16,
    pub rf_sw_temp: u8,
    long_press_timer: u32,
    pairing_retries: u8,
    dial_save: u8,
    dial_debounce: u8,
    dial_scan_timer: u32,
    timer_pro_first: bool,
    timer_pro_timer: u32,

    // ── rf.c globals (see wireless::uart) ───────────────────────────────
    pub rf: crate::wireless::uart::RfState,
    // ── sleep.c globals (see wireless::sleep) ───────────────────────────
    pub sleep: crate::wireless::sleep::SleepState,
    // ── side.c globals (see led::side) ──────────────────────────────────
    pub side: crate::led::side::SideState,

    // ── QMK state ───────────────────────────────────────────────────────
    pub rgb: RgbMatrix,
    pub driver: HostDriver,
    pub real_mods: u8,
    pub weak_mods: u8,
    /// `nkro_report->bits`.
    pub nkro_bits: [u8; NKRO_REPORT_BITS],
    /// `keyboard_report->keys`.
    pub keys6: [u8; KEYBOARD_REPORT_KEYS],
    last_nkro: Option<(u8, [u8; NKRO_REPORT_BITS])>,
    last_6kro: Option<[u8; 8]>,
    last_system_usage: u16,
    last_consumer_usage: u16,
    /// `keymap_config.nkro` (FORCE_NKRO).
    pub nkro: bool,
    /// `keyboard_protocol` (1 = report protocol).
    pub keyboard_protocol: u8,
    pub layer_state: u32,
    pub default_layer_state: u32,
    /// QMK action engine state (see `action.rs`).
    pub act: crate::action::ActionState,
}

impl Default for Kb {
    fn default() -> Self {
        Self::new()
    }
}

impl Kb {
    pub const fn new() -> Self {
        Self {
            user_config: UserConfigT { default_brightness_flag: 0, ee_side_mode: 0, ee_side_light: 0, ee_side_speed: 0, ee_side_rgb: 0, ee_side_colour: 0, sleep_enable: false },
            dev_info: DevInfo {
                link_mode: LINK_USB,
                rf_channel: 0,
                ble_channel: 0,
                rf_state: RF_IDLE,
                rf_charge: 0,
                rf_led: 0,
                rf_baterry: 100,
                sys_sw_state: 0,
            },
            f_uart_ack: false,
            f_bat_hold: false,
            f_sys_show: false,
            f_sleep_show: false,
            f_rf_read_data_ok: false,
            f_rf_sts_sysc_ok: false,
            f_rf_new_adv_ok: false,
            f_rf_reset: false,
            f_send_channel: false,
            f_rf_hand_ok: false,
            f_dial_sw_init_ok: false,
            f_goto_sleep: false,
            f_wakeup_prepare: false,
            f_rf_sw_press: false,
            f_dev_reset_press: false,
            f_rgb_test_press: false,
            f_bat_num_show: false,
            numlock_phase: 0,
            numlock_settle_start: 0,
            numlock_hold_start: 0,
            f_numlock_rearm: false,
            numlock_was_ready: false,
            host_mode: 0,
            rf_linking_time: 0,
            rf_link_show_time: 0,
            rf_blink_cnt: 0,
            no_act_time: 0,
            dev_reset_press_delay: 0,
            rf_sw_press_delay: 0,
            rgb_test_press_delay: 0,
            rf_sw_temp: 0,
            long_press_timer: 0,
            pairing_retries: 0,
            dial_save: 0xF0,
            dial_debounce: 0,
            dial_scan_timer: 0,
            timer_pro_first: true,
            timer_pro_timer: 0,
            rf: crate::wireless::uart::RfState::new(),
            sleep: crate::wireless::sleep::SleepState::new(),
            side: crate::led::side::SideState::new(),
            rgb: RgbMatrix::new(),
            driver: HostDriver::Usb,
            real_mods: 0,
            weak_mods: 0,
            nkro_bits: [0; NKRO_REPORT_BITS],
            keys6: [0; KEYBOARD_REPORT_KEYS],
            last_nkro: None,
            last_6kro: None,
            last_system_usage: 0,
            last_consumer_usage: 0,
            nkro: true,
            keyboard_protocol: 1,
            layer_state: 0,
            default_layer_state: 1,
            act: crate::action::ActionState::new(),
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // QMK host.c — the modified copy in the C tree
    // ════════════════════════════════════════════════════════════════════

    /// `host_keyboard_leds()`.
    pub fn host_keyboard_leds(&self, hal: &impl Hal) -> u8 {
        match self.driver {
            HostDriver::Usb => hal.usb_keyboard_leds(),
            HostDriver::Rf => self.rf_keyboard_leds(),
        }
    }

    /// `host_keyboard_send()`. The C tree's host.c runs the wireless report
    /// task before every 6KRO send.
    pub fn host_keyboard_send(&mut self, hal: &mut impl Hal, report: [u8; 8]) {
        self.uart_send_report_func(hal);
        match self.driver {
            HostDriver::Usb => hal.usb_send_keyboard(&report),
            HostDriver::Rf => self.rf_send_keyboard(hal, report),
        }
    }

    /// `host_nkro_send()`.
    pub fn host_nkro_send(&mut self, hal: &mut impl Hal, mods: u8, bits: [u8; NKRO_REPORT_BITS]) {
        match self.driver {
            HostDriver::Usb => hal.usb_send_nkro(mods, &bits),
            HostDriver::Rf => self.rf_send_nkro(hal, mods, bits),
        }
    }

    /// `host_system_send()`. The C copy also calls `uart_send_system_report()`
    /// with no argument (undefined behaviour); that stray call is not ported.
    pub fn host_system_send(&mut self, hal: &mut impl Hal, usage: u16) {
        if usage == self.last_system_usage {
            return;
        }
        self.last_system_usage = usage;
        match self.driver {
            HostDriver::Usb => hal.usb_send_system(usage),
            HostDriver::Rf => self.uart_send_system_report(hal, usage),
        }
    }

    /// `host_consumer_send()`. As above, the stray argument-less
    /// `uart_send_consumer_report()` call in the C copy is not ported.
    pub fn host_consumer_send(&mut self, hal: &mut impl Hal, usage: u16) {
        if usage == self.last_consumer_usage {
            return;
        }
        self.last_consumer_usage = usage;
        match self.driver {
            HostDriver::Usb => hal.usb_send_consumer(usage),
            HostDriver::Rf => self.uart_send_consumer_report(hal, usage),
        }
    }

    /// `host_mouse_send()`. The C copy also calls `uart_send_mouse_report()`
    /// with no argument (undefined behaviour); that stray call is not ported.
    pub fn host_mouse_send(&mut self, hal: &mut impl Hal, report: [u8; 5]) {
        match self.driver {
            HostDriver::Usb => hal.usb_send_mouse(&report),
            HostDriver::Rf => self.uart_send_mouse_report(hal, report),
        }
    }

    fn host_set_driver(&mut self, driver: HostDriver) {
        self.driver = driver;
    }

    // ════════════════════════════════════════════════════════════════════
    // QMK action_util.c
    // ════════════════════════════════════════════════════════════════════

    fn nkro_active(&self) -> bool {
        self.keyboard_protocol != 0 && self.nkro
    }

    pub fn add_key(&mut self, key: u8) {
        if self.nkro_active() {
            if (key as usize / 8) < NKRO_REPORT_BITS {
                self.nkro_bits[key as usize / 8] |= 1 << (key % 8);
            }
        } else {
            // add_key_byte(): keep an existing entry, else take the first free slot.
            let mut empty = None;
            for i in 0..KEYBOARD_REPORT_KEYS {
                if self.keys6[i] == key {
                    return;
                }
                if empty.is_none() && self.keys6[i] == 0 {
                    empty = Some(i);
                }
            }
            if let Some(i) = empty {
                self.keys6[i] = key;
            }
        }
    }

    pub fn del_key(&mut self, key: u8) {
        if self.nkro_active() {
            if (key as usize / 8) < NKRO_REPORT_BITS {
                self.nkro_bits[key as usize / 8] &= !(1 << (key % 8));
            }
        } else {
            for k in self.keys6.iter_mut() {
                if *k == key {
                    *k = 0;
                }
            }
        }
    }

    fn clear_keys(&mut self) {
        self.nkro_bits = [0; NKRO_REPORT_BITS];
        self.keys6 = [0; KEYBOARD_REPORT_KEYS];
    }

    /// `is_key_pressed()`.
    pub fn is_key_pressed(&self, key: u8) -> bool {
        if key == 0 {
            return false;
        }
        if self.nkro_active() {
            (key as usize / 8) < NKRO_REPORT_BITS && self.nkro_bits[key as usize / 8] & (1 << (key % 8)) != 0
        } else {
            self.keys6.contains(&key)
        }
    }

    /// `has_anykey()`.
    pub fn has_anykey(&self) -> bool {
        if self.nkro_active() {
            self.nkro_bits.iter().any(|&b| b != 0)
        } else {
            self.keys6.iter().any(|&k| k != 0)
        }
    }

    /// `has_anymod()`.
    pub fn has_anymod(&self) -> bool {
        self.real_mods != 0
    }

    /// `send_keyboard_report()`.
    pub fn send_keyboard_report(&mut self, hal: &mut impl Hal) {
        let mods = self.get_mods_for_report();
        if self.nkro_active() {
            let report = (mods, self.nkro_bits);
            if self.last_nkro != Some(report) {
                self.last_nkro = Some(report);
                self.host_nkro_send(hal, mods, self.nkro_bits);
            }
        } else {
            let mut report = [0u8; 8];
            report[0] = mods;
            report[2..].copy_from_slice(&self.keys6);
            if self.last_6kro != Some(report) {
                self.last_6kro = Some(report);
                self.host_keyboard_send(hal, report);
            }
        }
    }

    pub fn clear_mods(&mut self) {
        self.real_mods = 0;
    }
    pub fn clear_weak_mods(&mut self) {
        self.weak_mods = 0;
    }

    /// `clear_keyboard()`.
    pub fn clear_keyboard(&mut self, hal: &mut impl Hal) {
        self.clear_mods();
        self.clear_keys();
        // clear_keyboard_but_mods_and_keys()
        self.host_system_send(hal, 0);
        self.host_consumer_send(hal, 0);
        self.clear_weak_mods();
        self.send_keyboard_report(hal);
        self.mousekey_clear();
        self.mousekey_send(hal);
    }

    /// `process_rgb()`: QMK acts on key *release* (no RGB_TRIGGER_ON_KEYDOWN);
    /// a held Shift reverses the step. The legacy RGB_* keycodes in keymap.c
    /// are the QK_UNDERGLOW_* range, which also drives the RGB matrix.
    pub(crate) fn process_rgb(&mut self, hal: &mut impl Hal, keycode: u16, pressed: bool) -> bool {
        use keymap::*;
        if pressed {
            return true;
        }
        let shifted = self.real_mods & (MOD_LSFT | 0x20) != 0;
        let (fwd, rev): (fn(&mut RgbMatrix), fn(&mut RgbMatrix)) = match keycode {
            QK_UNDERGLOW_TOGGLE | QK_RGB_MATRIX_TOGGLE => {
                self.rgb.toggle();
                self.eeconfig_update_user_datablock(hal);
                return false;
            }
            QK_RGB_MATRIX_ON => {
                self.rgb.enable();
                self.eeconfig_update_user_datablock(hal);
                return false;
            }
            QK_RGB_MATRIX_OFF => {
                self.rgb.disable();
                self.eeconfig_update_user_datablock(hal);
                return false;
            }
            QK_UNDERGLOW_MODE_NEXT | QK_RGB_MATRIX_MODE_NEXT => (|r| r.next_mode(), |r| r.prev_mode()),
            QK_UNDERGLOW_MODE_PREVIOUS | QK_RGB_MATRIX_MODE_PREVIOUS => (|r| r.prev_mode(), |r| r.next_mode()),
            QK_UNDERGLOW_HUE_UP | QK_RGB_MATRIX_HUE_UP => (rgb_increase_hue, rgb_decrease_hue),
            QK_UNDERGLOW_HUE_DOWN | QK_RGB_MATRIX_HUE_DOWN => (rgb_decrease_hue, rgb_increase_hue),
            QK_UNDERGLOW_SATURATION_UP | QK_RGB_MATRIX_SATURATION_UP => (rgb_increase_sat, rgb_decrease_sat),
            QK_UNDERGLOW_SATURATION_DOWN | QK_RGB_MATRIX_SATURATION_DOWN => (rgb_decrease_sat, rgb_increase_sat),
            QK_UNDERGLOW_VALUE_UP | QK_RGB_MATRIX_VALUE_UP => (rgb_increase_val, rgb_decrease_val),
            QK_UNDERGLOW_VALUE_DOWN | QK_RGB_MATRIX_VALUE_DOWN => (rgb_decrease_val, rgb_increase_val),
            QK_UNDERGLOW_SPEED_UP | QK_RGB_MATRIX_SPEED_UP => (rgb_increase_speed, rgb_decrease_speed),
            QK_UNDERGLOW_SPEED_DOWN | QK_RGB_MATRIX_SPEED_DOWN => (rgb_decrease_speed, rgb_increase_speed),
            _ => return true,
        };
        if shifted { rev(&mut self.rgb) } else { fwd(&mut self.rgb) }
        self.eeconfig_update_user_datablock(hal);
        false
    }

    /// `eeconfig_update_user_datablock(&user_config)` together with the RGB
    /// matrix config, which QMK persists in the same eeconfig.
    #[inline(never)]
    pub fn eeconfig_update_user_datablock(&mut self, hal: &mut impl Hal) {
        let cfg = UserConfig {
            side_mode: self.user_config.ee_side_mode,
            side_brightness: self.user_config.ee_side_light,
            side_speed: self.user_config.ee_side_speed,
            side_colour: self.user_config.ee_side_colour,
            side_rgb: self.user_config.ee_side_rgb != 0,
            sleep_enable: self.user_config.sleep_enable,
            rgb_mode: self.rgb.mode,
            rgb_hue: self.rgb.hue,
            rgb_sat: self.rgb.sat,
            rgb_val: self.rgb.val,
            rgb_speed: self.rgb.speed,
            rgb_enabled: self.rgb.enabled,
        };
        hal.eeconfig_update(&cfg);
    }

    // ════════════════════════════════════════════════════════════════════
    // ansi.c
    // ════════════════════════════════════════════════════════════════════

    /// `long_press_key()`.
    fn long_press_key(&mut self, hal: &mut impl Hal) {
        if hal.now().wrapping_sub(self.long_press_timer) < 50 {
            return;
        }
        self.long_press_timer = hal.now();

        if self.pairing_retries != 0 {
            if self.dev_info.rf_state != RF_PAIRING || self.f_rf_new_adv_ok {
                self.pairing_retries = 0;
            } else {
                self.pairing_retries -= 1;
                self.uart_send_cmd(hal, CMD_NEW_ADV, 0, 0);
            }
        }

        if self.f_rf_sw_press {
            self.rf_sw_press_delay += 1;
            if self.rf_sw_press_delay >= RF_LONG_PRESS_DELAY {
                self.f_rf_sw_press = false;
                self.switch_dev_link(hal, self.rf_sw_temp);
                self.f_send_channel = false; /* Pair below; do not queue a normal link. */
                self.dev_info.rf_channel = self.rf_sw_temp;
                self.dev_info.ble_channel = self.rf_sw_temp;

                self.uart_send_cmd(hal, CMD_NEW_ADV, 0, 0);
                self.pairing_retries = 2;
            }
        } else {
            self.rf_sw_press_delay = 0;
        }

        if self.f_dev_reset_press {
            self.dev_reset_press_delay += 1;
            if self.dev_reset_press_delay >= DEV_RESET_PRESS_DELAY {
                self.f_dev_reset_press = false;

                if self.dev_info.link_mode != LINK_USB {
                    if self.dev_info.link_mode != LINK_RF_24 {
                        self.dev_info.link_mode = LINK_BT_1;
                        self.dev_info.ble_channel = LINK_BT_1;
                        self.dev_info.rf_channel = LINK_BT_1;
                    }
                } else {
                    self.dev_info.ble_channel = LINK_BT_1;
                    self.dev_info.rf_channel = LINK_BT_1;
                }

                self.uart_send_cmd(hal, CMD_SET_LINK, 0, 0);
                hal.wait_ms(500);
                self.uart_send_cmd(hal, CMD_CLR_DEVICE, 10, 5);

                hal.eeconfig_init();
                self.device_reset_show(hal);
                self.device_reset_init(hal);

                if self.dev_info.sys_sw_state == SYS_SW_MAC {
                    self.default_layer_set(1 << 0);
                } else {
                    self.default_layer_set(1 << 2);
                }
            }
        } else {
            self.dev_reset_press_delay = 0;
        }

        if self.f_rgb_test_press {
            self.rgb_test_press_delay += 1;
            if self.rgb_test_press_delay >= RGB_TEST_PRESS_DELAY {
                self.f_rgb_test_press = false;
                self.rgb_test_show(hal);
            }
        } else {
            self.rgb_test_press_delay = 0;
        }
    }

    /// `m_break_all_key()`: release everything on every endpoint.
    pub fn m_break_all_key(&mut self, hal: &mut impl Hal) {
        let nkro_temp = self.nkro;

        self.clear_weak_mods();
        self.clear_mods();
        self.clear_keyboard(hal);

        self.nkro = true;
        self.nkro_bits = [0; NKRO_REPORT_BITS];
        self.host_nkro_send(hal, 0, [0; NKRO_REPORT_BITS]);

        self.nkro = false;
        self.keys6 = [0; KEYBOARD_REPORT_KEYS];
        self.host_keyboard_send(hal, [0; 8]);

        self.nkro = nkro_temp;

        if self.dev_info.link_mode != LINK_USB {
            self.uart_send_report(hal, CMD_RPT_BIT_KB, &[0; 16]);
            self.uart_send_report(hal, CMD_RPT_BYTE_KB, &[0; 8]);
        }

        self.rf.uart_bit_report_buf = [0; NKRO_REPORT_BITS + 1];
        self.rf.bitkb_report_buf = [0; NKRO_REPORT_BITS + 1];
        self.rf.bytekb_report_buf = [0; 8];

        /* Clear NKRO active flag to stop periodic resends of stale bit reports */
        self.rf.f_bit_kb_act = false;
    }

    /// `switch_dev_link()`.
    pub fn switch_dev_link(&mut self, hal: &mut impl Hal, mode: u8) {
        if mode > LINK_USB {
            return;
        }
        if self.numlock_phase == 2 {
            self.numlock_phase = 0;
        }
        self.numlock_settle_start = 0;
        self.m_break_all_key(hal);

        self.f_goto_sleep = false;
        self.dev_info.link_mode = mode;
        self.dev_info.rf_state = RF_IDLE;
        self.rf_linking_time = 0;
        self.no_act_time = 0;
        self.sleep.rf_disconnect_time = 0;
        self.rf_reset_sync_lost(hal);
        self.f_send_channel = true;

        if mode == LINK_USB {
            self.host_mode = HOST_USB_TYPE;
            self.host_set_driver(HostDriver::Usb);
            self.rf_link_show_time = 0;
        } else {
            self.host_mode = HOST_RF_TYPE;
            self.host_set_driver(HostDriver::Rf);
        }
    }

    /// `dial_sw_scan()`.
    fn dial_sw_scan(&mut self, hal: &mut impl Hal) {
        if hal.now().wrapping_sub(self.dial_scan_timer) < 20 {
            return;
        }
        self.dial_scan_timer = hal.now();

        let mut dial_scan = 0u8;
        if hal.read_dev_mode_pin() {
            dial_scan |= 0x01;
        }
        if hal.read_sys_mode_pin() {
            dial_scan |= 0x02;
        }

        if self.dial_save != dial_scan {
            /* Keep reports on the current host until the switch is stable. */
            self.dial_save = dial_scan;
            self.dial_debounce = 25;
            return;
        } else if self.dial_debounce != 0 {
            self.dial_debounce -= 1;
            return;
        }

        if dial_scan & 0x01 != 0 {
            if self.dev_info.link_mode != LINK_USB {
                self.switch_dev_link(hal, LINK_USB);
            }
        } else if self.dev_info.link_mode != self.dev_info.rf_channel {
            self.switch_dev_link(hal, self.dev_info.rf_channel);
        }

        if dial_scan & 0x02 != 0 {
            if self.dev_info.sys_sw_state != SYS_SW_MAC {
                self.m_break_all_key(hal);
                self.f_sys_show = true;
                self.default_layer_set(1 << 0);
                self.dev_info.sys_sw_state = SYS_SW_MAC;
                self.nkro = true;
            }
        } else if self.dev_info.sys_sw_state != SYS_SW_WIN {
            self.m_break_all_key(hal);
            self.f_sys_show = true;
            self.default_layer_set(1 << 2);
            self.dev_info.sys_sw_state = SYS_SW_WIN;
            self.nkro = true;
        }

        if !self.f_dial_sw_init_ok {
            self.f_dial_sw_init_ok = true;
            if self.dev_info.link_mode != LINK_USB {
                self.host_set_driver(HostDriver::Rf);
            }
        }
    }

    /// `m_power_on_dial_sw_scan()`.
    fn m_power_on_dial_sw_scan(&mut self, hal: &mut impl Hal) {
        let mut dial_scan_dev = 0u8;
        let mut dial_scan_sys = 0u8;
        let mut dial_check_dev = 0u8;
        let mut dial_check_sys = 0u8;

        let mut debounce = 0u8;
        while debounce < 10 {
            dial_scan_dev = hal.read_dev_mode_pin() as u8;
            dial_scan_sys = hal.read_sys_mode_pin() as u8;
            if dial_scan_dev != dial_check_dev || dial_scan_sys != dial_check_sys {
                dial_check_dev = dial_scan_dev;
                dial_check_sys = dial_scan_sys;
                debounce = 0;
            }
            hal.wait_ms(1);
            debounce += 1;
        }

        if dial_scan_dev != 0 {
            if self.dev_info.link_mode != LINK_USB {
                self.switch_dev_link(hal, LINK_USB);
            }
        } else if self.dev_info.link_mode != self.dev_info.rf_channel {
            self.switch_dev_link(hal, self.dev_info.rf_channel);
        }

        if dial_scan_sys != 0 {
            if self.dev_info.sys_sw_state != SYS_SW_MAC {
                self.default_layer_set(1 << 0);
                self.dev_info.sys_sw_state = SYS_SW_MAC;
                self.nkro = true;
                self.m_break_all_key(hal);
            }
        } else if self.dev_info.sys_sw_state != SYS_SW_WIN {
            self.default_layer_set(1 << 2);
            self.dev_info.sys_sw_state = SYS_SW_WIN;
            self.nkro = true;
            self.m_break_all_key(hal);
        }
    }

    /// `tap_shortcut()`: shortcuts borrow modifiers without releasing
    /// physically held modifiers.
    fn tap_shortcut(&mut self, hal: &mut impl Hal, mods: u8, key: u8) {
        let saved_weak_mods = self.weak_mods;
        let held = self.is_key_pressed(key);
        self.weak_mods |= mods;
        self.register_code(hal, key);
        hal.wait_ms(5);
        if !held {
            self.unregister_code(hal, key);
        }
        self.weak_mods = saved_weak_mods;
        self.send_keyboard_report(hal);
    }

    /// `process_record_kb()`.
    pub(crate) fn process_record_kb(&mut self, hal: &mut impl Hal, keycode: u16, pressed: bool) -> bool {
        self.no_act_time = 0;
        match keycode {
            keymap::KC_RF_DFU => {
                if pressed {
                    if self.dev_info.link_mode != LINK_USB {
                        return false;
                    }
                    self.uart_send_cmd(hal, CMD_RF_DFU, 10, 5);
                }
                false
            }
            keymap::KC_LNK_USB => {
                if !pressed {
                    if self.dev_info.link_mode != LINK_USB {
                        self.switch_dev_link(hal, LINK_USB);
                        self.f_send_channel = false;
                        self.uart_send_cmd(hal, CMD_SET_LINK, 0, 0);
                    }
                    self.rf_blink_cnt = 3;
                }
                false
            }
            keymap::KC_LNK_RF | keymap::KC_LNK_BLE1 | keymap::KC_LNK_BLE2 | keymap::KC_LNK_BLE3 => {
                /* channel constants are sequential: LINK_RF_24=0, LINK_BT_1=1, LINK_BT_2=2, LINK_BT_3=3 */
                let ch = (keycode - keymap::KC_LNK_RF) as u8;
                if pressed {
                    if self.dev_info.link_mode != LINK_USB {
                        self.rf_sw_temp = ch;
                        self.f_rf_sw_press = true;
                    }
                } else if self.f_rf_sw_press && ch == self.rf_sw_temp {
                    self.f_rf_sw_press = false;
                    if self.rf_sw_press_delay < RF_LONG_PRESS_DELAY {
                        self.switch_dev_link(hal, ch);
                        self.f_send_channel = false; /* Send immediately below. */
                        self.dev_info.rf_channel = ch;
                        self.dev_info.ble_channel = ch;
                        self.uart_send_cmd(hal, CMD_SET_LINK, 0, 0);
                    }
                }
                false
            }
            keymap::KC_MAC_TASK => {
                self.host_consumer_send(hal, if pressed { 0x029F } else { 0 });
                false
            }
            keymap::KC_MAC_SEARCH => {
                if pressed {
                    self.tap_shortcut(hal, MOD_LGUI, KC_SPACE);
                }
                false
            }
            keymap::KC_MAC_VOICE => {
                self.host_consumer_send(hal, if pressed { 0xCF } else { 0 });
                false
            }
            keymap::KC_MAC_CONSOLE => {
                self.host_consumer_send(hal, if pressed { 0x02A0 } else { 0 });
                false
            }
            keymap::KC_MAC_DND => {
                self.host_system_send(hal, if pressed { 0x9B } else { 0 });
                false
            }
            keymap::KC_MAC_PRT => {
                if pressed {
                    self.tap_shortcut(hal, MOD_LGUI | MOD_LSFT, KC_3);
                }
                false
            }
            keymap::KC_MAC_PRTA => {
                if pressed {
                    let key = if self.dev_info.sys_sw_state == SYS_SW_WIN { KC_S } else { KC_4 };
                    self.tap_shortcut(hal, MOD_LGUI | MOD_LSFT, key);
                }
                false
            }
            keymap::KC_SIDE_VAI => {
                if pressed {
                    self.light_level_control(hal, true);
                }
                false
            }
            keymap::KC_SIDE_VAD => {
                if pressed {
                    self.light_level_control(hal, false);
                }
                false
            }
            keymap::KC_SIDE_MOD => {
                if pressed {
                    self.side_mode_control(hal, true);
                }
                false
            }
            keymap::KC_SIDE_HUI => {
                if pressed {
                    self.side_colour_control(hal, true);
                }
                false
            }
            keymap::KC_SIDE_SPI => {
                if pressed {
                    self.light_speed_control(hal, true);
                }
                false
            }
            keymap::KC_SIDE_SPD => {
                if pressed {
                    self.light_speed_control(hal, false);
                }
                false
            }
            keymap::KC_DEV_RESET => {
                if pressed {
                    self.f_dev_reset_press = true;
                    self.m_break_all_key(hal);
                } else {
                    self.f_dev_reset_press = false;
                }
                false
            }
            keymap::KC_SLEEP_MODE => {
                if pressed {
                    self.user_config.sleep_enable = !self.user_config.sleep_enable;
                    self.f_sleep_show = true;
                    self.eeconfig_update_user_datablock(hal);
                }
                false
            }
            keymap::KC_BAT_SHOW => {
                if pressed {
                    self.f_bat_hold = !self.f_bat_hold;
                }
                false
            }
            keymap::KC_RGB_TEST => {
                self.f_rgb_test_press = pressed;
                false
            }
            keymap::KC_BAT_NUM => {
                self.f_bat_num_show = pressed;
                false
            }
            _ => true,
        }
    }

    /// `timer_pro()`.
    fn timer_pro(&mut self, hal: &mut impl Hal) {
        if self.timer_pro_first {
            self.timer_pro_first = false;
            self.timer_pro_timer = hal.now();
        }

        if hal.now().wrapping_sub(self.timer_pro_timer) < 10 {
            return;
        }
        self.timer_pro_timer = hal.now();

        if self.rf_link_show_time < RF_LINK_SHOW_TIME {
            self.rf_link_show_time += 1;
        }
        if self.no_act_time < 0xFFFF {
            self.no_act_time += 1;
        }
        if self.rf_linking_time < 0xFFFF {
            self.rf_linking_time += 1;
        }
    }

    /// `m_londing_eeprom_data()`. `saved` is what `eeconfig_read_user_datablock()`
    /// and the RGB matrix eeconfig returned (None = never written).
    fn m_londing_eeprom_data(&mut self, hal: &mut impl Hal, saved: Option<UserConfig>) {
        match saved {
            Some(cfg) => {
                self.rgb.mode = cfg.rgb_mode;
                self.rgb.hue = cfg.rgb_hue;
                self.rgb.sat = cfg.rgb_sat;
                self.rgb.val = cfg.rgb_val;
                self.rgb.speed = cfg.rgb_speed;
                self.rgb.enabled = cfg.rgb_enabled;
                self.user_config = UserConfigT {
                    default_brightness_flag: 0xA5,
                    ee_side_mode: cfg.side_mode,
                    ee_side_light: cfg.side_brightness,
                    ee_side_speed: cfg.side_speed,
                    ee_side_rgb: cfg.side_rgb as u8,
                    ee_side_colour: cfg.side_colour,
                    sleep_enable: cfg.sleep_enable,
                };
                self.side.side_mode = self.user_config.ee_side_mode;
                self.side.side_light = self.user_config.ee_side_light;
                self.side.side_speed = self.user_config.ee_side_speed;
                self.side.side_rgb = self.user_config.ee_side_rgb;
                self.side.side_colour = self.user_config.ee_side_colour;
            }
            None => {
                // eeconfig_update_rgb_matrix_default(), then the C first-boot override.
                self.rgb.enabled = true;
                self.rgb.mode = RGB_MATRIX_DEFAULT_MODE;
                self.rgb.hue = 0;
                self.rgb.sat = 255;
                self.rgb.val = RGB_MATRIX_MAXIMUM_BRIGHTNESS;
                self.rgb.speed = RGB_MATRIX_DEFAULT_SPD;
                self.rgb.sethsv_noeeprom(255, 255, RGB_MATRIX_MAXIMUM_BRIGHTNESS - RGB_MATRIX_VAL_STEP * 2);
                self.user_config.default_brightness_flag = 0xA5;
                self.user_config.ee_side_mode = self.side.side_mode;
                self.user_config.ee_side_light = self.side.side_light;
                self.user_config.ee_side_speed = self.side.side_speed;
                self.user_config.ee_side_rgb = self.side.side_rgb;
                self.user_config.ee_side_colour = self.side.side_colour;
                self.user_config.sleep_enable = true;
                self.eeconfig_update_user_datablock(hal);
            }
        }

        /* Always clamp side config to prevent OOB on colour_lib[8] and other tables. */
        if self.side.side_mode > 4 {
            self.side.side_mode = 0;
        }
        if self.side.side_light > 5 {
            self.side.side_light = 3;
        }
        if self.side.side_speed > 4 {
            self.side.side_speed = 2;
        }
        if self.side.side_rgb > 1 {
            self.side.side_rgb = 1;
        }
        if self.side.side_colour >= 8 {
            self.side.side_colour = 0;
        }
    }

    /// `keyboard_post_init_kb()` (after `m_gpio_init()` and `rf_uart_init()`,
    /// which the board code performs).
    #[inline(never)]
    pub fn keyboard_post_init_kb(&mut self, hal: &mut impl Hal, saved: Option<UserConfig>) {
        hal.wait_ms(100);
        self.rf_device_init(hal);

        self.m_break_all_key(hal);
        self.m_londing_eeprom_data(hal, saved);
        self.m_power_on_dial_sw_scan(hal);
        self.f_dial_sw_init_ok = true; /* allow reports immediately after init */
        self.numlock_phase = 1; /* trigger NumLock press next housekeeping */
    }

    /// `protocol_post_init()`: QMK installs the USB host driver after
    /// `keyboard_init()`, i.e. after `keyboard_post_init_kb()` may already
    /// have selected the wireless driver.
    pub fn protocol_post_init(&mut self) {
        self.host_set_driver(HostDriver::Usb);
    }

    /// `notify_usb_device_state_change_kb()`: USB bus reset or deconfigure
    /// means the host is enumerating again (PC restart or replug), so re-check
    /// NumLock. Ignored in wireless mode, where the cable only charges.
    pub fn notify_usb_device_state_change_kb(&mut self, state: UsbDeviceState) {
        if state == UsbDeviceState::Init && self.dev_info.link_mode == LINK_USB {
            self.f_numlock_rearm = true;
        }
    }

    /// `suspend_wakeup_init()` (QMK, on USB resume) + `suspend_wakeup_init_kb()`.
    #[inline(never)]
    pub fn suspend_wakeup_init(&mut self, hal: &mut impl Hal) {
        self.clear_mods();
        self.clear_weak_mods();
        self.clear_keys();
        self.host_system_send(hal, 0);
        self.host_consumer_send(hal, 0);
        self.rgb.suspended = false;
    }

    /// `numlock_task()`: turn host NumLock on each time the link to a host
    /// comes up. Windows mode only: macOS has no NumLock.
    fn numlock_task(&mut self, hal: &mut impl Hal) {
        let ready = self.dev_info.sys_sw_state == SYS_SW_WIN
            && if self.dev_info.link_mode == LINK_USB {
                self.host_mode == HOST_USB_TYPE && hal.usb_state() == UsbDeviceState::Configured
            } else {
                self.host_mode == HOST_RF_TYPE
                    && self.dev_info.rf_state == RF_CONNECT
                    && !self.f_wakeup_prepare
                    && !self.f_goto_sleep
            };

        /* Always finish a started press, even if the link dropped meanwhile. */
        if self.numlock_phase == 2 {
            if hal.now().wrapping_sub(self.numlock_hold_start) >= 50 {
                self.unregister_code(hal, KC_NUM_LOCK);
                self.numlock_phase = 0;
            }
            return;
        }

        if self.f_numlock_rearm || (ready && !self.numlock_was_ready) {
            self.f_numlock_rearm = false;
            self.numlock_phase = 1;
            self.numlock_settle_start = 0;
        }
        self.numlock_was_ready = ready;

        if !ready {
            self.numlock_settle_start = 0;
            return;
        }
        if self.numlock_phase == 1 {
            if self.numlock_settle_start == 0 {
                self.numlock_settle_start = hal.now();
            } else if hal.now().wrapping_sub(self.numlock_settle_start) >= 2000 && self.no_act_time > 20 {
                if self.host_keyboard_leds(hal) & 0x01 == 0 {
                    self.register_code(hal, KC_NUM_LOCK);
                    self.numlock_phase = 2;
                    self.numlock_hold_start = hal.now();
                } else {
                    self.numlock_phase = 0;
                }
            }
        }
    }

    /// `rgb_matrix_indicators_kb()`.
    #[inline(never)]
    pub fn rgb_matrix_indicators_kb(&mut self) {
        if self.f_bat_num_show {
            self.num_led_show();
        }
    }

    /// `housekeeping_task_kb()`. `held_matrix` is `matrix_get_row()` != 0 for
    /// any row (used by `Sleep_Handle()`).
    #[inline(never)]
    pub fn housekeeping_task_kb(&mut self, hal: &mut impl Hal, held_matrix: bool) {
        self.timer_pro(hal);

        self.uart_receive_pro(hal);

        self.uart_send_report_func(hal);

        self.long_press_key(hal);

        self.dial_sw_scan(hal);

        self.sleep_handle(hal, held_matrix);

        /* Lower priority — rate-limited and/or slow (I2C). */
        self.dev_sts_sync(hal);

        self.m_side_led_show(hal);

        /* Wireless sleep: idle the core between scans. */
        if self.f_wakeup_prepare && self.dev_info.link_mode != LINK_USB {
            hal.wait_ms(10);
        }

        self.numlock_task(hal);
    }
}
