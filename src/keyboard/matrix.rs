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

/// Set a pin low (to scan that row) — writes ODR at offset 0x14
#[inline(always)]
unsafe fn set_pin_low(p: &GpioPin) {
    // BRR = bit reset register at offset 0x28
    let brr = (p.port as *mut u32).add(0x28 / 4);
    brr.write_volatile(1u32 << p.pin);
}

/// Set a pin high — writes BSRR at offset 0x18
#[inline(always)]
unsafe fn set_pin_high(p: &GpioPin) {
    let bsrr = (p.port as *mut u32).add(0x18 / 4);
    bsrr.write_volatile(1u32 << p.pin);
}

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub row: u8,
    pub col: u8,
    pub pressed: bool,
}

pub struct Matrix {
    // Debounce state: 6 rows × 21 cols = 126 bits → 16 bytes
    pub(crate) debounced: [u8; 16],
    // Per-key debounce counters (remaining ms of stability required)
    pub(crate) counters: [[u8; 21]; 6],
    // Pending events generated in tick_debounce() upon timer expiry
    pub(crate) pending_events: heapless::Vec<KeyEvent, 32>,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::new()
    }
}

impl Matrix {
    pub fn new() -> Self {
        Self {
            debounced: [0; 16],
            counters: [[0; 21]; 6],
            pending_events: heapless::Vec::new(),
        }
    }

    /// Initialize all row/col pins via HAL (called once at startup).
    /// This sets the correct modes before we switch to raw register access.
    pub unsafe fn init_pins() {
        // Set row pins as outputs (push-pull, initially high)
        for row in &ROW_PINS {
            set_pin_mode(row, 0b01); // output
            set_pin_high(row);
        }
        // Set col pins as inputs with pull-up
        for col in &COL_PINS {
            set_pin_mode(col, 0b00); // input
            // Pull-up is set via PUPDR register (offset 0x0C, 01 = pull-up)
            let pupdr = (col.port as *mut u32).add(0x0C / 4);
            let val = pupdr.read_volatile();
            let mask = 0b11u32 << (col.pin * 2);
            pupdr.write_volatile((val & !mask) | (0b01u32 << (col.pin * 2)));
        }
    }

    /// Full matrix scan — returns list of changed keys.
    /// Call continuously from main loop.
    /// DO NOT MODIFY — sym_eager_pk debounce (instant trigger + tick lockout).
    /// Do NOT add counter reset (causes missed keystrokes).
    /// Do NOT switch to deferred/pending_events (breaks USB enumeration).
    pub fn scan(&mut self) -> heapless::Vec<KeyEvent, 32> {
        let mut events: heapless::Vec<KeyEvent, 32> = heapless::Vec::new();

        for (row_idx, _row_pin) in ROW_PINS.iter().enumerate() {
            unsafe {
                // Drive row low
                set_pin_low(_row_pin);
                // ~30µs settling delay (matches QMK matrix_io_delay default)
                for _ in 0..500u32 {
                    core::ptr::read_volatile(0xE000_E010 as *const u32);
                }
            }

            // Batch read IDR registers for GPIOA, GPIOB, GPIOC, and GPIOD once per row
            let (idr_a, idr_b, idr_c, idr_d) = unsafe {
                (
                    (GPIOA_BASE as *const u32).add(IDR_OFFSET / 4).read_volatile(),
                    (GPIOB_BASE as *const u32).add(IDR_OFFSET / 4).read_volatile(),
                    (GPIOC_BASE as *const u32).add(IDR_OFFSET / 4).read_volatile(),
                    (GPIOD_BASE as *const u32).add(IDR_OFFSET / 4).read_volatile(),
                )
            };

            for (col_idx, col_pin) in COL_PINS.iter().enumerate() {
                // Select pre-read IDR based on port and check if pin is low (pressed)
                let idr = match col_pin.port {
                    p if p == GPIOA_BASE => idr_a,
                    p if p == GPIOB_BASE => idr_b,
                    p if p == GPIOC_BASE => idr_c,
                    _ => idr_d,
                };
                let pressed = (idr & (1 << col_pin.pin)) == 0;

                // Bit position in debounced array
                let bit_pos = row_idx * 21 + col_idx;
                let byte_idx = bit_pos / 8;
                let bit_mask = 1u8 << (bit_pos % 8);

                let debounced_bit = (self.debounced[byte_idx] & bit_mask) != 0;
                let ctr = &mut self.counters[row_idx][col_idx];

                if pressed == debounced_bit {
                    // Pin matches debounced state: do nothing.
                    // Do NOT reset counter — let tick_debounce() handle it.
                    // Resetting here at scan rate (~500µs) would prevent
                    // the counter from ever reaching 0.
                } else {
                    // Pin differs from debounced state: start timer if idle
                    if *ctr == 0 {
                        *ctr = 5; // Require 5ms of continuous difference
                    }
                }
            }

            unsafe {
                // Release row (drive high)
                set_pin_high(_row_pin);
            }
        }

        // Drain any pending events generated by tick_debounce
        let mut events = heapless::Vec::new();
        while let Some(ev) = self.pending_events.pop() {
            let _ = events.push(ev);
        }
        events
    }

    /// Decrement per-key stability debounce counters.
    /// Call exactly once every 1ms from SysTick loop.
    pub fn tick_debounce(&mut self) {
        for row_idx in 0..6 {
            for col_idx in 0..21 {
                let ctr = &mut self.counters[row_idx][col_idx];
                if *ctr > 0 {
                    *ctr -= 1;
                    if *ctr == 0 {
                        // Switch has been stable and different for 5ms: commit the state change
                        let bit_pos = row_idx * 21 + col_idx;
                        let byte_idx = bit_pos / 8;
                        let bit_mask = 1u8 << (bit_pos % 8);

                        let old_debounced = (self.debounced[byte_idx] & bit_mask) != 0;
                        let new_debounced = !old_debounced;

                        if new_debounced {
                            self.debounced[byte_idx] |= bit_mask;
                        } else {
                            self.debounced[byte_idx] &= !bit_mask;
                        }

                        let _ = self.pending_events.push(KeyEvent {
                            row: row_idx as u8,
                            col: col_idx as u8,
                            pressed: new_debounced,
                        });
                    }
                }
            }
        }
    }

    /// Check if Escape key (row 0, col 0) is held — for DFU entry.
    /// Call after `init_pins()`. Samples 3 times with 30µs settling to
    /// ride out power-on transients from USB hot-plug.
    pub unsafe fn check_escape_held() -> bool {
        let row = &ROW_PINS[0];
        let col = &COL_PINS[0];
        for _ in 0..3u8 {
            set_pin_low(row);
            for _ in 0..150u32 {
                core::ptr::read_volatile(0xE000_E010 as *const u32);
            }
            if read_pin(col) {
                set_pin_high(row);
                return false;
            }
            set_pin_high(row);
            for _ in 0..50u32 {
                core::ptr::read_volatile(0xE000_E010 as *const u32);
            }
        }
        true
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
        loop {}
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
        loop {}
    }
}
