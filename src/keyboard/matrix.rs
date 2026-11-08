//! Matrix scanner — PAC register-level access (matches C firmware approach)
//!
//! 6 rows × 21 cols, COL2ROW diodes on STM32F072.
//! Pins from keyboard.json:
//!   rows:  [C14, C15, A0, A1, A2, A3]
//!   cols:  [A4, A5, A6, A7, B0, B1, B10, B11, B12, B13, B14, B15,
//!           A8, A9, A10, A15, B3, C10, C11, C12, D2]

#![allow(clippy::missing_safety_doc)]


use stm32f0xx_hal::pac;

/// Magic value written to RTC backup register for DFU entry
#[allow(dead_code)]
const DFU_MAGIC: u32 = 0xDF0DF0DF;

/// Port + pin number pair for raw register access
#[derive(Clone, Copy)]
struct GpioPin {
    port: *const pac::gpioa::RegisterBlock, // pointer to GPIOA-style register block
    pin: u8,
}

/// Pre-computed port base addresses (STM32F0)
const GPIOA_BASE: *const pac::gpioa::RegisterBlock = 0x4800_0000 as *const _;
const GPIOB_BASE: *const pac::gpioa::RegisterBlock = 0x4800_0400 as *const _;
const GPIOC_BASE: *const pac::gpioa::RegisterBlock = 0x4800_0800 as *const _;
const GPIOD_BASE: *const pac::gpioa::RegisterBlock = 0x4800_0C00 as *const _;

// Register offsets (same layout across GPIOA..GPIOD)
const MODER_OFFSET: usize = 0x00;
const IDR_OFFSET: usize = 0x10;

/// Row pins (6) — driven low one at a time during scan
const ROW_PINS: [GpioPin; 6] = [
    GpioPin { port: GPIOC_BASE, pin: 14 }, // C14
    GpioPin { port: GPIOC_BASE, pin: 15 }, // C15
    GpioPin { port: GPIOA_BASE, pin: 0  }, // A0
    GpioPin { port: GPIOA_BASE, pin: 1  }, // A1
    GpioPin { port: GPIOA_BASE, pin: 2  }, // A2
    GpioPin { port: GPIOA_BASE, pin: 3  }, // A3
];

/// Column pins (21) — read during scan (low = key pressed in COL2ROW)
const COL_PINS: [GpioPin; 21] = [
    GpioPin { port: GPIOA_BASE, pin: 4  }, // A4
    GpioPin { port: GPIOA_BASE, pin: 5  }, // A5
    GpioPin { port: GPIOA_BASE, pin: 6  }, // A6
    GpioPin { port: GPIOA_BASE, pin: 7  }, // A7
    GpioPin { port: GPIOB_BASE, pin: 0  }, // B0
    GpioPin { port: GPIOB_BASE, pin: 1  }, // B1
    GpioPin { port: GPIOB_BASE, pin: 10 }, // B10
    GpioPin { port: GPIOB_BASE, pin: 11 }, // B11
    GpioPin { port: GPIOB_BASE, pin: 12 }, // B12
    GpioPin { port: GPIOB_BASE, pin: 13 }, // B13
    GpioPin { port: GPIOB_BASE, pin: 14 }, // B14
    GpioPin { port: GPIOB_BASE, pin: 15 }, // B15
    GpioPin { port: GPIOA_BASE, pin: 8  }, // A8
    GpioPin { port: GPIOA_BASE, pin: 9  }, // A9
    GpioPin { port: GPIOA_BASE, pin: 10 }, // A10
    GpioPin { port: GPIOA_BASE, pin: 15 }, // A15
    GpioPin { port: GPIOB_BASE, pin: 3  }, // B3
    GpioPin { port: GPIOC_BASE, pin: 10 }, // C10
    GpioPin { port: GPIOC_BASE, pin: 11 }, // C11
    GpioPin { port: GPIOC_BASE, pin: 12 }, // C12
    GpioPin { port: GPIOD_BASE, pin: 2  }, // D2
];

/// Read a pin from its port's IDR register
#[inline(always)]
unsafe fn read_pin(p: &GpioPin) -> bool {
    let idr = (p.port as *const u32).add(IDR_OFFSET / 4).read_volatile();
    (idr >> p.pin) & 1 != 0
}

/// Set MODER bits for a pin (00 = input, 01 = output)
#[inline(always)]
unsafe fn set_pin_mode(p: &GpioPin, mode: u32) {
    let moder = (p.port as *mut u32).add(MODER_OFFSET / 4);
    let val = moder.read_volatile();
    let mask = 0b11u32 << (p.pin * 2);
    moder.write_volatile((val & !mask) | (mode << (p.pin * 2)));
}

/// Set a pin low — BRR (bit reset register) at offset 0x28
#[inline(always)]
unsafe fn set_pin_low(p: &GpioPin) {
    let brr = (p.port as *mut u32).add(0x28 / 4);
    brr.write_volatile(1u32 << p.pin);
}

/// `setPinInputHigh()`: input with pull-up (PUPDR at 0x0C, 01 = pull-up).
#[inline(always)]
unsafe fn set_pin_input_high(p: &GpioPin) {
    set_pin_mode(p, 0b00);
    let pupdr = (p.port as *mut u32).add(0x0C / 4);
    let val = pupdr.read_volatile();
    let mask = 0b11u32 << (p.pin * 2);
    pupdr.write_volatile((val & !mask) | (0b01u32 << (p.pin * 2)));
}

/// QMK `select_row()`: drive the row low.
#[inline(always)]
unsafe fn select_row(p: &GpioPin) {
    set_pin_low(p);
    set_pin_mode(p, 0b01);
}

/// QMK `unselect_row()`: release the row to input pull-up.
#[inline(always)]
unsafe fn unselect_row(p: &GpioPin) {
    set_pin_input_high(p);
}

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub row: u8,
    pub col: u8,
    pub pressed: bool,
}

pub const MATRIX_ROWS: usize = 6;
pub const MATRIX_COLS: usize = 21;

/// `DEBOUNCE` from keyboards/air96_v2/ansi/keyboard.json.
pub const DEBOUNCE: u8 = 10;
const DEBOUNCE_ELAPSED: u8 = 0;

/// `debounce_counter_t`.
#[derive(Clone, Copy, Default)]
struct Counter {
    pressed: bool,
    time: u8,
}

/// Port of `quantum/debounce/asym_eager_defer_pk.c` (the C build's
/// `DEBOUNCE_TYPE`): a press reports on the first scan that sees it, then the
/// key ignores input for `DEBOUNCE` ms; a release reports only after the key
/// has read released for `DEBOUNCE` ms without change.
pub struct Debouncer {
    counters: [[Counter; MATRIX_COLS]; MATRIX_ROWS],
    last_time: u16,
    counters_need_update: bool,
    matrix_need_update: bool,
    cooked_changed: bool,
}

impl Default for Debouncer {
    fn default() -> Self {
        Self::new()
    }
}

impl Debouncer {
    pub const fn new() -> Self {
        Self {
            counters: [[Counter { pressed: false, time: 0 }; MATRIX_COLS]; MATRIX_ROWS],
            last_time: 0,
            counters_need_update: false,
            matrix_need_update: false,
            cooked_changed: false,
        }
    }

    /// `debounce()`. `now` is `timer_read_fast()` (ms).
    pub fn debounce(&mut self, raw: &[u32; MATRIX_ROWS], cooked: &mut [u32; MATRIX_ROWS], changed: bool, now: u16) -> bool {
        let mut updated_last = false;
        self.cooked_changed = false;

        if self.counters_need_update {
            let elapsed = now.wrapping_sub(self.last_time).min(u8::MAX as u16) as u8;
            self.last_time = now;
            updated_last = true;
            if elapsed > 0 {
                self.update_debounce_counters_and_transfer_if_expired(raw, cooked, elapsed);
            }
        }

        if changed || self.matrix_need_update {
            if !updated_last {
                self.last_time = now;
            }
            self.transfer_matrix_values(raw, cooked);
        }

        self.cooked_changed
    }

    fn update_debounce_counters_and_transfer_if_expired(&mut self, raw: &[u32; MATRIX_ROWS], cooked: &mut [u32; MATRIX_ROWS], elapsed: u8) {
        self.counters_need_update = false;
        self.matrix_need_update = false;

        for row in 0..MATRIX_ROWS {
            for col in 0..MATRIX_COLS {
                let mask = 1u32 << col;
                let c = &mut self.counters[row][col];
                if c.time != DEBOUNCE_ELAPSED {
                    if c.time <= elapsed {
                        c.time = DEBOUNCE_ELAPSED;
                        if c.pressed {
                            // key-down: eager
                            self.matrix_need_update = true;
                        } else {
                            // key-up: defer
                            let next = (cooked[row] & !mask) | (raw[row] & mask);
                            self.cooked_changed |= next != cooked[row];
                            cooked[row] = next;
                        }
                    } else {
                        c.time -= elapsed;
                        self.counters_need_update = true;
                    }
                }
            }
        }
    }

    fn transfer_matrix_values(&mut self, raw: &[u32; MATRIX_ROWS], cooked: &mut [u32; MATRIX_ROWS]) {
        self.matrix_need_update = false;

        for row in 0..MATRIX_ROWS {
            let delta = raw[row] ^ cooked[row];
            for col in 0..MATRIX_COLS {
                let mask = 1u32 << col;
                let c = &mut self.counters[row][col];
                if delta & mask != 0 {
                    if c.time == DEBOUNCE_ELAPSED {
                        c.pressed = raw[row] & mask != 0;
                        c.time = DEBOUNCE;
                        self.counters_need_update = true;

                        if c.pressed {
                            // key-down: eager
                            cooked[row] ^= mask;
                            self.cooked_changed = true;
                        }
                    }
                } else if c.time != DEBOUNCE_ELAPSED && !c.pressed {
                    // key-up: defer
                    c.time = DEBOUNCE_ELAPSED;
                }
            }
        }
    }
}

/// QMK `matrix_scan()` + `keyboard_task()` change detection: raw matrix,
/// debounced matrix and the previously reported matrix.
pub struct Matrix {
    pub raw: [u32; MATRIX_ROWS],
    pub matrix: [u32; MATRIX_ROWS],
    previous: [u32; MATRIX_ROWS],
    pub debouncer: Debouncer,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::new()
    }
}

impl Matrix {
    pub const fn new() -> Self {
        Self { raw: [0; MATRIX_ROWS], matrix: [0; MATRIX_ROWS], previous: [0; MATRIX_ROWS], debouncer: Debouncer::new() }
    }

    /// Initialize all row/col pins (`matrix_init_pins()`): rows unselected
    /// (input pull-up), columns input pull-up.
    pub unsafe fn init_pins() {
        for row in &ROW_PINS {
            unselect_row(row);
        }
        for col in &COL_PINS {
            set_pin_input_high(col);
        }
    }

    /// One `matrix_scan()` pass with the given raw rows, then the
    /// `keyboard_task()` diff against the previous debounced matrix.
    /// Events are in row-major, column-ascending order.
    #[inline(never)]
    pub fn process(&mut self, curr: [u32; MATRIX_ROWS], now: u16) -> heapless::Vec<KeyEvent, 32> {
        let changed = curr != self.raw;
        if changed {
            self.raw = curr;
        }
        let raw = self.raw;
        self.debouncer.debounce(&raw, &mut self.matrix, changed, now);

        let mut events = heapless::Vec::new();
        for row in 0..MATRIX_ROWS {
            let diff = self.matrix[row] ^ self.previous[row];
            if diff == 0 {
                continue;
            }
            for col in 0..MATRIX_COLS {
                let mask = 1u32 << col;
                if diff & mask != 0 && events.push(KeyEvent { row: row as u8, col: col as u8, pressed: self.matrix[row] & mask != 0 }).is_ok() {
                    self.previous[row] ^= mask;
                }
            }
        }
        events
    }

    /// True if any debounced key is down (`matrix_get_row(row) != 0`).
    pub fn any_pressed(&self) -> bool {
        self.matrix.iter().any(|&r| r != 0)
    }

    /// `matrix_read_cols_on_row()` for every row (COL2ROW).
    #[inline(never)]
    pub unsafe fn read_raw() -> [u32; MATRIX_ROWS] {
        let mut rows = [0u32; MATRIX_ROWS];
        for (row_idx, row_pin) in ROW_PINS.iter().enumerate() {
            select_row(row_pin);
            // matrix_output_select_delay(): waitInputPinDelay()
            cortex_m::asm::delay(12);
            let mut bits = 0u32;
            for (col_idx, col_pin) in COL_PINS.iter().enumerate() {
                if !read_pin(col_pin) {
                    bits |= 1 << col_idx;
                }
            }
            rows[row_idx] = bits;
            unselect_row(row_pin);
            // matrix_output_unselect_delay(): MATRIX_IO_DELAY (30us)
            cortex_m::asm::delay(48 * 30);
        }
        rows
    }

    /// Bootmagic (`bootmagic_scan()`): Escape held while plugging in.
    /// Scan, wait `BOOTMAGIC_DEBOUNCE` (2 × DEBOUNCE), scan again. With the
    /// eager-press debounce the key counts if either scan saw it down.
    pub unsafe fn check_escape_held(mut wait_ms: impl FnMut(u32)) -> bool {
        let first = Self::read_raw()[0] & 1 != 0;
        wait_ms(2 * DEBOUNCE as u32);
        let second = Self::read_raw()[0] & 1 != 0;
        first || second
    }

    // ─────────────────────────────────────────────────────────────────
    // DO NOT MODIFY — DFU bootloader entry (STM32F072 Cortex-M0+).
    // This sequence is hardware-sensitive: NVIC clear, SYSCFG remap,
    // VTOR jump, interrupt re-enable. Any change can soft-brick the
    // keyboard requiring BOOT0 pad shorting.
    // ─────────────────────────────────────────────────────────────────
    /// Write DFU magic to RTC backup register then trigger system reset.
    /// On next boot, check_dfu_magic_and_jump() detects the magic and
    /// performs a clean jump to the ROM bootloader.
    /// This is the QMK stm32_dfu pattern — survives system reset.
    #[cfg(all(target_arch = "arm", not(test)))]
    pub unsafe fn enter_bootloader() -> ! {
        cortex_m::interrupt::disable();

        // Enable PWR clock (RCC_APB1ENR bit 28) and backup domain (PWR_CR.DBP = bit 8)
        const RCC_APB1ENR: *mut u32 = 0x4002_101C as *mut u32;
        RCC_APB1ENR.write_volatile(RCC_APB1ENR.read_volatile() | (1 << 28));

        const PWR_CR: *mut u32 = 0x4000_7000 as *mut u32;
        PWR_CR.write_volatile(PWR_CR.read_volatile() | (1 << 8));

        // Write magic value to RTC backup register 0 (offset 0x50 from RTC base 0x40002800)
        const RTC_BKP0R: *mut u32 = 0x4000_2850 as *mut u32;
        RTC_BKP0R.write_volatile(DFU_MAGIC);

        // System reset — RAM and RTC backup registers survive
        cortex_m::peripheral::SCB::sys_reset();
    }

    #[cfg(not(all(target_arch = "arm", not(test))))]
    pub unsafe fn enter_bootloader() -> ! {
        panic!("enter_bootloader is only supported on ARM target");
    }

    // ─────────────────────────────────────────────────────────────────
    // DO NOT MODIFY — Bootloader jump (STM32F072 Cortex-M0+).
    // NVIC, SysTick, GPIO reset, VTOR remap, raw asm MSP+bx.
    // Same sensitivity as enter_bootloader above.
    // ─────────────────────────────────────────────────────────────────
    /// Full-cleanup jump to STM32F072 ROM bootloader (0483:DF11).
    /// Disables all NVIC interrupts, clears pending flags, resets SysTick,
    /// points VTOR to system memory (0x1FFF_C800), sets MSP and jumps.
    /// Must only be called from check_dfu_magic_and_jump() at boot.
    #[allow(asm_sub_register, clippy::zero_ptr, clippy::manual_dangling_ptr)]
    #[cfg(all(target_arch = "arm", not(test)))]
    pub unsafe fn jump_to_bootloader() -> ! {
        // Disable interrupts globally
        cortex_m::interrupt::disable();

        // Disable SysTick
        (0xE000_E010 as *mut u32).write_volatile(0);

        // Reset GPIOA so USB pins (PA11/PA12) are in default state
        const RCC_AHBRSTR: *mut u32 = 0x4002_1028 as *mut u32;
        RCC_AHBRSTR.write_volatile(1 << 17);  // GPIOARST
        RCC_AHBRSTR.write_volatile(0);

        // Clear ALL NVIC interrupt enables and pending flags (QMK pattern)
        // NVIC_ICER: 0xE000E180-0xE000E19C (8 registers for up to 240 IRQs)
        // NVIC_ICPR: 0xE000E280-0xE000E29C
        for i in 0..8u32 {
            (0xE000_E180 as *mut u32).add(i as usize).write_volatile(0xFFFF_FFFF);
            (0xE000_E280 as *mut u32).add(i as usize).write_volatile(0xFFFF_FFFF);
        }

        // Set CONTROL to 0 (privileged thread mode, MSP)
        core::arch::asm!("msr CONTROL, {r}", r = in(reg) 0u32);

        // Enable SYSCFG clock (RCC_APB2ENR bit 0)
        const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;
        RCC_APB2ENR.write_volatile(RCC_APB2ENR.read_volatile() | (1 << 0));

        // Remap System Memory (bootloader) to 0x0000_0000 (SYSCFG_CFGR1 MEM_MODE = 0b01)
        const SYSCFG_CFGR1: *mut u32 = 0x4001_0000 as *mut u32;
        SYSCFG_CFGR1.write_volatile((SYSCFG_CFGR1.read_volatile() & !0b11) | 0b01);

        cortex_m::asm::dsb();
        cortex_m::asm::isb();

        // Load bootloader's initial SP and reset vector (from remapped 0x0000_0000)
        let sp = core::ptr::read_volatile(0x0000_0000 as *const u32);
        let rv = core::ptr::read_volatile(0x0000_0004 as *const u32);

        // Re-enable interrupts globally (cpsie i) so that the bootloader can handle USB events,
        // set MSP, and jump to the reset handler.
        core::arch::asm!(
            "cpsie i",
            "msr MSP, {sp}",
            "bx {rv}",
            sp = in(reg) sp,
            rv = in(reg) rv,
            options(noreturn)
        );
    }

    #[cfg(not(all(target_arch = "arm", not(test))))]
    pub unsafe fn jump_to_bootloader() -> ! {
        panic!("jump_to_bootloader is only supported on ARM target");
    }
}
