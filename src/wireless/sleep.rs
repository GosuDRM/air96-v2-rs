//! Power management — port of sleep.c
//!
//! Handles inactivity timeout, USB suspend detection, and sleep/wakeup sequence.
//! All GosuDRM latency fixes applied (K1: inline wakeup, L1: reduced delays).

use heapless::Vec;

use crate::wireless::uart::{UartProtocol, CMD_HAND, CMD_SLEEP, CMD_SET_CONFIG, LinkMode, RfState};

/// Sleep timing constants (port of ansi.h defines)
pub const SLEEP_TIME_DELAY: u16 = 100 * 360;    // 6 min inactivity
pub const LINK_TIMEOUT: u16 = 100 * 120;         // 2 min link timeout
pub const POWER_DOWN_DELAY: u16 = 24;

pub struct SleepManager {
    /// Inactivity counter (incremented every 10ms, reset on key event)
    pub no_act_time: u16,
    /// Linking timeout counter
    pub rf_linking_time: u16,
    /// USB suspend debounce counter
    usb_suspend_debounce: u8,
    /// RF disconnect debounce counter (50ms ticks)
    rf_disconnect_time: u32,
    /// Sleep enabled by user config
    pub sleep_enabled: bool,
    /// Goto-sleep flag (set by external trigger)
    pub f_goto_sleep: bool,
    /// Wakeup prepared flag (set after sleep, cleared on activity)
    pub f_wakeup_prepare: bool,
}

impl SleepManager {
    pub fn new() -> Self {
        Self {
            no_act_time: 0,
            rf_linking_time: 0,
            usb_suspend_debounce: 0,
            rf_disconnect_time: 0,
            sleep_enabled: true,
            f_goto_sleep: false,
            f_wakeup_prepare: false,
        }
    }

    /// Called on every key event — resets inactivity timer
    pub fn on_activity(&mut self) {
        self.no_act_time = 0;
    }

    /// Called every 10ms (port of timer_pro increment, ansi.c:680)
    pub fn tick_10ms(&mut self) {
        self.no_act_time = self.no_act_time.saturating_add(1);
        self.rf_linking_time = self.rf_linking_time.saturating_add(1);
    }

    /// Main sleep state machine — port of Sleep_Handle (sleep.c:34-113)
    /// Called every 50ms by the main loop.
    /// Returns true if a UART command was generated that needs sending.
    pub fn tick(&mut self, proto: &mut UartProtocol, usb_suspended: bool) -> Option<Vec<u8, 64>> {
        // ── Goto-sleep transition ──
        if self.f_goto_sleep {
            self.f_goto_sleep = false;
            if self.sleep_enabled {
                if proto.rf_state == RfState::Connect {
                    // Connected — tell NRF to go to low-power mode
                    proto.build_link_cmd(CMD_SET_CONFIG);
                } else {
                    proto.build_link_cmd(CMD_SLEEP);
                }
                // Power down handled by main.rs GPIO
            }
            self.f_wakeup_prepare = true;
            return None; // CMD sent inline by build_link_cmd → caller handles TX
        }

        // ── Wakeup transition (K1 fix: also triggered inline by uart_send_report) ──
        if self.f_wakeup_prepare && self.no_act_time < 10 {
            self.f_wakeup_prepare = false;
            // Power-up GPIO handled by main.rs
            proto.build_link_cmd(CMD_HAND);
            // USB wakeup handled by main.rs if link_mode == USB
            return None;
        }

        // Block normal checks during sleep/wakeup transitions
        if self.f_goto_sleep || self.f_wakeup_prepare {
            return None;
        }

        // ── Normal operation: check sleep conditions ──
        match proto.link_mode {
            LinkMode::Usb => {
                if usb_suspended {
                    self.usb_suspend_debounce += 1;
                    if self.usb_suspend_debounce >= 20 {
                        self.f_goto_sleep = true;
                    }
                } else {
                    self.usb_suspend_debounce = 0;
                }
            }
            _ => {
                // Wireless mode
                if proto.rf_state == RfState::Connect {
                    self.rf_disconnect_time = 0;
                    self.rf_linking_time = 0;
                    if self.no_act_time >= SLEEP_TIME_DELAY {
                        self.f_goto_sleep = true;
                    }
                } else if proto.rf_state == RfState::Linking || proto.rf_state == RfState::Pairing {
                    if self.rf_linking_time >= LINK_TIMEOUT {
                        self.rf_linking_time = 0;
                        self.f_goto_sleep = true;
                    }
                } else if proto.rf_state == RfState::Disconnect {
                    self.rf_linking_time = 0;
                    self.rf_disconnect_time += 1;
                    if self.rf_disconnect_time > 5 * 20 {
                        self.rf_disconnect_time = 0;
                        self.f_goto_sleep = true;
                    }
                } else {
                    self.rf_linking_time = 0;
                }
            }
        }
        None
    }
}
