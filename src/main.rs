#![no_std]
#![no_main]
#![allow(dead_code)]

mod config;
mod keyboard;
mod wireless;
mod led;
mod usb_hid;

use cortex_m::peripheral::Peripherals as CorePeripherals;
use cortex_m_rt::{entry, exception};

use stm32f0xx_hal::{
    prelude::*,
    pac,
    i2c,
    serial::Serial,
};

use wireless::uart::{
    UartProtocol, CMD_HAND, CMD_SET_LINK, CMD_READ_DATA,
    CMD_RF_STS_SYSC, CMD_SET_NAME, CMD_SET_24G_NAME,
    LinkMode, RfState,
};
use wireless::{sleep::SleepManager, report};
use keyboard::matrix::Matrix;
use keyboard::keymap::{self, is_custom, lnk_to_channel, resolve_keycode, mo_layer};
use led::side::SideLeds;
use led::rgb::RgbMatrix;
use config::eeprom::{self, UserConfig};
use usb_hid::UsbHid;

// ── SysTick tick flag ──────────────────────────────────────────────
// Atomic read+clear via PRIMASK critical section. The precompiled
// core for thumbv6m-none-eabi on this toolchain doesn't expose
// AtomicBool::swap/compare_exchange, so we use `cortex_m::interrupt::free`
// which disables interrupts on this single-core Cortex-M0+ for the
// duration of the read+clear, eliminating the TOCTOU race that the
// previous `static mut bool` had.
static mut TICK_FLAG: bool = false;

/// Wait for a SysTick interrupt. Returns true when tick received.
fn tick_arrived() -> bool {
    cortex_m::interrupt::free(|_| unsafe {
        if TICK_FLAG {
            TICK_FLAG = false;
            true
        } else {
            false
        }
    })
}

#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|_| unsafe {
        TICK_FLAG = true;
    });
}

struct Device {
    proto: UartProtocol,
    sleep: SleepManager,
    matrix: Matrix,
    side: SideLeds,
    rgb: RgbMatrix,
    active_layers: [usize; 4],
    active_layer_count: usize,
    /// Keycode tracking: store keycode on press so release uses same code regardless of layer state
    keycode_track: [[u16; 21]; 6],
    current_keys: [u8; 6],
    current_mods: u8,
    nkro_enabled: bool,
    rf_sw_press: bool,
    rf_sw_press_delay: u16,
    rf_sw_temp: u8,
    dev_reset_press: bool,
    dev_reset_press_delay: u16,
    rgb_test_press: bool,
    rgb_test_press_delay: u16,
    rgb_mod_press: bool,
    rgb_mod_press_delay: u16,
    f_sys_show: bool,
    f_sleep_show: bool,
    f_bat_hold: bool,
    dfu_hold_ticks: u16,
    save_pending: bool,
    sleep_enabled: bool,
    bat_num_show: bool,
    // ── Non-blocking state machines ────────────────────────────
    /// Device reset blink state machine: 0=idle, 1-6=phases (on1,off1,on2,off2,on3,off3)
    reset_blink_phase: u8,
    reset_blink_timer: u16,
    /// RGB test state machine: 0=idle, 1-7=solid colors
    rgb_test_phase: u8,
    rgb_test_timer: u16,
    /// Pending consumer usage for USB HID (0=none)
    pending_consumer_usb: u16,
    /// Pending system control usage for USB HID (0=none)
    pending_system_usb: u16,
    /// Dial switch debounce: blocks changes until stable for 500ms
    f_dial_sw_init_ok: bool,
    dial_dev_debounce: u16,
    dial_sys_debounce: u16,
    dial_dev_last: bool,
    dial_sys_last: bool,
    /// One-shot: send NumLock at boot so numpad works on Linux login
    boot_numlock_done: bool,
    /// I2C flush state machine (chunked: 64 bytes per iteration)
    /// 0=idle, 1=unlock+page d1, 2-4=write d1 chunks, 5=unlock+page d2, 6-8=write d2 chunks
    i2c_flush_phase: u8,
    pwm_buf1: [u8; 192],
    pwm_buf2: [u8; 192],
}

impl Device {
    fn new() -> Self {
        Self {
            proto: UartProtocol::new(),
            sleep: SleepManager::new(),
            matrix: Matrix::new(),
            side: SideLeds::new(),
            rgb: RgbMatrix::new(),
            active_layers: [0, 0, 0, 0],
            active_layer_count: 1,
            keycode_track: [[0; 21]; 6],
            current_keys: [0; 6],
            current_mods: 0,
            nkro_enabled: true,
            rf_sw_press: false, rf_sw_press_delay: 0, rf_sw_temp: 0,
            dev_reset_press: false, dev_reset_press_delay: 0,
            rgb_test_press: false, rgb_test_press_delay: 0,
            rgb_mod_press: false, rgb_mod_press_delay: 0,
            f_sys_show: false, f_sleep_show: false, f_bat_hold: false,
            dfu_hold_ticks: 0,
            save_pending: false,
            sleep_enabled: true, bat_num_show: false,
            reset_blink_phase: 0, reset_blink_timer: 0,
            rgb_test_phase: 0, rgb_test_timer: 0,
            pending_consumer_usb: 0,
            pending_system_usb: 0,
            f_dial_sw_init_ok: false,
            dial_dev_debounce: 0,
            dial_sys_debounce: 0,
            dial_dev_last: false,
            dial_sys_last: false,
            boot_numlock_done: false,
            i2c_flush_phase: 0,
            pwm_buf1: [0u8; 192],
            pwm_buf2: [0u8; 192],
        }
    }

    fn break_all_keys(&mut self) {
        self.current_mods = 0;
        self.current_keys = [0; 6];
        report::send_keyboard(&mut self.proto, 0, &[0; 6]);
    }

    fn get_nkro_bitmap(&self) -> [u8; 31] {
        let mut bits = [0u8; 31];
        for &k in &self.current_keys {
            if k > 0 && k < 248 {
                let byte = k as usize / 8;
                let bit = k as usize % 8;
                bits[byte] |= 1 << bit;
            }
        }
        bits
    }

    /// Defer EEPROM save (non-blocking — saved in main loop during idle ticks)
    fn save_config(&mut self) {
        self.save_pending = true;
    }

    fn process_custom_keycode(&mut self, kc: u16, pressed: bool, usb_hid: &mut UsbHid, mut flush: impl FnMut(&mut UartProtocol)) {
        match kc {
            keymap::KC_RF_DFU => {
                if pressed && self.proto.link_mode == LinkMode::Usb {
                    self.proto.build_link_cmd(wireless::uart::CMD_RF_DFU);
                }
            }
            keymap::KC_LNK_USB => {
                if pressed {
                    self.break_all_keys();
                } else {
                    self.proto.link_mode = LinkMode::Usb;
                    self.proto.build_link_cmd(CMD_SET_LINK);
                    flush(&mut self.proto);
                    self.side.blink_rf(3);
                }
            }
            k if (keymap::KC_LNK_RF..=keymap::KC_LNK_BLE3).contains(&k) => {
                let ch = lnk_to_channel(k);
                if pressed && self.proto.link_mode != LinkMode::Usb {
                    self.rf_sw_temp = ch as u8;
                    self.rf_sw_press = true;
                    self.rf_sw_press_delay = 0; // Reset delay on press
                    self.break_all_keys();
                } else if !pressed && self.rf_sw_press {
                    self.rf_sw_press = false;
                    if self.rf_sw_press_delay < 60 {
                        self.proto.link_mode = ch;
                        self.proto.rf_channel = ch as u8;
                        self.proto.ble_channel = ch as u8;
                        self.proto.build_link_cmd(CMD_SET_LINK);
                        flush(&mut self.proto);
                    }
                }
            }
            keymap::KC_SIDE_VAI => {
                if pressed { self.side.brightness = (self.side.brightness + 1).min(5); self.save_config(); }
            },
            keymap::KC_SIDE_VAD => {
                if pressed { self.side.brightness = self.side.brightness.saturating_sub(1); self.save_config(); }
            },
            keymap::KC_SIDE_MOD => {
                if pressed { self.side.mode = (self.side.mode + 1) % 5; self.save_config(); }
            },
            keymap::KC_SIDE_HUI => {
                if pressed { self.side.colour = (self.side.colour + 1) % 8; self.save_config(); }
            },
            keymap::KC_SIDE_SPI => {
                if pressed { self.side.speed = (self.side.speed + 1).min(4); self.save_config(); }
            },
            keymap::KC_SIDE_SPD => {
                if pressed { self.side.speed = self.side.speed.saturating_sub(1); self.save_config(); }
            },
            keymap::KC_DEV_RESET => {
                if pressed {
                    self.dev_reset_press = true;
                    self.break_all_keys();
                } else {
                    self.dev_reset_press = false;
                }
            }
            keymap::KC_SLEEP_MODE => {
                if pressed { self.sleep_enabled = !self.sleep_enabled; self.f_sleep_show = true; self.save_config(); }
            }
            keymap::KC_BAT_SHOW => if pressed { self.f_bat_hold = !self.f_bat_hold; },
            keymap::KC_RGB_TEST => self.rgb_test_press = pressed,
            keymap::KC_BAT_NUM => self.bat_num_show = pressed,

            // ── Consumer / media keys ───────────────────────────────
            k if keymap::is_consumer_key(k) => {
                let usage = keymap::consumer_usage(k);
                if pressed {
                    report::send_consumer(&mut self.proto, usage);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = usage;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(usage);
                    }
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0);
                    }
                }
            }

            // ── RGB matrix controls ────────────────────────────────────
            keymap::KC_RGB_SPD => if pressed { self.rgb.speed = self.rgb.speed.saturating_sub(16); self.save_config(); },
            keymap::KC_RGB_SPI => if pressed { self.rgb.speed = self.rgb.speed.saturating_add(16); self.save_config(); },
            keymap::KC_RGB_VAI => if pressed { self.rgb.val = self.rgb.val.saturating_add(16); if self.rgb.mode == 0 { self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val); } self.save_config(); },
            keymap::KC_RGB_VAD => if pressed { self.rgb.val = self.rgb.val.saturating_sub(16); if self.rgb.mode == 0 { self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val); } self.save_config(); },
            // 1:1 with C: RGB_MOD cycles to the next effect on press (BAT_SHOW now
            // owns fn+\, so the old tap-hold-for-battery behaviour is gone).
            keymap::KC_RGB_MOD => {
                if pressed {
                    self.rgb.next_mode();
                    if self.rgb.mode == 0 {
                        self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val);
                    }
                    self.save_config();
                }
            }
            keymap::KC_RGB_HUI => if pressed { self.rgb.hue = self.rgb.hue.wrapping_add(8); if self.rgb.mode == 0 { self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val); } self.save_config(); },

            keymap::KC_MAC_TASK => {
                if pressed {
                    report::send_consumer(&mut self.proto, 0x029F);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0x029F;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0x029F);
                    }
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0);
                    }
                }
            }
            keymap::KC_MAC_SEARCH => {
                if pressed {
                    // Tap: LGUI+Space press then immediate release (matches C register_code/unregister_code)
                    self.current_mods |= 0x08;
                    self.current_keys[0] = 0x2C;
                    if self.proto.link_mode == LinkMode::Usb {
                        if self.nkro_enabled {
                            let bits = self.get_nkro_bitmap();
                            usb_hid.send_nkro(self.current_mods, &bits);
                        } else {
                            usb_hid.send_keyboard(self.current_mods, &self.current_keys);
                        }
                    } else {
                        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                        flush(&mut self.proto);
                    }
                    
                    self.current_mods &= !0x08;
                    self.current_keys[0] = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        if self.nkro_enabled {
                            let bits = self.get_nkro_bitmap();
                            usb_hid.send_nkro(self.current_mods, &bits);
                        } else {
                            usb_hid.send_keyboard(self.current_mods, &self.current_keys);
                        }
                    } else {
                        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                        flush(&mut self.proto);
                    }
                }
            }
            keymap::KC_MAC_VOICE => {
                if pressed {
                    report::send_consumer(&mut self.proto, 0x00CF);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0x00CF;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0x00CF);
                    }
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0);
                    }
                }
            }
            keymap::KC_MAC_CONSOLE => {
                if pressed {
                    report::send_consumer(&mut self.proto, 0x02A0);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0x02A0;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0x02A0);
                    }
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    flush(&mut self.proto);
                    self.pending_consumer_usb = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_consumer(0);
                    }
                }
            }
            keymap::KC_MAC_DND => {
                if pressed {
                    report::send_system(&mut self.proto, 0x009B);
                    flush(&mut self.proto);
                    self.pending_system_usb = 0x009B;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_system(0x009B);
                    }
                } else {
                    report::send_system(&mut self.proto, 0);
                    flush(&mut self.proto);
                    self.pending_system_usb = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        usb_hid.send_system(0);
                    }
                }
            }
            keymap::KC_MAC_PRT => {
                if pressed {
                    // Tap: LGUI+LSFT+3 press then immediate release
                    self.current_mods |= 0x08 | 0x02;
                    self.current_keys[0] = 0x20;
                    if self.proto.link_mode == LinkMode::Usb {
                        if self.nkro_enabled {
                            let bits = self.get_nkro_bitmap();
                            usb_hid.send_nkro(self.current_mods, &bits);
                        } else {
                            usb_hid.send_keyboard(self.current_mods, &self.current_keys);
                        }
                    } else {
                        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                        flush(&mut self.proto);
                    }
                    
                    self.current_mods &= !(0x08 | 0x02);
                    self.current_keys[0] = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        if self.nkro_enabled {
                            let bits = self.get_nkro_bitmap();
                            usb_hid.send_nkro(self.current_mods, &bits);
                        } else {
                            usb_hid.send_keyboard(self.current_mods, &self.current_keys);
                        }
                    } else {
                        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                        flush(&mut self.proto);
                    }
                }
            }
            keymap::KC_MAC_PRTA => {
                if pressed {
                    // Tap: LGUI+LSFT+S (nkro) or LGUI+LSFT+4 (non-nkro)
                    self.current_mods |= 0x08 | 0x02;
                    if self.nkro_enabled {
                        self.current_keys[0] = 0x16;
                    } else {
                        self.current_keys[0] = 0x23;
                    }
                    if self.proto.link_mode == LinkMode::Usb {
                        if self.nkro_enabled {
                            let bits = self.get_nkro_bitmap();
                            usb_hid.send_nkro(self.current_mods, &bits);
                        } else {
                            usb_hid.send_keyboard(self.current_mods, &self.current_keys);
                        }
                    } else {
                        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                        flush(&mut self.proto);
                    }
                    
                    self.current_mods &= !(0x08 | 0x02);
                    self.current_keys[0] = 0;
                    if self.proto.link_mode == LinkMode::Usb {
                        if self.nkro_enabled {
                            let bits = self.get_nkro_bitmap();
                            usb_hid.send_nkro(self.current_mods, &bits);
                        } else {
                            usb_hid.send_keyboard(self.current_mods, &self.current_keys);
                        }
                    } else {
                        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                        flush(&mut self.proto);
                    }
                }
            }
            _ => {}
        }
    }

    fn process_key_event(&mut self, row: u8, col: u8, pressed: bool, usb_hid: &mut UsbHid, mut flush: impl FnMut(&mut UartProtocol)) {
        let ri = row as usize; let ci = col as usize;
        // On press: resolve + store keycode. On release: use stored keycode
        // so that releasing Fn before the modified key still uses the Fn-layer keycode.
        let kc = if pressed {
            self.sleep.on_activity();

            // Register hit for reactive RGB animations
            // (matrix_co_ordered bounds-checks internally and returns NO_LED for
            //  positions without an LED — no need to clamp col to 18 here.)
            let led_idx = led::animation::matrix_co_ordered(row as usize, col as usize);
            self.rgb.register_hit(led_idx, self.rgb.anim_tick);

            // Register keypress for typing heatmap frame buffer
            if self.rgb.mode == 48 {
                led::animation::typing_heatmap_keypress(&mut self.rgb, row, col);
            }

            let kc = resolve_keycode(&self.active_layers[..self.active_layer_count], ri, ci);
            self.keycode_track[ri][ci] = kc;
            kc
        } else {
            let kc = self.keycode_track[ri][ci];
            self.keycode_track[ri][ci] = 0;
            kc
        };
        if kc == keymap::KC_NO { return; }

        if let Some(layer) = mo_layer(kc) {
            if pressed && self.active_layer_count < 4 {
                self.active_layers[self.active_layer_count] = layer;
                self.active_layer_count += 1;
            } else if !pressed {
                if let Some(pos) = self.active_layers[..self.active_layer_count].iter().position(|&l| l == layer) {
                    for i in pos..self.active_layer_count - 1 { self.active_layers[i] = self.active_layers[i + 1]; }
                    self.active_layer_count -= 1;
                }
            }
            return;
        }

        if is_custom(kc) {
            self.process_custom_keycode(kc, pressed, usb_hid, flush);
            return;
        }

        let code = kc as u8;
        if pressed {
            match code {
                0xE0 => self.current_mods |= 0x01,
                0xE1 => self.current_mods |= 0x02,
                0xE2 => self.current_mods |= 0x04,
                0xE3 => self.current_mods |= 0x08,
                0xE4 => self.current_mods |= 0x10,
                0xE5 => self.current_mods |= 0x20,
                0xE6 => self.current_mods |= 0x40,
                0xE7 => self.current_mods |= 0x80,
                _ => {
                    for slot in &mut self.current_keys {
                        if *slot == 0 || *slot == code { *slot = code; break; }
                    }
                }
            }
        } else {
            match code {
                0xE0 => self.current_mods &= !0x01,
                0xE1 => self.current_mods &= !0x02,
                0xE2 => self.current_mods &= !0x04,
                0xE3 => self.current_mods &= !0x08,
                0xE4 => self.current_mods &= !0x10,
                0xE5 => self.current_mods &= !0x20,
                0xE6 => self.current_mods &= !0x40,
                0xE7 => self.current_mods &= !0x80,
                _ => {
                    for slot in &mut self.current_keys {
                        if *slot == code { *slot = 0; break; }
                    }
                }
            }
        }

        // Send NKRO report when enabled (wireless only, USB does 6KRO boot protocol)
        if self.nkro_enabled && self.proto.link_mode != LinkMode::Usb {
            let mut bits = [0u8; 32];
            if self.current_mods & 0x01 != 0 { bits[0] |= 0x01; } // Left Ctrl
            if self.current_mods & 0x02 != 0 { bits[0] |= 0x02; } // Left Shift
            if self.current_mods & 0x04 != 0 { bits[0] |= 0x04; } // Left Alt
            if self.current_mods & 0x08 != 0 { bits[0] |= 0x08; } // Left GUI
            if self.current_mods & 0x10 != 0 { bits[0] |= 0x10; } // Right Ctrl
            if self.current_mods & 0x20 != 0 { bits[0] |= 0x20; } // Right Shift
            if self.current_mods & 0x40 != 0 { bits[0] |= 0x40; } // Right Alt
            if self.current_mods & 0x80 != 0 { bits[0] |= 0x80; } // Right GUI
            for &k in &self.current_keys {
                if k > 0 && k < 248 {
                    let byte = (k as usize / 8) + 1;
                    let bit = k as usize % 8;
                    bits[byte] |= 1 << bit;
                }
            }
            report::send_nkro(&mut self.proto, &bits, flush);
        } else {
            report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
            flush(&mut self.proto);
        }
    }
}

// ── I2C PWM flush macro ────────────────────────────────────────────
// IS31FL3733 requires write-lock unlock (0xC5→0xFE) before page select.
macro_rules! pwm_flush {
    ($rgb:expr, $i2c:expr) => {{
        let d1 = $rgb.dirty1;
        let d2 = $rgb.dirty2;
        let (b1, b2) = $rgb.build_pwm_buffers();

        if d1 {
            // Unlock + select PWM page (0x01)
            let _ = $i2c.write(0x50, &[0xFE, 0xC5]);
            let _ = $i2c.write(0x50, &[0xFD, 0x01]);
            let mut tx_buf = [0u8; 193];
            tx_buf[0] = 0x00;
            tx_buf[1..].copy_from_slice(&b1);
            let _ = $i2c.write(0x50, &tx_buf);
        }

        if d2 {
            // Unlock + select PWM page again for second chip
            let _ = $i2c.write(0x53, &[0xFE, 0xC5]);
            let _ = $i2c.write(0x53, &[0xFD, 0x01]);
            let mut tx_buf = [0u8; 193];
            tx_buf[0] = 0x00;
            tx_buf[1..].copy_from_slice(&b2);
            let _ = $i2c.write(0x53, &tx_buf);
        }
        $rgb.dirty1 = false;
        $rgb.dirty2 = false;
    }};
}

// ── DFU magic check (QMK stm32_dfu pattern) ──────────────────────────
// Called at the VERY start of main(). Checks RTC backup register for
// DFU magic. If set, clears it and jumps to ROM bootloader.
// The magic is written by enter_bootloader() before a system reset.
const DFU_MAGIC: u32 = 0xDF0DF0DF;

fn check_dfu_magic_and_jump() {
    // Enable PWR clock (RCC_APB1ENR bit 28)
    const RCC_APB1ENR: *mut u32 = 0x4002_101C as *mut u32;
    unsafe {
        RCC_APB1ENR.write_volatile(RCC_APB1ENR.read_volatile() | (1 << 28));
    }

    // Enable backup domain access (PWR_CR.DBP = bit 8)
    const PWR_CR: *mut u32 = 0x4000_7000 as *mut u32;
    unsafe {
        PWR_CR.write_volatile(PWR_CR.read_volatile() | (1 << 8));
    }

    // Read RTC backup register 0
    const RTC_BKP0R: *const u32 = 0x4000_2850 as *const u32;
    let magic = unsafe { RTC_BKP0R.read_volatile() };

    if magic == DFU_MAGIC {
        // Clear the magic so we don't DFU-loop
        unsafe { (RTC_BKP0R as *mut u32).write_volatile(0); }
        // Full-cleanup jump to ROM bootloader
        unsafe { keyboard::matrix::Matrix::jump_to_bootloader(); }
    }
}

#[entry]
fn main() -> ! {
    // ── DFU magic check — must be FIRST before any init ──────────
    // QMK pattern: write magic to RTC backup register, system reset,
    // check magic on next boot, jump to ROM bootloader with full cleanup.
    check_dfu_magic_and_jump();

    let mut dp = pac::Peripherals::take().unwrap();
    let mut cp = CorePeripherals::take().unwrap();
    let mut rcc = dp.RCC.configure()
        .hsi48()
        .enable_crs(dp.CRS)
        .sysclk(48.mhz())
        .freeze(&mut dp.FLASH);

    // ── SysTick: 1ms tick ───────────────────────────────────────────
    // 48MHz / 1000 = 48000 cycles per tick.
    // MUST select the core clock: the cortex-m reset default is the external
    // reference clock, which on STM32F0 is HCLK/8 (6MHz). Without this, every
    // "1ms" tick is actually 8ms — making RGB animations ~8x too slow (static)
    // and stretching all debounce/sleep timings 8x. (stm32f0xx-hal's own Delay
    // sets Core for the same reason.)
    cp.SYST.set_clock_source(cortex_m::peripheral::syst::SystClkSource::Core);
    cp.SYST.set_reload(47999);
    cp.SYST.clear_current();
    cp.SYST.enable_counter();
    cp.SYST.enable_interrupt();

    // ── GPIO init ───────────────────────────────────────────────────
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    // GPIOD needed for matrix column D2 (pin 42 on 48-pin package)
    let _gpiod = dp.GPIOD.split(&mut rcc);

    // ── DFU entry check — hold Escape while plugging in USB ──────────
    // Must happen before any hardware init that could hang without DFU escape.
    unsafe { keyboard::matrix::Matrix::init_pins(); }
    if unsafe { keyboard::matrix::Matrix::check_escape_held() } {
        unsafe { keyboard::matrix::Matrix::enter_bootloader(); }
    }

    let (mut dc_boost, mut rgb_sdb1, mut rgb_sdb2, mut nrf_wakeup, mut nrf_reset, _nrf_boot, dev_mode, sys_mode) =
        cortex_m::interrupt::free(|cs| {
            let dc   = gpioc.pc2.into_push_pull_output(cs);
            let sdb1 = gpioc.pc6.into_push_pull_output(cs);
            let sdb2 = gpioc.pc7.into_push_pull_output(cs);
            let wake = gpioc.pc4.into_push_pull_output(cs);
            let rst  = gpiob.pb4.into_push_pull_output(cs);
            let boot = gpiob.pb5.into_pull_up_input(cs);
            let dev  = gpioc.pc0.into_floating_input(cs);
            let sys  = gpioc.pc1.into_floating_input(cs);
            (dc, sdb1, sdb2, wake, rst, boot, dev, sys)
        });

    dc_boost.set_high();
    rgb_sdb1.set_high();
    rgb_sdb2.set_high();
    nrf_wakeup.set_high();
    nrf_reset.set_low();
    // 50ms startup — non-blocking
    { let mut c = 50; while c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } c -= 1; } }
    nrf_reset.set_high();

    // ── UART: 460800 baud, 8E1 ──────────────────────────────────────
    let tx = cortex_m::interrupt::free(|cs| gpiob.pb6.into_alternate_af0(cs));
    let rx = cortex_m::interrupt::free(|cs| gpiob.pb7.into_alternate_af0(cs));
    let mut serial = Serial::usart1(dp.USART1, (tx, rx), 460_800.bps(), &mut rcc);

    unsafe {
        let usart1 = &*pac::USART1::ptr();
        // Disable USART to modify UE-locked control bits (like PCE and M0)
        usart1.cr1.modify(|_, w| w.ue().clear_bit());
        // Configure 9-bit word length (8 data bits + 1 parity bit) and enable parity control
        // Default PS is 0 (Even parity)
        usart1.cr1.modify(|_, w| w.pce().set_bit().m0().set_bit());
        // Re-enable USART
        usart1.cr1.modify(|_, w| w.ue().set_bit());
    }

    // ── UART TX macro (port of UART_Send_Bytes) ─────────────────────
    // Uses volatile register read for reliable busy-wait on Cortex-M0+
    // (DWT cycle counter not available on armv6m without manual DEMCR enable)
    macro_rules! uart_flush {
        ($proto:expr) => {{
            let len = $proto.tx_buf[3] as usize + 5;
            nrf_wakeup.set_low();
            // ~50µs: 50 * 10 loops with volatile read of SYST_CSR
            for _ in 0..500u32 { unsafe { core::ptr::read_volatile(0xE000_E010 as *const u32); } }
            for &b in &$proto.tx_buf[..len] {
                let _ = nb::block!(serial.write(b));
            }
            // ~50µs + len*32µs
            for _ in 0..(500u32 + len as u32 * 320) { unsafe { core::ptr::read_volatile(0xE000_E010 as *const u32); } }
            nrf_wakeup.set_high();
        }};
    }

    // ── UART RX wait macro with 2ms idle gap packet detection ──────
    macro_rules! uart_wait_rx {
        ($dev:expr, $ms:expr) => {{
            let mut _c = $ms as u32;
            let mut rx_idle = 0;
            while _c > 0 {
                while !tick_arrived() { cortex_m::asm::wfi(); }
                _c -= 1;
                while let Ok(b) = serial.read() {
                    $dev.proto.rx_queue_byte(b);
                    rx_idle = 0;
                }
                if $dev.proto.rx_len > 0 {
                    rx_idle += 1;
                    if rx_idle >= 2 {
                        let _ = $dev.proto.rx_finish();
                        rx_idle = 0;
                    }
                } else {
                    rx_idle = 0;
                }
            }
        }};
    }

    // ── Enable SYSCFG clock and configure Fm+ driving capability ──
    unsafe {
        let rcc_ptr = &*pac::RCC::ptr();
        rcc_ptr.apb2enr.modify(|_, w| w.syscfgen().set_bit());
    }
    dp.SYSCFG.cfgr1.modify(|_, w| w
        .i2c_pb8_fmp().set_bit()
        .i2c_pb9_fmp().set_bit()
        .i2c1_fmp().set_bit()
    );

    // ── I2C: 1MHz for IS31FL3733 ────────────────────────────────────
    let scl = cortex_m::interrupt::free(|cs| gpiob.pb8.into_alternate_af1(cs));
    let sda = cortex_m::interrupt::free(|cs| gpiob.pb9.into_alternate_af1(cs));
    let mut i2c = i2c::I2c::i2c1(dp.I2C1, (scl, sda), 1000.khz(), &mut rcc);

    // Overwrite the TIMINGR register with a correct 1 MHz setting for 8 MHz clock source
    unsafe {
        let i2c1 = &*pac::I2C1::ptr();
        i2c1.timingr.write(|w| w.bits(0x00000204));
    }

    // ── IS31FL3733 init (QMK sequence: clear LEDs → clear PWM → config → enable → delay) ──
    for &addr in &[0x50u8, 0x53u8] {
        // Step 1: Clear LED control registers (page 0)
        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x00]);
        for reg in 0x00u8..0x18 {
            let _ = i2c.write(addr, &[reg, 0x00]);
        }

        // Step 2: Clear all PWM registers (page 1)
        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x01]);
        let mut clear_buf = [0u8; 193];
        clear_buf[0] = 0x00; // register address
        let _ = i2c.write(addr, &clear_buf);

        // Step 3: Configure function registers (page 3)
        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x03]);
        let _ = i2c.write(addr, &[0x00, 0x01]); // Config: normal operation (bit 0 = disable shutdown)
        let _ = i2c.write(addr, &[0x01, 0xFF]); // GCC: max current
        let _ = i2c.write(addr, &[0x0F, 0x00]); // SW pull-up: 0 Ohm (default)
        let _ = i2c.write(addr, &[0x10, 0x00]); // CS pull-down: 0 Ohm (default)

        // Step 4: Enable all LED channels (page 0)
        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x00]);
        for reg in 0x00u8..0x18 {
            let _ = i2c.write(addr, &[reg, 0xFF]);
        }
    }

    // Step 5: Wait 10ms for oscillator startup / chip wake-up
    { let mut c = 10; while c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } c -= 1; } }

    // ── USB HID (wired mode) ────────────────────────────────────────
    let (usb_pa11, usb_pa12) = cortex_m::interrupt::free(|cs| {
        (gpioa.pa11.into_floating_input(cs), gpioa.pa12.into_floating_input(cs))
    });
    let usb_periph = stm32f0xx_hal::usb::Peripheral {
        usb: dp.USB,
        pin_dm: usb_pa11,
        pin_dp: usb_pa12,
    };
    let usb_bus = stm32f0xx_hal::usb::UsbBus::new(usb_periph);
    let mut usb_hid = UsbHid::new(&usb_bus);

    // ── Device state ─────────────────────────────────────────────────
    let mut dev = Device::new();

    // ── Load saved config from EEPROM ────────────────────────────────
    if let Some(cfg) = eeprom::load() {
        dev.side.mode = cfg.side_mode;
        dev.side.brightness = cfg.side_brightness;
        dev.side.speed = cfg.side_speed;
        dev.side.colour = cfg.side_colour;
        dev.side.rgb_enabled = cfg.side_rgb;
        dev.sleep_enabled = cfg.sleep_enable;

        dev.rgb.mode = cfg.rgb_mode;
        dev.rgb.hue = cfg.rgb_hue;
        dev.rgb.sat = cfg.rgb_sat;
        dev.rgb.val = cfg.rgb_val;
        dev.rgb.speed = cfg.rgb_speed;
        dev.rgb.enabled = cfg.rgb_enabled;
        if dev.rgb.mode == 0 {
            dev.rgb.set_hsv(cfg.rgb_hue, cfg.rgb_sat, cfg.rgb_val);
        }
        if !dev.rgb.enabled {
            dc_boost.set_low();
            rgb_sdb1.set_low();
            rgb_sdb2.set_low();
        }
    }

    // ── Initial dial scan with debounce ──────────────────────────────
    {
        let mut dial_dev = false;
        let mut dial_sys = false;
        for _ in 0..10 {
            let d = dev_mode.is_high().unwrap_or(false);
            let s = sys_mode.is_high().unwrap_or(false);
            if d != dial_dev || s != dial_sys {
                dial_dev = d;
                dial_sys = s;
            }
            while !tick_arrived() { cortex_m::asm::wfi(); }
        }
        if dial_dev {
            dev.proto.link_mode = LinkMode::Usb;
        } else {
            dev.proto.link_mode = LinkMode::from_u8(dev.proto.rf_channel);
        }
        if dial_sys {
            dev.proto.sys_sw_state = 0xA2;
            dev.active_layers[0] = 0;
            dev.nkro_enabled = false;
        } else {
            dev.proto.sys_sw_state = 0xA1;
            dev.active_layers[0] = 2;
            dev.nkro_enabled = true;
        }
        // Track initial stable state for debounce in main loop
        dev.dial_dev_last = dial_dev;
        dev.dial_sys_last = dial_sys;
        dev.f_dial_sw_init_ok = true;
    }

    // ── Matrix pin init ──────────────────────────────────────────────
    unsafe { Matrix::init_pins(); }

    // ── Startup: 100ms delay (L1 fix) ────────────────────────────────
    { let mut c = 100; while c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } c -= 1; } }

    // ── RF module init ───────────────────────────────────────────────
    for &cmd in &[CMD_HAND, CMD_READ_DATA, CMD_RF_STS_SYSC] {
        dev.proto.build_link_cmd(cmd);
        uart_flush!(&dev.proto);
        { let mut c = 5; while c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } c -= 1; } }
    }
    dev.proto.build_link_cmd(CMD_SET_NAME);
    uart_flush!(&dev.proto);
    { let mut c = 5; while c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } c -= 1; } }

    dev.proto.build_link_cmd(CMD_SET_24G_NAME);
    uart_flush!(&dev.proto);

    // ── Send CMD_SET_LINK to configure NRF for initial wireless mode ──
    if dev.proto.link_mode != LinkMode::Usb {
        dev.proto.build_link_cmd(CMD_SET_LINK);
        uart_flush!(&dev.proto);
    }

    // ── Main loop ────────────────────────────────────────────────────
    let mut t10: u32 = 0;
    let mut rgb_flush_timer: u32 = 0;
    let mut rgb_anim_timer: u32 = 0;
    let mut t50: u32 = 0;
    let mut t200: u32 = 0;
    let mut periodic_timer = 0u16;

    let mut rx_idle_ticks: u32 = 0;
    // Prescaler for USB-mode RF status sync (port of usb_sync_prescaler, rf.c:540-548).
    // C code sends CMD_RF_STS_SYSC every 10 calls in USB mode so the NRF keeps
    // reporting rf_battery/rf_charge/rf_led. Without this, the wireless fields
    // freeze and the Caps/Num indicators driven by rf_led stop working after
    // switching from wireless to USB.
    let mut usb_sync_prescaler: u8 = 0;

    loop {
        let tick = tick_arrived();

        // ── High-Performance Continuous Matrix Scan ─────────────────
        let events = dev.matrix.scan();
        let mut flush = |proto: &mut UartProtocol| {
            if proto.link_mode != LinkMode::Usb {
                uart_flush!(proto);
            }
        };

        // ── Immediate Wakeup on Key Activity ──────────────────────────
        if dev.sleep.f_wakeup_prepare && !events.is_empty() {
            dev.sleep.f_wakeup_prepare = false;
            dev.sleep.no_act_time = 0;
            dc_boost.set_high();
            rgb_sdb1.set_high();
            rgb_sdb2.set_high();
            dev.rgb.enabled = true;
            dev.proto.build_link_cmd(wireless::uart::CMD_HAND);
            flush(&mut dev.proto);
        }

        for ev in &events {
            dev.process_key_event(ev.row, ev.col, ev.pressed, &mut usb_hid, &mut flush);
        }

        // DO NOT MODIFY — USB report send path (NKRO + standard).
        // Restructuring this block or moving wireless dispatch here
        // breaks USB enumeration or wireless reports. See v4.0.2 regression.
        if !events.is_empty() && dev.proto.link_mode == LinkMode::Usb {
            // Wired mode: send either NKRO or standard keyboard report, not both,
            // to prevent endpoint collision and packet dropping.
            if dev.nkro_enabled {
                let bits = dev.get_nkro_bitmap();
                usb_hid.send_nkro(dev.current_mods, &bits);
            } else {
                usb_hid.send_keyboard(dev.current_mods, &dev.current_keys);
            }
        }

        // Always poll USB HID to flush reports/process tokens as fast as possible
        let _usb_configured = usb_hid.poll();

        // One-shot NumLock at boot so numpad works on Linux without manual toggle
        if !dev.boot_numlock_done && usb_hid.configured() {
            usb_hid.send_keyboard(0, &[0x53, 0, 0, 0, 0, 0]); // NumLock press
            usb_hid.poll(); // flush it
            usb_hid.send_keyboard(0, &[0; 6]); // release
            dev.boot_numlock_done = true;
        }

        let keyboard_leds = usb_hid.host_led_state();

        // ── 1ms Timing-Dependent Logic ──────────────────────────────
        if tick {
            dev.matrix.tick_debounce();

            // UART RX idle gap detection: call rx_finish after 2ms of no bytes
            if dev.proto.rx_len > 0 {
                rx_idle_ticks += 1;
                if rx_idle_ticks >= 2 {
                    let _ = dev.proto.rx_finish();
                    rx_idle_ticks = 0;
                }
            } else {
                rx_idle_ticks = 0;
            }

            // ── DFU entry: hold Escape alone (no other keys) for 3 seconds ──
            {
                let esc = dev.current_keys.contains(&0x29u8);
                let other = dev.current_keys.iter().any(|&k| k != 0 && k != 0x29);
                if esc && !other {
                    dev.dfu_hold_ticks = dev.dfu_hold_ticks.saturating_add(1);
                    if dev.dfu_hold_ticks >= 3000 {
                        unsafe { keyboard::matrix::Matrix::enter_bootloader(); }
                    }
                } else {
                    dev.dfu_hold_ticks = 0;
                }
            }

            // ── Dial switch read (debounced: 500ms stable before acting) ──
            // DO NOT MODIFY — Wireless/wired toggle via PC0 (DEV_MODE) / PC1 (SYS_MODE).
            // 500ms debounce, floating inputs (PCB has external pull-ups).
            // Changing pin mode or debounce timing breaks mode switching.
            if dev.f_dial_sw_init_ok {
                let dev_now = dev_mode.is_high().unwrap_or(false);
                let sys_now = sys_mode.is_high().unwrap_or(false);

                // Debounce DEV dial switch
                if dev_now != dev.dial_dev_last {
                    dev.dial_dev_debounce = 0;
                    dev.dial_dev_last = dev_now;
                } else if dev.dial_dev_debounce < 500 {
                    dev.dial_dev_debounce += 1;
                }

                // Debounce SYS dial switch
                if sys_now != dev.dial_sys_last {
                    dev.dial_sys_debounce = 0;
                    dev.dial_sys_last = sys_now;
                } else if dev.dial_sys_debounce < 500 {
                    dev.dial_sys_debounce += 1;
                }

                // Act on stable DEV state (>=500ms)
                if dev.dial_dev_debounce >= 500 {
                    if dev.dial_dev_last {
                        if dev.proto.link_mode != LinkMode::Usb {
                            dev.proto.link_mode = LinkMode::Usb;
                            dev.break_all_keys();
                            usb_hid.release_all();
                            dev.proto.f_send_channel = true;
                        }
                    } else {
                        let desired_ch = dev.proto.rf_channel;
                        if dev.proto.link_mode as u8 != desired_ch {
                            dev.proto.link_mode = LinkMode::from_u8(desired_ch);
                            dev.break_all_keys();
                            dev.proto.f_send_channel = true;
                        }
                    }
                }

                // Act on stable SYS state (>=500ms)
                if dev.dial_sys_debounce >= 500 {
                    if dev.dial_sys_last {
                        if dev.proto.sys_sw_state != 0xA2 {
                            dev.proto.sys_sw_state = 0xA2;
                            dev.active_layers[0] = 0;
                            dev.nkro_enabled = false;
                            dev.break_all_keys();
                            dev.side.show_sys();
                            if dev.proto.link_mode == LinkMode::Usb { usb_hid.release_all(); }
                        }
                    } else if dev.proto.sys_sw_state != 0xA1 {
                        dev.proto.sys_sw_state = 0xA1;
                        dev.active_layers[0] = 2;
                        dev.nkro_enabled = true;
                        dev.break_all_keys();
                        dev.side.show_sys();
                        if dev.proto.link_mode == LinkMode::Usb { usb_hid.release_all(); }
                    }
                }
            }

            t10 += 1;
            if t10 >= 10 { t10 = 0; dev.sleep.tick_10ms(); }

            t50 += 1;
            if t50 >= 50 {
                t50 = 0;
                if dev.proto.f_goto_sleep {
                    dev.proto.f_goto_sleep = false;
                    dev.sleep.f_goto_sleep = true;
                }

                let usb_suspended = usb_hid.is_suspended();
                let prev_wakeup = dev.sleep.f_wakeup_prepare;

                dev.sleep.tick(&mut dev.proto, usb_suspended);

                if !prev_wakeup && dev.sleep.f_wakeup_prepare {
                    // Entering sleep: power down GPIO rails and disable RGB matrix
                    dc_boost.set_low();
                    rgb_sdb1.set_low();
                    rgb_sdb2.set_low();
                    dev.rgb.enabled = false;
                }

                if prev_wakeup && !dev.sleep.f_wakeup_prepare {
                    // Waking up: restore GPIO rails and enable RGB matrix
                    dc_boost.set_high();
                    rgb_sdb1.set_high();
                    rgb_sdb2.set_high();
                    dev.rgb.enabled = true;
                }

                // ── Long press handler ────────────────────────────────
                if dev.rf_sw_press {
                    dev.rf_sw_press_delay += 1;
                    if dev.rf_sw_press_delay >= 60 {
                        dev.rf_sw_press = false;
                        let ch = dev.rf_sw_temp;
                        dev.proto.link_mode = LinkMode::from_u8(ch);
                        dev.proto.rf_channel = ch;
                        dev.proto.ble_channel = ch;
                        dev.proto.build_link_cmd(CMD_SET_LINK);
                        uart_flush!(&dev.proto);
                        uart_wait_rx!(dev, 20);
                        for _ in 0..5 {
                            dev.proto.build_link_cmd(wireless::uart::CMD_NEW_ADV);
                            uart_flush!(&dev.proto);
                            uart_wait_rx!(dev, 20);
                            if dev.proto.f_rf_new_adv_ok { break; }
                        }
                        dev.side.blink_rf(3);
                    }
                } else {
                    dev.rf_sw_press_delay = 0;
                }

                if dev.dev_reset_press {
                    dev.dev_reset_press_delay += 1;
                    if dev.dev_reset_press_delay >= 60 {
                        dev.dev_reset_press = false;
                        if dev.proto.link_mode != LinkMode::Usb {
                            if dev.proto.link_mode != LinkMode::Rf24 {
                                dev.proto.link_mode = LinkMode::Bt1;
                                dev.proto.ble_channel = 1;
                                dev.proto.rf_channel = 1;
                            }
                        } else {
                            dev.proto.ble_channel = 1;
                            dev.proto.rf_channel = 1;
                        }
                        dev.proto.build_link_cmd(CMD_SET_LINK);
                        uart_flush!(&dev.proto);
                        uart_wait_rx!(dev, 500);
                        dev.proto.build_link_cmd(wireless::uart::CMD_CLR_DEVICE);
                        uart_flush!(&dev.proto);

                        // Reset config (M15 fix: full device_reset_init)
                        dev.side.reset();
                        dev.rgb.enabled = true;
                        dev.rgb.hue = 0; dev.rgb.sat = 255;
                        dev.rgb.val = 255;   // RGB_MATRIX_DEFAULT_VAL
                        dev.rgb.speed = 255; // max speed (default)
                        dev.rgb.mode = 4;    // CYCLE_LEFT_RIGHT
                        dev.rgb.set_hsv(0, 255, 255);
                        dev.f_bat_hold = false;
                        dev.active_layers = [0, 0, 0, 0]; dev.active_layer_count = 1;
                        if dev.proto.sys_sw_state == 0xA2 {
                            dev.active_layers[0] = 0;
                        } else {
                            dev.active_layers[0] = 2;
                        }
                        // Save default config to EEPROM
                        dev.save_config();

                        // Start non-blocking blink state machine
                        dc_boost.set_high();
                        rgb_sdb1.set_high();
                        rgb_sdb2.set_high();
                        dev.reset_blink_phase = 1;  // first ON phase
                        dev.reset_blink_timer = 200;
                    }
                } else {
                    dev.dev_reset_press_delay = 0;
                }

                if dev.rgb_test_press {
                    dev.rgb_test_press_delay += 1;
                    if dev.rgb_test_press_delay >= 60 {
                        dev.rgb_test_press = false;
                        dc_boost.set_high();
                        rgb_sdb1.set_high();
                        rgb_sdb2.set_high();
                        dev.rgb_test_phase = 1; // first color
                        dev.rgb_test_timer = 500;
                    }
                } else {
                    dev.rgb_test_press_delay = 0;
                }

                if dev.rgb_mod_press {
                    dev.rgb_mod_press_delay += 1;
                    if dev.rgb_mod_press_delay >= 6 {
                        dev.f_bat_hold = true;
                    }
                } else {
                    dev.rgb_mod_press_delay = 0;
                }
            }

            t200 += 1;
            if t200 >= 200 {
                t200 = 0;

                // ── RF state sync (once every 200ms) ───────────────────────────────────
                if dev.proto.f_rf_reset {
                    dev.proto.f_rf_reset = false;
                    uart_wait_rx!(dev, 100);
                    nrf_reset.set_low();
                    uart_wait_rx!(dev, 50);
                    nrf_reset.set_high();
                    uart_wait_rx!(dev, 50);
                } else if dev.proto.f_send_channel {
                    dev.proto.f_send_channel = false;
                    dev.proto.build_link_cmd(CMD_SET_LINK);
                    uart_flush!(&dev.proto);
                }

                if dev.proto.link_mode != LinkMode::Usb {
                    // Wireless: poll NRF status every 200ms
                    dev.proto.build_link_cmd(CMD_RF_STS_SYSC);
                    uart_flush!(&dev.proto);
                    dev.proto.sync_lost += 1;
                    if dev.proto.sync_lost >= 10 {
                        // 10 missed syncs (≈2s) → hard reset NRF (matches C rf.c:551)
                        dev.proto.sync_lost = 0;
                        dev.proto.f_rf_reset = true;
                    }
                } else {
                    // USB: keep NRF reporting status every 10×200ms = 2s so the
                    // shared fields (rf_battery, rf_charge, rf_led) stay fresh.
                    // rf_led drives the Caps/Num lock side-LED indicators.
                    usb_sync_prescaler = usb_sync_prescaler.saturating_add(1);
                    if usb_sync_prescaler >= 10 {
                        usb_sync_prescaler = 0;
                        dev.proto.build_link_cmd(CMD_RF_STS_SYSC);
                        uart_flush!(&dev.proto);
                    }
                }

                // ── B1 blink guard + M14: 24G name on connect ─────────
                if dev.proto.rf_state != RfState::Connect {
                    if dev.proto.disconnect_delay >= 10 {
                        if dev.side.link_state_temp != dev.proto.rf_state as u8 {
                            dev.side.blink_rf(3);
                            dev.side.link_state_temp = dev.proto.rf_state as u8;
                        }
                    } else {
                        dev.proto.disconnect_delay += 1;
                    }
                } else {
                    dev.proto.disconnect_delay = 0;
                    let st = dev.proto.rf_state as u8;
                    if dev.side.link_state_temp != st {
                        if dev.proto.link_mode == LinkMode::Rf24 {
                            dev.proto.build_link_cmd(CMD_SET_24G_NAME);
                            uart_flush!(&dev.proto);
                        }
                        dev.side.link_state_temp = st;
                    }
                }
            }
        }

        // Device reset blink state machine (6 phases: on,off,on,off,on,off)
        if dev.reset_blink_phase > 0 && dev.reset_blink_phase <= 6 {
            let is_on = (dev.reset_blink_phase % 2) == 1;
            if is_on {
                dev.rgb.set_all(0xFF, 0xFF, 0xFF);
            } else {
                dev.rgb.set_all(0, 0, 0);
            }
            if rgb_flush_timer >= 10 { rgb_flush_timer = 0; pwm_flush!(&mut dev.rgb, i2c); }

            if tick {
                if dev.reset_blink_timer > 0 {
                    dev.reset_blink_timer -= 1;
                } else {
                    dev.reset_blink_phase += 1;
                    dev.reset_blink_timer = 200;
                    if dev.reset_blink_phase > 6 {
                        dev.reset_blink_phase = 0; // done
                        dev.rgb.set_all(0, 0, 0);
                    }
                }
            }
        }

        // RGB test state machine (7 colors × 500ms)
        if dev.rgb_test_phase >= 1 && dev.rgb_test_phase <= 7 {
            let colors: [(u8, u8, u8); 7] = [
                (0xFF, 0x00, 0x00),
                (0x00, 0xFF, 0x00),
                (0x00, 0x00, 0xFF),
                (0x80, 0x80, 0x80),
                (0x80, 0x80, 0x00),
                (0x80, 0x00, 0x80),
                (0x00, 0x80, 0x80),
            ];
            let (r, g, b) = colors[(dev.rgb_test_phase - 1) as usize];
            dev.rgb.set_all(r, g, b);
            if rgb_flush_timer >= 10 { rgb_flush_timer = 0; pwm_flush!(&mut dev.rgb, i2c); }

            if tick {
                if dev.rgb_test_timer > 0 {
                    dev.rgb_test_timer -= 1;
                } else {
                    dev.rgb_test_phase += 1;
                    dev.rgb_test_timer = 500;
                    if dev.rgb_test_phase > 7 {
                        dev.rgb_test_phase = 0;
                        dev.rgb.set_all(0, 0, 0);
                    }
                }
            }
        }

        // ── Side LED update + animation tick ──────────────────────────
        if tick {
            dev.side.update(&dev.proto, 1, keyboard_leds, &mut dev.f_bat_hold);

            // Advance animation clock by 1ms (matches C g_rgb_timer)
            dev.rgb.anim_tick = dev.rgb.anim_tick.wrapping_add(1);

            rgb_anim_timer += 1;
            if rgb_anim_timer >= 16 {
                rgb_anim_timer = 0;
                dev.rgb.tick_animation();
            }

            // Copy side LED output AFTER animation tick so it overrides
            // any colors set by animations on indices 100-109.
            for i in 0..10 {
                let [r, g, b] = dev.side.output[i];
                dev.rgb.set_color(100 + i, r, g, b);
            }

            // Per-key lock indicators: solid white, overrides any RGB profile/brightness
            // LED 55 = Caps Lock key (row 3, col 0), LED 33 = Num Lock key (row 1, col 14)
            let caps_on = (keyboard_leds & 0x02) != 0 || (dev.proto.rf_led & 0x02) != 0;
            let num_on  = (keyboard_leds & 0x01) != 0 || (dev.proto.rf_led & 0x01) != 0;
            if dev.rgb.caps_lock != caps_on {
                dev.rgb.caps_lock = caps_on;
                dev.rgb.dirty1 = true;
                dev.rgb.dirty2 = true;
            }
            if dev.rgb.num_lock != num_on {
                dev.rgb.num_lock = num_on;
                dev.rgb.dirty1 = true;
                dev.rgb.dirty2 = true;
            }
        }

        // ── BAT_NUM ──────────────────────────────────────────────────
        if dev.bat_num_show {
            let pct = dev.proto.rf_battery;
            let (r, g, b) = if pct <= 15 { (0xFF, 0x00, 0x00) }
            else if pct <= 50 { (0xFF, 0x40, 0x00) }
            else if pct <= 80 { (0xFF, 0xFF, 0x00) }
            else { (0x00, 0xFF, 0x00) };
            if pct >= 1  { dev.rgb.set_color(29, r, g, b); }
            if pct > 10 { dev.rgb.set_color(28, r, g, b); }
            if pct > 20 { dev.rgb.set_color(27, r, g, b); }
            if pct > 30 { dev.rgb.set_color(26, r, g, b); }
            if pct > 40 { dev.rgb.set_color(25, r, g, b); }
            if pct > 50 { dev.rgb.set_color(24, r, g, b); }
            if pct > 60 { dev.rgb.set_color(23, r, g, b); }
            if pct > 70 { dev.rgb.set_color(22, r, g, b); }
            if pct > 80 { dev.rgb.set_color(21, r, g, b); }
            if pct > 90 { dev.rgb.set_color(20, r, g, b); }
        }

        // ── Sleep indicator ──────────────────────────────────────────
        if dev.f_sleep_show {
            dev.f_sleep_show = false;
            dev.side.show_sleep(dev.sleep_enabled);
        }

        // ── RGB matrix I2C flush (chunked: 64 bytes per iteration to minimize scan blocking) ──
        if tick {
            rgb_flush_timer += 1;
        }
        if dev.i2c_flush_phase == 0 && dev.rgb.needs_flush() && rgb_flush_timer >= 10 {
            let (b1, b2) = dev.rgb.build_pwm_buffers();
            dev.pwm_buf1 = b1;
            dev.pwm_buf2 = b2;
            rgb_flush_timer = 0;
            dev.i2c_flush_phase = 1;
        }
        match dev.i2c_flush_phase {
            1 => {
                let _ = i2c.write(0x50, &[0xFE, 0xC5]);
                let _ = i2c.write(0x50, &[0xFD, 0x01]);
                dev.i2c_flush_phase = if dev.rgb.dirty1 { 2 } else { 5 };
            }
            2 => {
                let mut tx = [0u8; 65];
                tx[0] = 0x00;
                tx[1..].copy_from_slice(&dev.pwm_buf1[0..64]);
                let _ = i2c.write(0x50, &tx);
                dev.i2c_flush_phase = 3;
            }
            3 => {
                let mut tx = [0u8; 65];
                tx[0] = 0x40;
                tx[1..].copy_from_slice(&dev.pwm_buf1[64..128]);
                let _ = i2c.write(0x50, &tx);
                dev.i2c_flush_phase = 4;
            }
            4 => {
                let mut tx = [0u8; 65];
                tx[0] = 0x80;
                tx[1..].copy_from_slice(&dev.pwm_buf1[128..192]);
                let _ = i2c.write(0x50, &tx);
                dev.rgb.dirty1 = false;
                dev.i2c_flush_phase = 5;
            }
            5 => {
                let _ = i2c.write(0x53, &[0xFE, 0xC5]);
                let _ = i2c.write(0x53, &[0xFD, 0x01]);
                dev.i2c_flush_phase = if dev.rgb.dirty2 { 6 } else { 0 };
            }
            6 => {
                let mut tx = [0u8; 65];
                tx[0] = 0x00;
                tx[1..].copy_from_slice(&dev.pwm_buf2[0..64]);
                let _ = i2c.write(0x53, &tx);
                dev.i2c_flush_phase = 7;
            }
            7 => {
                let mut tx = [0u8; 65];
                tx[0] = 0x40;
                tx[1..].copy_from_slice(&dev.pwm_buf2[64..128]);
                let _ = i2c.write(0x53, &tx);
                dev.i2c_flush_phase = 8;
            }
            8 => {
                let mut tx = [0u8; 65];
                tx[0] = 0x80;
                tx[1..].copy_from_slice(&dev.pwm_buf2[128..192]);
                let _ = i2c.write(0x53, &tx);
                dev.rgb.dirty2 = false;
                dev.i2c_flush_phase = 0;
            }
            _ => {}
        }

        // ── Deferred EEPROM save (only when idle, no events) ──────────
        if dev.save_pending && events.is_empty() {
            dev.save_pending = false;
            let cfg = UserConfig {
                side_mode: dev.side.mode,
                side_brightness: dev.side.brightness,
                side_speed: dev.side.speed,
                side_colour: dev.side.colour,
                side_rgb: dev.side.rgb_enabled,
                sleep_enable: dev.sleep_enabled,
                rgb_mode: dev.rgb.mode,
                rgb_hue: dev.rgb.hue,
                rgb_sat: dev.rgb.sat,
                rgb_val: dev.rgb.val,
                rgb_speed: dev.rgb.speed,
                rgb_enabled: dev.rgb.enabled,
            };
            eeprom::save(&cfg);
        }

        // ── Periodic sender (every 200ms) ────────────────────────────
        if tick {
            periodic_timer += 1;
            if periodic_timer >= 200 && dev.proto.link_mode != LinkMode::Usb {
                periodic_timer = 0;
                if dev.sleep.no_act_time <= 2000 {
                    let report = dev.proto.bytekb_report_buf;
                    dev.proto.build_report(wireless::uart::CMD_RPT_BYTE_KB, &report, 8);
                    uart_flush!(&dev.proto);
                }
            }
        }

        // ── UART RX (drain all available bytes immediately) ──────────
        while let Ok(byte) = serial.read() {
            dev.proto.rx_queue_byte(byte);
            rx_idle_ticks = 0;
        }

        // Halt CPU in deep standby while sleeping to reduce power draw by >99%
        if dev.sleep.f_wakeup_prepare {
            cortex_m::asm::wfi();
        }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::wfe(); }
}

