// Air96 V2 hardware configuration
// STM32F072CB — 128KB flash, 16KB RAM

pub const MATRIX_ROWS: usize = 6;
pub const MATRIX_COLS: usize = 21;

pub const RGB_LED_COUNT: usize = 110;  // DRIVER_1: 58 + DRIVER_2: 42+10

// UART to NRF module
pub const UART_BAUD: u32 = 460_800;

// I2C for IS31FL3733 LED drivers
pub const I2C_FREQ: u32 = 1_000_000;  // 1MHz Fast-mode Plus

// Timing (milliseconds)
pub const DEBOUNCE_MS: u32 = 1;
pub const LONG_PRESS_MS: u32 = 3000;
pub const SLEEP_TIMEOUT_MS: u32 = 360_000;  // 6 minutes
pub const LINK_TIMEOUT_MS: u32 = 120_000;    // 2 minutes
pub const STARTUP_DELAY_MS: u32 = 100;
