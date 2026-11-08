//! Power management — 1:1 port of `keyboards/air96_v2/ansi/sleep.c`.

use crate::hal::Hal;
use crate::kb::*;
use crate::wireless::uart::{CMD_HAND, CMD_SET_CONFIG, CMD_SLEEP};

/// sleep.c globals and `Sleep_Handle()` statics.
pub struct SleepState {
    pub rf_disconnect_time: u32,
    delay_step_timer: u32,
    usb_suspend_debounce: u8,
}

impl Default for SleepState {
    fn default() -> Self {
        Self::new()
    }
}

impl SleepState {
    pub const fn new() -> Self {
        Self { rf_disconnect_time: 0, delay_step_timer: 0, usb_suspend_debounce: 0 }
    }
}

impl Kb {
    /// `Sleep_Handle()`. `held_matrix`: any debounced matrix row is non-zero.
    pub fn sleep_handle(&mut self, hal: &mut impl Hal, held_matrix: bool) {
        /* 50ms interval */
        if hal.now().wrapping_sub(self.sleep.delay_step_timer) < 50 {
            return;
        }
        self.sleep.delay_step_timer = hal.now();

        /* A held key is activity even when no new press/release report occurs.
           USB suspend must still follow the host's power state. */
        if self.dev_info.link_mode != LINK_USB {
            let held = self.has_anykey() || self.has_anymod() || held_matrix;
            if held {
                self.no_act_time = 0;
                self.f_goto_sleep = false;
            }
        }

        if self.f_goto_sleep {
            /* Guard: reject NRF sleep request when USB is active and not suspended */
            if self.dev_info.link_mode == LINK_USB && !hal.usb_suspended() {
                self.f_goto_sleep = false;
                return;
            }

            self.f_goto_sleep = false;

            if self.dev_info.rf_state == RF_CONNECT {
                self.uart_send_cmd(hal, CMD_SET_CONFIG, 5, 5);
            } else {
                self.uart_send_cmd(hal, CMD_SLEEP, 5, 5);
            }

            // power off led
            hal.led_power(false);

            self.f_wakeup_prepare = true;
        }

        if self.f_wakeup_prepare {
            /* Stay asleep until there is a real reason to wake:
                 - key/activity event: process_record_kb() resets no_act_time to 0.
                 - USB mode only: the host resumes the bus. */
            let key_wake = self.no_act_time < 10;
            let usb_wake = self.dev_info.link_mode == LINK_USB && !hal.usb_suspended();

            if !key_wake && !usb_wake {
                return;
            }

            self.f_wakeup_prepare = false;
            self.sleep.usb_suspend_debounce = 0;
            self.sleep.rf_disconnect_time = 0;

            hal.led_power(true);

            if self.dev_info.link_mode == LINK_USB {
                self.uart_send_cmd(hal, CMD_HAND, 0, 1);
            } else {
                self.rf_wake(hal);
            }
            /* Only flush key state on a host-driven wake. */
            if !key_wake {
                self.m_break_all_key(hal);
            }

            if key_wake && self.dev_info.link_mode == LINK_USB && hal.usb_suspended() && hal.usb_remote_wakeup_enabled() {
                /* The USB driver owns the resume pulse. */
                hal.usb_wakeup_host();
            }
        }

        if self.f_goto_sleep || self.f_wakeup_prepare {
            return;
        }

        if self.dev_info.link_mode == LINK_USB {
            if hal.usb_suspended() {
                self.sleep.usb_suspend_debounce += 1;
                if self.sleep.usb_suspend_debounce >= 20 {
                    self.f_goto_sleep = true;
                }
            } else {
                self.sleep.usb_suspend_debounce = 0;
            }
        } else if self.dev_info.rf_state == RF_CONNECT {
            self.sleep.rf_disconnect_time = 0;
            if self.user_config.sleep_enable && self.no_act_time >= SLEEP_TIME_DELAY {
                self.f_goto_sleep = true;
            }
        } else if self.rf_linking_time >= LINK_TIMEOUT {
            self.rf_linking_time = 0;
            self.f_goto_sleep = true;
        } else if self.dev_info.rf_state == RF_DISCONNECT {
            self.sleep.rf_disconnect_time += 1;
            if self.sleep.rf_disconnect_time > 5 * 20 {
                self.sleep.rf_disconnect_time = 0;
                self.f_goto_sleep = true;
            }
        }
    }
}
