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
static mut TICK_FLAG: bool = false;

/// Wait for a SysTick interrupt. Returns true when tick received.
fn tick_arrived() -> bool {
    unsafe {
        if TICK_FLAG {
            TICK_FLAG = false;
            true
        } else {
            false
        }
    }
}

#[exception]
fn SysTick() {
    unsafe { TICK_FLAG = true; }
}

struct Device {
    proto: UartProtocol,
    sleep: SleepManager,
    matrix: Matrix,
    side: SideLeds,
    rgb: RgbMatrix,
    active_layers: [usize; 4],
    active_layer_count: usize,
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
    f_sys_show: bool,
    f_sleep_show: bool,
    f_bat_hold: bool,
    dfu_hold_ticks: u16,
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
    pending_system_usb: u8,
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
            current_keys: [0; 6],
            current_mods: 0,
            nkro_enabled: true,
            rf_sw_press: false, rf_sw_press_delay: 0, rf_sw_temp: 0,
            dev_reset_press: false, dev_reset_press_delay: 0,
            rgb_test_press: false, rgb_test_press_delay: 0,
            f_sys_show: false, f_sleep_show: false, f_bat_hold: false,
            dfu_hold_ticks: 0,
            sleep_enabled: true, bat_num_show: false,
            reset_blink_phase: 0, reset_blink_timer: 0,
            rgb_test_phase: 0, rgb_test_timer: 0,
            pending_consumer_usb: 0,
            pending_system_usb: 0,
        }
    }

    fn break_all_keys(&mut self) {
        self.current_mods = 0;
        self.current_keys = [0; 6];
        report::send_keyboard(&mut self.proto, 0, &[0; 6]);
    }

    /// Save side LED config to EEPROM
    fn save_config(&self) {
        let cfg = UserConfig {
            side_mode: self.side.mode,
            side_brightness: self.side.brightness,
            side_speed: self.side.speed,
            side_colour: self.side.colour,
            side_rgb: self.side.rgb_enabled,
            sleep_enable: self.sleep_enabled,
        };
        eeprom::save(&cfg);
    }

    fn process_custom_keycode(&mut self, kc: u16, pressed: bool) {
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
                    self.side.blink_rf(3);
                }
            }
            k if (keymap::KC_LNK_RF..=keymap::KC_LNK_BLE3).contains(&k) => {
                let ch = lnk_to_channel(k);
                if pressed && self.proto.link_mode != LinkMode::Usb {
                    self.rf_sw_temp = ch as u8;
                    self.rf_sw_press = true;
                    self.break_all_keys();
                } else if !pressed && self.rf_sw_press {
                    self.rf_sw_press = false;
                    if self.rf_sw_press_delay < 60 {
                        self.proto.link_mode = ch;
                        self.proto.rf_channel = ch as u8;
                        self.proto.ble_channel = ch as u8;
                        self.proto.build_link_cmd(CMD_SET_LINK);
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
            keymap::KC_DEV_RESET => self.dev_reset_press = pressed,
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
                    self.pending_consumer_usb = usage;
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    self.pending_consumer_usb = 0;
                }
            }

            // ── RGB matrix controls ────────────────────────────────────
            keymap::KC_RGB_SPD => if pressed { self.rgb.speed = self.rgb.speed.saturating_sub(16); },
            keymap::KC_RGB_SPI => if pressed { self.rgb.speed = (self.rgb.speed + 16).min(255); },
            keymap::KC_RGB_VAI => if pressed { self.rgb.val = (self.rgb.val + 16).min(255); self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val); },
            keymap::KC_RGB_VAD => if pressed { self.rgb.val = self.rgb.val.saturating_sub(16); self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val); },
            keymap::KC_RGB_MOD => if pressed { self.rgb.mode = (self.rgb.mode + 1) % 10; },
            keymap::KC_RGB_HUI => if pressed { self.rgb.hue = self.rgb.hue.wrapping_add(16); self.rgb.set_hsv(self.rgb.hue, self.rgb.sat, self.rgb.val); },

            keymap::KC_MAC_TASK => {
                if pressed {
                    report::send_consumer(&mut self.proto, 0x029F);
                    self.pending_consumer_usb = 0x029F;
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    self.pending_consumer_usb = 0;
                }
            }
            keymap::KC_MAC_SEARCH => {
                if pressed {
                    self.current_mods |= 0x08;
                    self.current_keys[0] = 0x2C;
                    report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                } else {
                    self.current_mods &= !0x08;
                    self.current_keys[0] = 0;
                    report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                }
            }
            keymap::KC_MAC_VOICE => {
                if pressed {
                    report::send_consumer(&mut self.proto, 0x00CF);
                    self.pending_consumer_usb = 0x00CF;
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    self.pending_consumer_usb = 0;
                }
            }
            keymap::KC_MAC_CONSOLE => {
                if pressed {
                    report::send_consumer(&mut self.proto, 0x02A0);
                    self.pending_consumer_usb = 0x02A0;
                } else {
                    report::send_consumer(&mut self.proto, 0);
                    self.pending_consumer_usb = 0;
                }
            }
            keymap::KC_MAC_DND => {
                if pressed {
                    report::send_system(&mut self.proto, 0x009B);
                    self.pending_system_usb = 0x9B;
                } else {
                    report::send_system(&mut self.proto, 0);
                    self.pending_system_usb = 0;
                }
            }
            keymap::KC_MAC_PRT => {
                if pressed {
                    self.current_mods |= 0x08 | 0x02;
                    self.current_keys[0] = 0x20;
                    report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                } else {
                    self.current_mods &= !(0x08 | 0x02);
                    self.current_keys[0] = 0;
                    report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                }
            }
            keymap::KC_MAC_PRTA => {
                if pressed {
                    self.current_mods |= 0x08 | 0x02;
                    if self.nkro_enabled {
                        self.current_keys[0] = 0x16;
                    } else {
                        self.current_keys[0] = 0x23;
                    }
                    report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                } else {
                    self.current_mods &= !(0x08 | 0x02);
                    self.current_keys[0] = 0;
                    report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
                }
            }
            _ => {}
        }
    }

    fn process_key_event(&mut self, row: u8, col: u8, pressed: bool) {
        self.sleep.on_activity();
        let kc = resolve_keycode(&self.active_layers[..self.active_layer_count], row as usize, col as usize);
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
            self.process_custom_keycode(kc, pressed);
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

        report::send_keyboard(&mut self.proto, self.current_mods, &self.current_keys);
    }
}

// ── I2C PWM flush macro ────────────────────────────────────────────
macro_rules! pwm_flush {
    ($rgb:expr, $i2c:expr) => {{
        let (b1, b2) = $rgb.build_pwm_buffers();
        let _ = $i2c.write(0x50, &[0xFD, 0x01]);
        let _ = $i2c.write(0x50, &[0x00]);
        for chunk in b1.chunks(64) { let _ = $i2c.write(0x50, chunk); }
        let _ = $i2c.write(0x53, &[0xFD, 0x01]);
        let _ = $i2c.write(0x53, &[0x00]);
        for chunk in b2.chunks(64) { let _ = $i2c.write(0x53, chunk); }
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
    // 48MHz / 1000 = 48000 cycles per tick
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
        usart1.cr1.modify(|_, w| w.pce().set_bit().m0().set_bit());
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

    // ── I2C: 1MHz for IS31FL3733 ────────────────────────────────────
    let scl = cortex_m::interrupt::free(|cs| gpiob.pb8.into_alternate_af1(cs));
    let sda = cortex_m::interrupt::free(|cs| gpiob.pb9.into_alternate_af1(cs));
    let mut i2c = i2c::I2c::i2c1(dp.I2C1, (scl, sda), 1000.khz(), &mut rcc);

    // ── IS31FL3733 init ─────────────────────────────────────────────
    for &addr in &[0x50u8, 0x53u8] {
        // Function registers (page 3)
        let _ = i2c.write(addr, &[0xFD, 0x03]);
        let _ = i2c.write(addr, &[0x00, 0x01]); // config: normal operation
        let _ = i2c.write(addr, &[0x01, 0xFF]); // GCC: max current
        let _ = i2c.write(addr, &[0x0E, 0x01]); // SW pull-up
        let _ = i2c.write(addr, &[0x0F, 0x01]); // CS pull-down
        // LED on/off (page 0) — enable all channels
        let _ = i2c.write(addr, &[0xFD, 0x00]);
        for reg in 0x00u8..0x18 {
            let _ = i2c.write(addr, &[reg, 0xFF]);
        }
    }

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

    // ── Main loop ────────────────────────────────────────────────────
    let mut t10: u32 = 0;
    let mut t50: u32 = 0;
    let mut periodic_timer: u32 = 0;

    loop {
        // ── Wait for 1ms SysTick ──────────────────────────────────
        while !tick_arrived() {
            cortex_m::asm::wfi();
        }

        // ── Matrix scan ──────────────────────────────────────────
        let events = dev.matrix.scan();
        for ev in &events {
            dev.process_key_event(ev.row, ev.col, ev.pressed);
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
        if events.len() > 0 {
            if dev.proto.link_mode == LinkMode::Usb {
                // Wired mode: send keyboard + consumer + system via USB HID
                usb_hid.send_keyboard(dev.current_mods, &dev.current_keys);
                if dev.pending_consumer_usb != 0 {
                    usb_hid.send_consumer(dev.pending_consumer_usb);
                }
                if dev.pending_system_usb != 0 {
                    usb_hid.send_system(dev.pending_system_usb);
                }
            } else {
                uart_flush!(&dev.proto);
            }
        } else if dev.proto.link_mode == LinkMode::Usb {
            if dev.pending_consumer_usb != 0 {
                usb_hid.send_consumer(dev.pending_consumer_usb);
            }
            if dev.pending_system_usb != 0 {
                usb_hid.send_system(dev.pending_system_usb);
            }
        }

        // ── USB HID poll ──────────────────────────────────────────
        let _usb_configured = usb_hid.poll();

        // ── Dial switch read ──────────────────────────────────────
        let dev_now = dev_mode.is_high().unwrap_or(false);
        let sys_now = sys_mode.is_high().unwrap_or(false);

        if sys_now {
            if dev.proto.sys_sw_state != 0xA2 {
                dev.proto.sys_sw_state = 0xA2;
                dev.active_layers[0] = 0;
                dev.nkro_enabled = false;
                dev.break_all_keys();
                dev.side.show_sys();
                // USB: send empty report after mode switch
                if dev.proto.link_mode == LinkMode::Usb { usb_hid.release_all(); }
            }
        } else {
            if dev.proto.sys_sw_state != 0xA1 {
                dev.proto.sys_sw_state = 0xA1;
                dev.active_layers[0] = 2;
                dev.nkro_enabled = true;
                dev.break_all_keys();
                dev.side.show_sys();
                if dev.proto.link_mode == LinkMode::Usb { usb_hid.release_all(); }
            }
        }

        if dev_now {
            if dev.proto.link_mode != LinkMode::Usb {
                dev.proto.link_mode = LinkMode::Usb;
                dev.break_all_keys();
                usb_hid.release_all();
            }
        } else {
            let desired_ch = dev.proto.rf_channel;
            if dev.proto.link_mode as u8 != desired_ch {
                dev.proto.link_mode = LinkMode::from_u8(desired_ch);
                dev.break_all_keys();
            }
        }

        t10 += 1;
        if t10 >= 10 { t10 = 0; dev.sleep.tick_10ms(); }

        t50 += 1;
        if t50 >= 50 {
            t50 = 0;
            dev.sleep.tick(&mut dev.proto, false);

            // ── Long press handler ────────────────────────────────
            if dev.rf_sw_press {
                dev.rf_sw_press_delay += 1;
                if dev.rf_sw_press_delay >= 60 {
                    dev.rf_sw_press = false;
                    let ch = dev.rf_sw_temp;
                    dev.proto.link_mode = LinkMode::from_u8(ch);
                    dev.proto.rf_channel = ch;
                    dev.proto.ble_channel = ch;
                    for _ in 0..5 {
                        dev.proto.build_link_cmd(wireless::uart::CMD_NEW_ADV);
                        uart_flush!(&dev.proto);
                        // 20ms wait with UART RX
                        { let mut _c = 20u32; while _c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } _c -= 1; if let Ok(b) = serial.read() { dev.proto.rx_queue_byte(b); } else { let _ = dev.proto.rx_finish(); } } };
                        if dev.proto.f_rf_new_adv_ok { break; }
                    }
                }
            } else {
                dev.rf_sw_press_delay = 0;
            }

            if dev.dev_reset_press {
                dev.dev_reset_press_delay += 1;
                if dev.dev_reset_press_delay >= 60 {
                    dev.dev_reset_press = false;
                    if dev.proto.link_mode != LinkMode::Usb {
                        dev.proto.link_mode = LinkMode::Bt1;
                        dev.proto.ble_channel = 1;
                        dev.proto.rf_channel = 1;
                    } else {
                        dev.proto.ble_channel = 1;
                        dev.proto.rf_channel = 1;
                    }
                    dev.proto.build_link_cmd(CMD_SET_LINK);
                    uart_flush!(&dev.proto);
                    { let mut _c = 500u32; while _c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } _c -= 1; if let Ok(b) = serial.read() { dev.proto.rx_queue_byte(b); } else { let _ = dev.proto.rx_finish(); } } };
                    dev.proto.build_link_cmd(wireless::uart::CMD_CLR_DEVICE);
                    uart_flush!(&dev.proto);

                    // Reset config (M15 fix: full device_reset_init)
                    dev.side.reset();
                    dev.rgb.enabled = true; dev.rgb.set_hsv(255, 255, 128);
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

            // ── RF state sync ────────────────────────────────────
            if dev.proto.f_rf_reset {
                dev.proto.f_rf_reset = false;
                { let mut _c = 100u32; while _c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } _c -= 1; if let Ok(b) = serial.read() { dev.proto.rx_queue_byte(b); } else { let _ = dev.proto.rx_finish(); } } };
                nrf_reset.set_low();
                { let mut _c = 50u32; while _c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } _c -= 1; if let Ok(b) = serial.read() { dev.proto.rx_queue_byte(b); } else { let _ = dev.proto.rx_finish(); } } };
                nrf_reset.set_high();
                { let mut _c = 50u32; while _c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } _c -= 1; if let Ok(b) = serial.read() { dev.proto.rx_queue_byte(b); } else { let _ = dev.proto.rx_finish(); } } };
            } else if dev.proto.f_send_channel {
                dev.proto.f_send_channel = false;
                dev.proto.build_link_cmd(CMD_SET_LINK);
                uart_flush!(&dev.proto);
            }

            if dev.proto.link_mode != LinkMode::Usb {
                dev.proto.build_link_cmd(CMD_RF_STS_SYSC);
                uart_flush!(&dev.proto);
                dev.proto.sync_lost += 1;
                if dev.proto.sync_lost >= 5 {
                    dev.proto.sync_lost = 0;
                    dev.proto.f_rf_reset = true;
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

        // ── Non-blocking state machines (run every 1ms) ──────────────

        // Device reset blink state machine (6 phases: on,off,on,off,on,off)
        if dev.reset_blink_phase > 0 && dev.reset_blink_phase <= 6 {
            let is_on = (dev.reset_blink_phase % 2) == 1;
            if is_on {
                dev.rgb.set_all(0xFF, 0xFF, 0xFF);
            } else {
                dev.rgb.set_all(0, 0, 0);
            }
            pwm_flush!(&mut dev.rgb, i2c);

            if dev.reset_blink_timer > 0 {
                dev.reset_blink_timer -= 1;
            } else {
                dev.reset_blink_phase += 1;
                dev.reset_blink_timer = 200;
                if dev.reset_blink_phase > 6 {
                    dev.reset_blink_phase = 0; // done
                    dev.rgb.set_all(0, 0, 0);
                    pwm_flush!(&mut dev.rgb, i2c);
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
            pwm_flush!(&mut dev.rgb, i2c);

            if dev.rgb_test_timer > 0 {
                dev.rgb_test_timer -= 1;
            } else {
                dev.rgb_test_phase += 1;
                dev.rgb_test_timer = 500;
                if dev.rgb_test_phase > 7 {
                    dev.rgb_test_phase = 0;
                    dev.rgb.set_all(0, 0, 0);
                    pwm_flush!(&mut dev.rgb, i2c);
                }
            }
        }

        // ── Side LED update ─────────────────────────────────────────
        dev.side.update(&dev.proto, 1, false);
        for i in 0..10 {
            let [r, g, b] = dev.side.output[i];
            dev.rgb.set_color(100 + i, r, g, b);
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

        // ── RGB matrix I2C flush ─────────────────────────────────────
        if dev.rgb.needs_flush() {
            pwm_flush!(&mut dev.rgb, i2c);
        }

        // ── Periodic sender (every 200ms) ────────────────────────────
        periodic_timer += 1;
        if periodic_timer >= 200 && dev.proto.link_mode != LinkMode::Usb {
            periodic_timer = 0;
            if dev.sleep.no_act_time <= 2000 {
                let report = dev.proto.bytekb_report_buf;
                dev.proto.build_report(wireless::uart::CMD_RPT_BYTE_KB, &report, 8);
                uart_flush!(&dev.proto);
            }
        }

        // ── UART RX ─────────────────────────────────────────────────
        if let Ok(byte) = serial.read() {
            dev.proto.rx_queue_byte(byte);
        } else {
            let _ = dev.proto.rx_finish();
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::wfe(); }
}
