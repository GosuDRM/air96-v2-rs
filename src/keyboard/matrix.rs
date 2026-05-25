//! Matrix scanner — PAC register-level access (matches C firmware approach)
//!
//! 6 rows × 21 cols, COL2ROW diodes on STM32F072.
//! Pins from keyboard.json:
//!   rows:  [C14, C15, A0, A1, A2, A3]
//!   cols:  [A4, A5, A6, A7, B0, B1, B10, B11, B12, B13, B14, B15,
//!           A8, A9, A10, A15, B3, C10, C11, C12, D2]
//!
//! Uses raw GPIO register reads for speed — same approach as C firmware.

use stm32f0xx_hal::pac;

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
    debounced: [u8; 16],
    // Per-key debounce counters
    counters: [[u8; 21]; 6],
}

impl Matrix {
    pub fn new() -> Self {
        Self {
            debounced: [0; 16],
            counters: [[0; 21]; 6],
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
    /// Call every 1ms from main loop.
    pub fn scan(&mut self) -> heapless::Vec<KeyEvent, 32> {
        let mut events: heapless::Vec<KeyEvent, 32> = heapless::Vec::new();

        for (row_idx, row_pin) in ROW_PINS.iter().enumerate() {
            unsafe {
                // Drive row low
                set_pin_low(row_pin);
                // ~30µs settling delay (matches QMK matrix_io_delay default)
                for _ in 0..150u32 {
                    core::ptr::read_volatile(0xE000_E010 as *const u32);
                }
            }

            for (col_idx, col_pin) in COL_PINS.iter().enumerate() {
                let pressed = unsafe { !read_pin(col_pin) }; // COL2ROW: low = pressed

                // Bit position in debounced array
                let bit_pos = row_idx * 21 + col_idx;
                let byte_idx = bit_pos / 8;
                let bit_mask = 1u8 << (bit_pos % 8);

                let debounced_bit = (self.debounced[byte_idx] & bit_mask) != 0;

                if debounced_bit != pressed {
                    let ctr = &mut self.counters[row_idx][col_idx];
                    *ctr += 1;
                    if *ctr >= 5 {
                        // 5ms debounce
                        *ctr = 0;
                        if pressed {
                            self.debounced[byte_idx] |= bit_mask;
                        } else {
                            self.debounced[byte_idx] &= !bit_mask;
                        }
                        let _ = events.push(KeyEvent {
                            row: row_idx as u8,
                            col: col_idx as u8,
                            pressed,
                        });
                    }
                } else {
                    self.counters[row_idx][col_idx] = 0;
                }
            }

            unsafe {
                // Release row (drive high)
                set_pin_high(row_pin);
            }
        }

        events
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

    /// Jump to STM32F072 ROM bootloader (USB DFU, 0483:DF11).
    /// Canonical approach: disable interrupts, set VTOR to system memory
    /// (0x1FFF_C800), load bootloader's stack pointer and entry address,
    /// reset peripherals to clean state, then jump.
    pub unsafe fn enter_bootloader() -> ! {
        cortex_m::interrupt::disable();

        // Disable SysTick (set all bits to 0 in CSR)
        (0xE000_E010 as *mut u32).write_volatile(0);

        // Reset GPIOA so USB pins (PA11/PA12) are in their default state
        // AHBRSTR: write 1 to GPIOARST bit (17), hardware auto-clears
        const RCC_AHBRSTR: *mut u32 = 0x4002_1028 as *mut u32;
        RCC_AHBRSTR.write_volatile(1 << 17);  // set GPIOARST
        RCC_AHBRSTR.write_volatile(0);         // clear (hardware may have already)

        // Point VTOR to bootloader's vector table (critical — bootloader uses USB IRQs)
        let cp = cortex_m::Peripherals::steal();
        cp.SCB.vtor.write(0x1FFF_C800);

        // Load bootloader's initial stack pointer (first word of vector table)
        let sp = core::ptr::read_volatile(0x1FFF_C800 as *const u32);
        // Load bootloader's reset vector (second word of vector table)
        let rv = core::ptr::read_volatile(0x1FFF_C804 as *const u32);

        // Set MSP to bootloader's stack, then jump to bootloader entry
        core::arch::asm!(
            "msr MSP, {sp}",
            "bx {rv}",
            sp = in(reg) sp,
            rv = in(reg) rv,
            options(noreturn)
        );
    }
}
