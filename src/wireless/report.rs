//! Wireless host driver — 1:1 port of `keyboards/air96_v2/ansi/rf_driver.c`.

use crate::hal::Hal;
use crate::kb::{Kb, NKRO_REPORT_BITS};

impl Kb {
    /// `rf_keyboard_leds()`.
    pub fn rf_keyboard_leds(&self) -> u8 {
        self.dev_info.rf_led
    }

    /// `rf_send_keyboard()`.
    pub fn rf_send_keyboard(&mut self, hal: &mut impl Hal, report: [u8; 8]) {
        self.uart_send_report_keyboard(hal, report);
    }

    /// `rf_send_nkro()`: `report_nkro_t` is `{mods, bits[NKRO_REPORT_BITS]}`.
    pub fn rf_send_nkro(&mut self, hal: &mut impl Hal, mods: u8, bits: [u8; NKRO_REPORT_BITS]) {
        let mut report = [0u8; NKRO_REPORT_BITS + 1];
        report[0] = mods;
        report[1..].copy_from_slice(&bits);
        self.uart_send_report_nkro(hal, report);
    }
}
