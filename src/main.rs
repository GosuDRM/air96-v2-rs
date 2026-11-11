#![no_std]
#![no_main]

//! Board bring-up and main loop. All keyboard behaviour lives in the library
//! (`kb`, `wireless`, `led::side`), ported 1:1 from the C firmware; this file
//! supplies the hardware behind `hal::Hal` and runs the QMK-shaped loop.

use cortex_m::peripheral::Peripherals as CorePeripherals;
use cortex_m_rt::{entry, exception};

use stm32f0xx_hal::{i2c, pac, pac::interrupt, prelude::*, serial::Serial};

use air96_v2::config::eeprom::{self, UserConfig};
use air96_v2::hal::{Hal, UsbDeviceState};
use air96_v2::kb::Kb;
use air96_v2::keyboard::matrix::Matrix;
use air96_v2::led::rgb::RgbMatrix;
use air96_v2::led::rgb_matrix::TaskEvent;
use air96_v2::usb_hid::{Report, UsbHid};

// ── SysTick: 1ms tick + millisecond clock (timer_read32) ───────────────
static mut TICK_FLAG: bool = false;
static mut MILLIS: u32 = 0;

/// Returns true once per elapsed SysTick.
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

/// `timer_read32()`.
fn millis() -> u32 {
    cortex_m::interrupt::free(|_| unsafe { MILLIS })
}

#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|_| unsafe {
        TICK_FLAG = true;
        MILLIS = MILLIS.wrapping_add(1);
    });
}

// ── USART1 RX: interrupt-driven ring buffer (ChibiOS serial driver) ────
static mut RX_BUF: [u8; 256] = [0; 256];
static mut RX_HEAD: u8 = 0;
static mut RX_TAIL: u8 = 0;

#[interrupt]
fn USART1() {
    let usart = unsafe { &*pac::USART1::ptr() };
    let isr = usart.isr.read();
    if isr.rxne().bit_is_set() {
        // 8E1: 9-bit word including parity; the data byte is the low 8 bits.
        let b = usart.rdr.read().bits() as u8;
        unsafe {
            let next = RX_HEAD.wrapping_add(1);
            if next != RX_TAIL {
                RX_BUF[RX_HEAD as usize] = b;
                RX_HEAD = next;
            }
        }
    }
    // Clear parity / framing / noise / overrun errors (PECF|FECF|NCF|ORECF).
    if isr.bits() & 0x0F != 0 {
        usart.icr.write(|w| unsafe { w.bits(0x0F) });
    }
}

fn rx_pop() -> Option<u8> {
    cortex_m::interrupt::free(|_| unsafe {
        if RX_TAIL == RX_HEAD {
            None
        } else {
            let b = RX_BUF[RX_TAIL as usize];
            RX_TAIL = RX_TAIL.wrapping_add(1);
            Some(b)
        }
    })
}

// ── Raw GPIO for board control pins (config.h) ─────────────────────────
const GPIOB: u32 = 0x4800_0400;
const GPIOC: u32 = 0x4800_0800;
const DC_BOOST: (u32, u8) = (GPIOC, 2);
const RGB_DRIVER_SDB1: (u32, u8) = (GPIOC, 6);
const RGB_DRIVER_SDB2: (u32, u8) = (GPIOC, 7);
const NRF_WAKEUP: (u32, u8) = (GPIOC, 4);
const NRF_RESET: (u32, u8) = (GPIOB, 4);
const NRF_BOOT: (u32, u8) = (GPIOB, 5);
const DEV_MODE: (u32, u8) = (GPIOC, 0);
const SYS_MODE: (u32, u8) = (GPIOC, 1);

fn write_pin(pin: (u32, u8), high: bool) {
    let reg = if high { pin.0 + 0x18 } else { pin.0 + 0x28 }; // BSRR / BRR
    unsafe { (reg as *mut u32).write_volatile(1 << pin.1) };
}

fn read_pin(pin: (u32, u8)) -> bool {
    unsafe { ((pin.0 + 0x10) as *const u32).read_volatile() & (1 << pin.1) != 0 }
}

/// `setPinOutput()`.
fn set_pin_output(pin: (u32, u8)) {
    unsafe {
        let moder = pin.0 as *mut u32;
        let v = moder.read_volatile() & !(0b11 << (pin.1 * 2));
        moder.write_volatile(v | (0b01 << (pin.1 * 2)));
    }
}

/// `setPinInputHigh()`.
fn set_pin_input_high(pin: (u32, u8)) {
    unsafe {
        let moder = pin.0 as *mut u32;
        moder.write_volatile(moder.read_volatile() & !(0b11 << (pin.1 * 2)));
        let pupdr = (pin.0 + 0x0C) as *mut u32;
        let v = pupdr.read_volatile() & !(0b11 << (pin.1 * 2));
        pupdr.write_volatile(v | (0b01 << (pin.1 * 2)));
    }
}

fn delay_us(us: u32) {
    cortex_m::asm::delay(us * 48);
}

// ── IS31FL3733 PWM flush ───────────────────────────────────────────────
const DRIVER_ADDR_1: u8 = 0x50;
const DRIVER_ADDR_2: u8 = 0x53;

// ── USB: serviced from its interrupt, like the ChibiOS USB driver ───────
// The stack lives in a static; the interrupt and the main loop each borrow
// it only inside a critical section, so the two never overlap.
static mut USB_PTR: *mut UsbHid<'static> = core::ptr::null_mut();

/// Run `f` on the USB stack with the USB interrupt held off.
fn with_usb<R>(f: impl FnOnce(&mut UsbHid<'static>) -> R) -> R {
    cortex_m::interrupt::free(|_| unsafe { f(&mut *USB_PTR) })
}

#[interrupt]
fn USB() {
    unsafe {
        let p = USB_PTR;
        if !p.is_null() {
            (*p).poll();
        }
    }
}

/// `send_report()`: wait for endpoint space with interrupts enabled, so the
/// USB interrupt drains the endpoint and the 100ms timeout clock runs.
fn send_report(r: Report) {
    let start = millis();
    while with_usb(|u| u.send_step(&r, start)).is_none() {}
}

struct Board<I> {
    i2c: I,
    /// Deferred `eeconfig_update_user_datablock()` (a flash page erase stalls
    /// the CPU, so it waits for an idle loop iteration).
    pending_cfg: Option<UserConfig>,
    flush_phase: u8,
    /// Driver dirty flags captured with the PWM snapshot: changes made while
    /// the snapshot is being written set `rgb.dirty*` again for the next flush.
    flush_dirty: (bool, bool),
    pwm_buf1: [u8; 192],
    pwm_buf2: [u8; 192],
}

impl<I: _embedded_hal_blocking_i2c_Write> Board<I> {
    /// Write deferred settings to flash before a reset.
    fn persist(&mut self) {
        if let Some(cfg) = self.pending_cfg.take() {
            eeprom::save(&cfg);
        }
        eeprom::flush();
    }

    /// `is31fl3733_flush_continue()`: one I2C write per call so scanning keeps
    /// up; returns true once both drivers' dirty buffers are written.
    #[inline(never)]
    fn flush_continue(&mut self, rgb: &mut RgbMatrix) -> bool {
        if self.flush_phase == 0 {
            if !rgb.needs_flush() {
                return true;
            }
            let (b1, b2) = rgb.build_pwm_buffers();
            self.pwm_buf1 = b1;
            self.pwm_buf2 = b2;
            self.flush_dirty = (rgb.dirty1, rgb.dirty2);
            rgb.dirty1 = false;
            rgb.dirty2 = false;
            self.flush_phase = 1;
        }
        let chunk = |buf: &[u8; 192], part: usize| {
            let mut tx = [0u8; 65];
            tx[0] = (part * 64) as u8;
            tx[1..].copy_from_slice(&buf[part * 64..part * 64 + 64]);
            tx
        };
        match self.flush_phase {
            1 => {
                if self.flush_dirty.0 {
                    let _ = self.i2c.write(DRIVER_ADDR_1, &[0xFE, 0xC5]);
                    let _ = self.i2c.write(DRIVER_ADDR_1, &[0xFD, 0x01]);
                    self.flush_phase = 2;
                } else {
                    self.flush_phase = 5;
                }
            }
            2..=4 => {
                let tx = chunk(&self.pwm_buf1, (self.flush_phase - 2) as usize);
                let _ = self.i2c.write(DRIVER_ADDR_1, &tx);
                self.flush_phase += 1;
            }
            5 => {
                if self.flush_dirty.1 {
                    let _ = self.i2c.write(DRIVER_ADDR_2, &[0xFE, 0xC5]);
                    let _ = self.i2c.write(DRIVER_ADDR_2, &[0xFD, 0x01]);
                    self.flush_phase = 6;
                } else {
                    self.flush_phase = 0;
                    return true;
                }
            }
            6..=8 => {
                let tx = chunk(&self.pwm_buf2, (self.flush_phase - 6) as usize);
                let _ = self.i2c.write(DRIVER_ADDR_2, &tx);
                if self.flush_phase == 8 {
                    self.flush_phase = 0;
                    return true;
                }
                self.flush_phase += 1;
            }
            _ => self.flush_phase = 0,
        }
        false
    }
}

impl<I: _embedded_hal_blocking_i2c_Write> Hal for Board<I> {
    fn now(&self) -> u32 {
        millis()
    }

    fn wait_ms(&mut self, ms: u32) {
        let start = millis();
        loop {
            if millis().wrapping_sub(start) > ms {
                break;
            }
            cortex_m::asm::wfi();
        }
    }

    fn uart_send_bytes(&mut self, buf: &[u8]) {
        let usart = unsafe { &*pac::USART1::ptr() };
        write_pin(NRF_WAKEUP, false);
        delay_us(50);
        for &b in buf {
            while usart.isr.read().txe().bit_is_clear() {}
            usart.tdr.write(|w| unsafe { w.bits(b as u32) });
        }
        // C queues the bytes and waits 50us + 24us/byte; finish the last
        // frame, then hold the pin for the trailing 50us.
        while usart.isr.read().tc().bit_is_clear() {}
        delay_us(50);
        write_pin(NRF_WAKEUP, true);
    }

    fn uart_read(&mut self) -> Option<u8> {
        rx_pop()
    }

    fn nrf_reset_pin(&mut self, high: bool) {
        write_pin(NRF_RESET, high);
    }

    fn led_power(&mut self, on: bool) {
        write_pin(DC_BOOST, on);
        write_pin(RGB_DRIVER_SDB1, on);
        write_pin(RGB_DRIVER_SDB2, on);
    }

    fn read_dev_mode_pin(&mut self) -> bool {
        read_pin(DEV_MODE)
    }

    fn read_sys_mode_pin(&mut self) -> bool {
        read_pin(SYS_MODE)
    }

    fn usb_state(&self) -> UsbDeviceState {
        with_usb(|u| u.qmk_state())
    }

    fn usb_suspended(&self) -> bool {
        with_usb(|u| u.is_suspended())
    }

    fn usb_keyboard_leds(&self) -> u8 {
        with_usb(|u| u.host_led_state())
    }

    // UsbHid drops reports unless the device is configured (USB_ACTIVE).
    fn usb_send_nkro(&mut self, mods: u8, bits: &[u8; 30]) {
        send_report(Report::nkro(mods, bits));
    }

    fn usb_send_keyboard(&mut self, report: &[u8; 8]) {
        send_report(Report::keyboard(report));
    }

    fn usb_send_mouse(&mut self, report: &[u8; 5]) {
        send_report(Report::mouse(report));
    }

    fn usb_send_consumer(&mut self, usage: u16) {
        send_report(Report::consumer(usage));
    }

    fn usb_send_system(&mut self, usage: u16) {
        send_report(Report::system(usage));
    }

    fn usb_remote_wakeup_enabled(&self) -> bool {
        with_usb(|u| u.remote_wakeup_enabled())
    }

    /// ChibiOS sleeps STM32_USB_HOST_WAKEUP_DURATION (5ms) between setting
    /// and clearing RESUME, with interrupts running.
    fn usb_wakeup_host(&mut self) {
        if with_usb(|u| u.remote_wakeup_begin()) {
            delay_us(5000);
            with_usb(|u| u.remote_wakeup_end());
        }
    }

    /// `rgb_matrix_update_pwm_buffers()`: full blocking flush of both drivers.
    #[inline(never)]
    fn rgb_update_pwm_buffers(&mut self, rgb: &mut RgbMatrix) {
        self.flush_phase = 0;
        let (b1, b2) = rgb.build_pwm_buffers();
        for (addr, buf) in [(DRIVER_ADDR_1, b1), (DRIVER_ADDR_2, b2)] {
            let _ = self.i2c.write(addr, &[0xFE, 0xC5]);
            let _ = self.i2c.write(addr, &[0xFD, 0x01]);
            let mut tx = [0u8; 193];
            tx[1..].copy_from_slice(&buf);
            let _ = self.i2c.write(addr, &tx);
        }
        rgb.dirty1 = false;
        rgb.dirty2 = false;
    }

    fn eeconfig_update(&mut self, cfg: &UserConfig) {
        self.pending_cfg = Some(*cfg);
    }

    fn eeconfig_init(&mut self) {
        self.pending_cfg = None;
        eeprom::erase();
        air96_v2::via::eeconfig_init_via();
    }

    fn eeconfig_read_keymap(&self) -> u16 {
        eeprom::keymap_config_load()
    }

    fn eeconfig_update_keymap(&mut self, raw: u16) {
        if eeprom::keymap_config_load() != raw {
            eeprom::keymap_config_save(raw);
        }
    }

    fn bootloader_jump(&mut self) {
        self.persist();
        unsafe { Matrix::enter_bootloader() }
    }

    fn mcu_reset(&mut self) {
        self.persist();
        cortex_m::peripheral::SCB::sys_reset();
    }
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
        unsafe { Matrix::jump_to_bootloader(); }
    }
}

#[entry]
fn main() -> ! {
    // ── DFU magic check — must be FIRST before any init ──────────
    check_dfu_magic_and_jump();

    let mut dp = pac::Peripherals::take().unwrap();
    let mut cp = CorePeripherals::take().unwrap();
    let mut rcc = dp.RCC.configure()
        .hsi48()
        .enable_crs(dp.CRS)
        .sysclk(48.mhz())
        .freeze(&mut dp.FLASH);

    // ── SysTick: 1ms tick ───────────────────────────────────────────
    // MUST select the core clock: the cortex-m reset default is HCLK/8.
    cp.SYST.set_clock_source(cortex_m::peripheral::syst::SystClkSource::Core);
    cp.SYST.set_reload(47999);
    cp.SYST.clear_current();
    cp.SYST.enable_counter();
    cp.SYST.enable_interrupt();

    // ── GPIO clocks ─────────────────────────────────────────────────
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let _gpioc = dp.GPIOC.split(&mut rcc);
    // GPIOD needed for matrix column D2
    let _gpiod = dp.GPIOD.split(&mut rcc);

    // ── Bootmagic: hold Escape while plugging in ─────────────────────
    // QMK resets the EEPROM (eeconfig_disable) and jumps to the bootloader.
    unsafe { Matrix::init_pins(); }
    if unsafe { Matrix::check_escape_held(|ms| cortex_m::asm::delay(ms * 48_000)) } {
        eeprom::erase();
        unsafe { Matrix::enter_bootloader(); }
    }

    // ── UART: 460800 baud, 8E1 (rf_uart_init) ───────────────────────
    let tx = cortex_m::interrupt::free(|cs| gpiob.pb6.into_alternate_af0(cs));
    let rx = cortex_m::interrupt::free(|cs| gpiob.pb7.into_alternate_af0(cs));
    let _serial = Serial::usart1(dp.USART1, (tx, rx), 460_800.bps(), &mut rcc);
    unsafe {
        let usart1 = &*pac::USART1::ptr();
        // Disable USART to modify UE-locked control bits (PCE, M0)
        usart1.cr1.modify(|_, w| w.ue().clear_bit());
        // 9-bit word (8 data + parity), even parity
        usart1.cr1.modify(|_, w| w.pce().set_bit().m0().set_bit());
        usart1.cr1.modify(|_, w| w.rxneie().set_bit().ue().set_bit());
        // Rx/Tx pin pull-ups (GPIOB PUPDR6/7 = 01), low speed
        let gb = &*pac::GPIOB::ptr();
        gb.ospeedr.modify(|r, w| w.bits(r.bits() & !(0b1111 << 12)));
        gb.pupdr.modify(|r, w| w.bits((r.bits() & !(0b1111 << 12)) | (0b0101 << 12)));
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USART1);
    }

    // ── LED driver power, then IS31FL3733 init (rgb_matrix_init) ─────
    for p in [DC_BOOST, RGB_DRIVER_SDB1, RGB_DRIVER_SDB2] {
        set_pin_output(p);
        write_pin(p, true);
    }

    // SYSCFG clock + Fm+ drive on PB8/PB9 for 1MHz I2C
    unsafe {
        let rcc_ptr = &*pac::RCC::ptr();
        rcc_ptr.apb2enr.modify(|_, w| w.syscfgen().set_bit());
    }
    dp.SYSCFG.cfgr1.modify(|_, w| w
        .i2c_pb8_fmp().set_bit()
        .i2c_pb9_fmp().set_bit()
        .i2c1_fmp().set_bit()
    );

    let scl = cortex_m::interrupt::free(|cs| gpiob.pb8.into_alternate_af1(cs));
    let sda = cortex_m::interrupt::free(|cs| gpiob.pb9.into_alternate_af1(cs));
    let mut i2c = i2c::I2c::i2c1(dp.I2C1, (scl, sda), 1000.khz(), &mut rcc);
    // 1 MHz timing for the 8 MHz I2C clock source
    unsafe {
        let i2c1 = &*pac::I2C1::ptr();
        i2c1.timingr.write(|w| w.bits(0x00000204));
    }

    // IS31FL3733 init (QMK sequence: clear LEDs → clear PWM → config → enable → delay)
    for &addr in &[DRIVER_ADDR_1, DRIVER_ADDR_2] {
        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x00]);
        for reg in 0x00u8..0x18 {
            let _ = i2c.write(addr, &[reg, 0x00]);
        }

        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x01]);
        let clear_buf = [0u8; 193];
        let _ = i2c.write(addr, &clear_buf);

        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x03]);
        let _ = i2c.write(addr, &[0x00, 0x01]); // Config: normal operation
        let _ = i2c.write(addr, &[0x01, 0xFF]); // GCC: max current
        let _ = i2c.write(addr, &[0x0F, 0x00]); // SW pull-up
        let _ = i2c.write(addr, &[0x10, 0x00]); // CS pull-down

        let _ = i2c.write(addr, &[0xFE, 0xC5]);
        let _ = i2c.write(addr, &[0xFD, 0x00]);
        for reg in 0x00u8..0x18 {
            let _ = i2c.write(addr, &[reg, 0xFF]);
        }
    }
    { let mut c = 10; while c > 0 { while !tick_arrived() { cortex_m::asm::wfi(); } c -= 1; } }

    // ── USB (protocol_pre_init) ──────────────────────────────────────
    // From here on every wait polls USB, as ChibiOS services it from its
    // interrupt, so the host can enumerate while the radio initialises.
    let (usb_pa11, usb_pa12) = cortex_m::interrupt::free(|cs| {
        (gpioa.pa11.into_floating_input(cs), gpioa.pa12.into_floating_input(cs))
    });
    let usb_periph = stm32f0xx_hal::usb::Peripheral {
        usb: dp.USB,
        pin_dm: usb_pa11,
        pin_dp: usb_pa12,
    };
    // USB stack in statics (it is 3KB; QMK's lives in ChibiOS globals).
    static mut USB_BUS: core::mem::MaybeUninit<usb_device::bus::UsbBusAllocator<stm32f0xx_hal::usb::UsbBusType>> = core::mem::MaybeUninit::uninit();
    static mut USB: core::mem::MaybeUninit<UsbHid<'static>> = core::mem::MaybeUninit::uninit();
    let usb_bus: &'static _ = unsafe { (*core::ptr::addr_of_mut!(USB_BUS)).write(stm32f0xx_hal::usb::UsbBus::new(usb_periph)) };
    let usb: &'static mut UsbHid<'static> = UsbHid::init(unsafe { &mut *core::ptr::addr_of_mut!(USB) }, usb_bus, millis);
    // Building the device asserted the D+ pull-up; service the host at once.
    cortex_m::interrupt::free(|_| unsafe { USB_PTR = usb });
    unsafe { cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USB) };
    let mut board = Board {
        i2c,
        pending_cfg: None,
        flush_phase: 0,
        flush_dirty: (false, false),
        pwm_buf1: [0; 192],
        pwm_buf2: [0; 192],
    };

    // ── m_gpio_init() ────────────────────────────────────────────────
    board.led_power(true);
    set_pin_output(NRF_WAKEUP);
    write_pin(NRF_WAKEUP, true);
    set_pin_input_high(NRF_BOOT);
    set_pin_output(NRF_RESET);
    write_pin(NRF_RESET, false);
    board.wait_ms(50);
    write_pin(NRF_RESET, true);
    set_pin_input_high(DEV_MODE);
    set_pin_input_high(SYS_MODE);

    // ── keyboard_post_init_kb(), then protocol_post_init() ───────────
    // keyboard_init(): via_init(), keymap_config (NKRO forced on), then
    // keyboard_post_init_kb()
    air96_v2::via::via_init();
    // Keyboard state lives in .bss (QMK keeps it in globals), not main's stack.
    static mut KB: Kb = Kb::new();
    static mut MATRIX: Matrix = Matrix::new();
    let kb = unsafe { &mut *core::ptr::addr_of_mut!(KB) };
    let matrix = unsafe { &mut *core::ptr::addr_of_mut!(MATRIX) };
    kb.keymap_config_init(&mut board);
    kb.keyboard_post_init_kb(&mut board, eeprom::load());
    kb.protocol_post_init();

    loop {
        // ── protocol_pre_task(): USB events ──────────────────────────
        for state in with_usb(|u| u.take_state_changes()) {
            kb.notify_usb_device_state_change_kb(state);
        }
        if let Some(p) = with_usb(|u| u.take_protocol_change()) {
            kb.usb_set_protocol(p);
        }
        let (_, just_resumed) = with_usb(|u| u.take_suspend_edge());
        if just_resumed {
            kb.suspend_wakeup_init(&mut board);
        }
        // ── keyboard_task(): matrix scan + debounce + actions ────────
        let raw = unsafe { Matrix::read_raw() };
        let events = matrix.process(raw, millis() as u16);
        if events.is_empty() {
            kb.generate_tick_event(&mut board);
        }
        for ev in &events {
            kb.matrix_event(&mut board, ev.row, ev.col, ev.pressed);
        }

        // ── rgb_matrix_task() ────────────────────────────────────────
        if kb.rgb.rgb_matrix_task(millis(), |rgb| board.flush_continue(rgb)) == TaskEvent::Indicators {
            kb.rgb_matrix_indicators_kb();
        }

        // ── mousekey_task() ──────────────────────────────────────────
        kb.mousekey_task(&mut board);

        // ── housekeeping_task() ──────────────────────────────────────
        kb.housekeeping_task_kb(&mut board, matrix.any_pressed());

        // ── protocol_post_task(): usb_idle_task() ────────────────────
        with_usb(|u| u.idle_task());

        // raw_hid_task() → raw_hid_receive() (VIA)
        if let Some(mut cmd) = with_usb(|u| u.take_via_command()) {
            let mut ctx = air96_v2::via::ViaContext { rgb: &mut kb.rgb, now: millis(), matrix: matrix.matrix, rgb_save: false };
            air96_v2::via::raw_hid_receive(&mut cmd, &mut ctx);
            let save = ctx.rgb_save;
            send_report(Report::raw(&cmd));
            if save {
                kb.eeconfig_update_user_datablock(&mut board);
            }
        }


        // ── Deferred eeconfig write (idle passes only) ───────────────
        if events.is_empty() {
            if let Some(cfg) = board.pending_cfg.take() {
                eeprom::save(&cfg);
            }
            eeprom::task(millis());
        }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::wfe(); }
}
