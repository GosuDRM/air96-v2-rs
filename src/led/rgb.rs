//! IS31FL3733 RGB matrix driver — port of QMK is31fl3733 driver
//!
//! Two IS31FL3733 chips on I2C bus 1 (1MHz Fast-mode Plus):
//!   DRIVER_1: 0x50 (AD connected to GND)  — 58 LEDs (indices 0-57)
//!   DRIVER_2: 0x53 (AD connected to VCC) — 52 LEDs (indices 58-109)
//!
//! Each LED uses 3 channels (R, G, B) on one of 12 CS lines.
//! PWM register map from is31fl3733.h: A_1=0x00..A_16=0x0F, B_1=0x10..B_16=0x1F, etc.

pub const DRIVER_ADDR_1: u8 = 0x50; // 0b1010000 — AD → GND
pub const DRIVER_ADDR_2: u8 = 0x53; // 0b1010011 — AD → VCC
pub const LED_COUNT: usize = 110;

// IS31FL3733 registers (function page)
const REG_SHUTDOWN: u8 = 0x0A;
const REG_CONFIG: u8 = 0x00;

// Configuration
const CONFIG_NORMAL: u8 = 0x01;

use heapless::Vec;

/// Single LED channel mapping (port of is31_led struct from keymap.c)
#[derive(Debug, Clone, Copy)]
pub struct LedMapping {
    pub driver: u8,  // 0 = DRIVER_1, 1 = DRIVER_2
    pub r: u8,       // PWM register offset for red
    pub g: u8,       // PWM register offset for green
    pub b: u8,       // PWM register offset for blue
}

/// Full 110-LED mapping — generated from keymap.c g_is31_leds array.
/// Driver 0 = 0x50 (GND), Driver 1 = 0x53 (VCC).
/// Indices 0-99: main keyboard LEDs, 100-104: side left, 105-109: side right.
pub const LED_MAP: [LedMapping; LED_COUNT] = [
    // ── Indices 0–9 ─────────────────────────────────────────────────
    LedMapping { driver: 0, r: 0x0F, g: 0x1F, b: 0x2F },
    LedMapping { driver: 0, r: 0x0E, g: 0x1E, b: 0x2E },
    LedMapping { driver: 0, r: 0x0D, g: 0x1D, b: 0x2D },
    LedMapping { driver: 0, r: 0x0C, g: 0x1C, b: 0x2C },
    LedMapping { driver: 0, r: 0x0B, g: 0x1B, b: 0x2B },
    LedMapping { driver: 0, r: 0x3F, g: 0x4F, b: 0x5F },
    LedMapping { driver: 0, r: 0x3E, g: 0x4E, b: 0x5E },
    LedMapping { driver: 0, r: 0x3D, g: 0x4D, b: 0x5D },
    LedMapping { driver: 0, r: 0x3C, g: 0x4C, b: 0x5C },
    LedMapping { driver: 0, r: 0x3B, g: 0x4B, b: 0x5B },
    // ── Indices 10–19 ───────────────────────────────────────────────
    LedMapping { driver: 1, r: 0x3F, g: 0x4F, b: 0x5F },
    LedMapping { driver: 1, r: 0x3E, g: 0x4E, b: 0x5E },
    LedMapping { driver: 1, r: 0x3D, g: 0x4D, b: 0x5D },
    LedMapping { driver: 1, r: 0x3C, g: 0x4C, b: 0x5C },
    LedMapping { driver: 1, r: 0x63, g: 0x73, b: 0x83 },
    LedMapping { driver: 1, r: 0x34, g: 0x44, b: 0x54 },
    LedMapping { driver: 1, r: 0x64, g: 0x74, b: 0x84 },
    LedMapping { driver: 1, r: 0x35, g: 0x45, b: 0x55 },
    LedMapping { driver: 1, r: 0x65, g: 0x75, b: 0x85 },
    LedMapping { driver: 0, r: 0x00, g: 0x10, b: 0x20 },
    // ── Indices 20–29 ───────────────────────────────────────────────
    LedMapping { driver: 0, r: 0x01, g: 0x11, b: 0x21 },
    LedMapping { driver: 0, r: 0x02, g: 0x12, b: 0x22 },
    LedMapping { driver: 0, r: 0x03, g: 0x13, b: 0x23 },
    LedMapping { driver: 0, r: 0x04, g: 0x14, b: 0x24 },
    LedMapping { driver: 0, r: 0x05, g: 0x15, b: 0x25 },
    LedMapping { driver: 0, r: 0x06, g: 0x16, b: 0x26 },
    LedMapping { driver: 0, r: 0x07, g: 0x17, b: 0x27 },
    LedMapping { driver: 0, r: 0x08, g: 0x18, b: 0x28 },
    LedMapping { driver: 0, r: 0x09, g: 0x19, b: 0x29 },
    LedMapping { driver: 0, r: 0x0A, g: 0x1A, b: 0x2A },
    // ── Indices 30–37 ───────────────────────────────────────────────
    LedMapping { driver: 1, r: 0x30, g: 0x40, b: 0x50 },
    LedMapping { driver: 1, r: 0x31, g: 0x41, b: 0x51 },
    LedMapping { driver: 1, r: 0x32, g: 0x42, b: 0x52 },
    LedMapping { driver: 1, r: 0x36, g: 0x46, b: 0x56 },
    LedMapping { driver: 1, r: 0x37, g: 0x47, b: 0x57 },
    LedMapping { driver: 1, r: 0x38, g: 0x48, b: 0x58 },
    LedMapping { driver: 1, r: 0x39, g: 0x49, b: 0x59 },
    LedMapping { driver: 0, r: 0x30, g: 0x40, b: 0x50 },
    // ── Indices 38–47 ───────────────────────────────────────────────
    LedMapping { driver: 0, r: 0x31, g: 0x41, b: 0x51 },
    LedMapping { driver: 0, r: 0x32, g: 0x42, b: 0x52 },
    LedMapping { driver: 0, r: 0x33, g: 0x43, b: 0x53 },
    LedMapping { driver: 0, r: 0x34, g: 0x44, b: 0x54 },
    LedMapping { driver: 0, r: 0x35, g: 0x45, b: 0x55 },
    LedMapping { driver: 0, r: 0x36, g: 0x46, b: 0x56 },
    LedMapping { driver: 0, r: 0x37, g: 0x47, b: 0x57 },
    LedMapping { driver: 0, r: 0x38, g: 0x48, b: 0x58 },
    LedMapping { driver: 0, r: 0x39, g: 0x49, b: 0x59 },
    LedMapping { driver: 0, r: 0x3A, g: 0x4A, b: 0x5A },
    // ── Indices 48–55 ───────────────────────────────────────────────
    LedMapping { driver: 1, r: 0x60, g: 0x70, b: 0x80 },
    LedMapping { driver: 1, r: 0x61, g: 0x71, b: 0x81 },
    LedMapping { driver: 1, r: 0x62, g: 0x72, b: 0x82 },
    LedMapping { driver: 1, r: 0x66, g: 0x76, b: 0x86 },
    LedMapping { driver: 1, r: 0x67, g: 0x77, b: 0x87 },
    LedMapping { driver: 1, r: 0x68, g: 0x78, b: 0x88 },
    LedMapping { driver: 1, r: 0x69, g: 0x79, b: 0x89 },
    LedMapping { driver: 0, r: 0x60, g: 0x70, b: 0x80 },
    // ── Indices 56–65 ───────────────────────────────────────────────
    LedMapping { driver: 0, r: 0x61, g: 0x71, b: 0x81 },
    LedMapping { driver: 0, r: 0x62, g: 0x72, b: 0x82 },
    LedMapping { driver: 0, r: 0x63, g: 0x73, b: 0x83 },
    LedMapping { driver: 0, r: 0x64, g: 0x74, b: 0x84 },
    LedMapping { driver: 0, r: 0x65, g: 0x75, b: 0x85 },
    LedMapping { driver: 0, r: 0x66, g: 0x76, b: 0x86 },
    LedMapping { driver: 0, r: 0x67, g: 0x77, b: 0x87 },
    LedMapping { driver: 0, r: 0x68, g: 0x78, b: 0x88 },
    LedMapping { driver: 0, r: 0x69, g: 0x79, b: 0x89 },
    LedMapping { driver: 0, r: 0x6A, g: 0x7A, b: 0x8A },
    // ── Indices 66–70 ───────────────────────────────────────────────
    LedMapping { driver: 1, r: 0x6F, g: 0x7F, b: 0x8F },
    LedMapping { driver: 1, r: 0x6D, g: 0x7D, b: 0x8D },
    LedMapping { driver: 1, r: 0x6C, g: 0x7C, b: 0x8C },
    LedMapping { driver: 1, r: 0x6B, g: 0x7B, b: 0x8B },
    LedMapping { driver: 1, r: 0x6A, g: 0x7A, b: 0x8A },
    // ── Indices 71–80 ───────────────────────────────────────────────
    LedMapping { driver: 0, r: 0x90, g: 0xA0, b: 0xB0 },
    LedMapping { driver: 0, r: 0x92, g: 0xA2, b: 0xB2 },
    LedMapping { driver: 0, r: 0x93, g: 0xA3, b: 0xB3 },
    LedMapping { driver: 0, r: 0x94, g: 0xA4, b: 0xB4 },
    LedMapping { driver: 0, r: 0x95, g: 0xA5, b: 0xB5 },
    LedMapping { driver: 0, r: 0x96, g: 0xA6, b: 0xB6 },
    LedMapping { driver: 0, r: 0x97, g: 0xA7, b: 0xB7 },
    LedMapping { driver: 0, r: 0x98, g: 0xA8, b: 0xB8 },
    LedMapping { driver: 0, r: 0x99, g: 0xA9, b: 0xB9 },
    LedMapping { driver: 0, r: 0x9A, g: 0xAA, b: 0xBA },
    // ── Indices 81–87 ───────────────────────────────────────────────
    LedMapping { driver: 1, r: 0x90, g: 0xA0, b: 0xB0 },
    LedMapping { driver: 1, r: 0x92, g: 0xA2, b: 0xB2 },
    LedMapping { driver: 1, r: 0x93, g: 0xA3, b: 0xB3 },
    LedMapping { driver: 1, r: 0x94, g: 0xA4, b: 0xB4 },
    LedMapping { driver: 1, r: 0x95, g: 0xA5, b: 0xB5 },
    LedMapping { driver: 1, r: 0x96, g: 0xA6, b: 0xB6 },
    LedMapping { driver: 1, r: 0x97, g: 0xA7, b: 0xB7 },
    // ── Indices 88–92 ───────────────────────────────────────────────
    LedMapping { driver: 0, r: 0x9F, g: 0xAF, b: 0xBF },
    LedMapping { driver: 0, r: 0x9E, g: 0xAE, b: 0xBE },
    LedMapping { driver: 0, r: 0x9D, g: 0xAD, b: 0xBD },
    LedMapping { driver: 0, r: 0x9C, g: 0xAC, b: 0xBC },
    LedMapping { driver: 0, r: 0x9B, g: 0xAB, b: 0xBB },
    // ── Indices 93–99 ───────────────────────────────────────────────
    LedMapping { driver: 1, r: 0x9F, g: 0xAF, b: 0xBF },
    LedMapping { driver: 1, r: 0x9D, g: 0xAD, b: 0xBD },
    LedMapping { driver: 1, r: 0x9C, g: 0xAC, b: 0xBC },
    LedMapping { driver: 1, r: 0x9B, g: 0xAB, b: 0xBB },
    LedMapping { driver: 1, r: 0x9A, g: 0xAA, b: 0xBA },
    LedMapping { driver: 1, r: 0x99, g: 0xA9, b: 0xB9 },
    LedMapping { driver: 1, r: 0x98, g: 0xA8, b: 0xB8 },
    // ── Indices 100–104: Side LEDs LEFT ──────────────────────────────
    LedMapping { driver: 1, r: 0x04, g: 0x14, b: 0x24 },
    LedMapping { driver: 1, r: 0x03, g: 0x13, b: 0x23 },
    LedMapping { driver: 1, r: 0x02, g: 0x12, b: 0x22 },
    LedMapping { driver: 1, r: 0x01, g: 0x11, b: 0x21 },
    LedMapping { driver: 1, r: 0x00, g: 0x10, b: 0x20 },
    // ── Indices 105–109: Side LEDs RIGHT ─────────────────────────────
    LedMapping { driver: 1, r: 0x05, g: 0x15, b: 0x25 },
    LedMapping { driver: 1, r: 0x06, g: 0x16, b: 0x26 },
    LedMapping { driver: 1, r: 0x07, g: 0x17, b: 0x27 },
    LedMapping { driver: 1, r: 0x08, g: 0x18, b: 0x28 },
    LedMapping { driver: 1, r: 0x09, g: 0x19, b: 0x29 },
];

pub struct RgbMatrix {
    /// PWM buffer: [LED_COUNT][3] = RGB per LED (0-255)
    buffer: [[u8; 3]; LED_COUNT],
    /// Dirty flag — true if buffer changed since last flush
    dirty: bool,
    /// Current mode
    pub mode: u8,
    /// Current hue (0-255)
    pub hue: u8,
    /// Current saturation (0-255)
    pub sat: u8,
    /// Current brightness (value, 0-255)
    pub val: u8,
    /// Animation speed (0-255, higher = faster)
    pub speed: u8,
    /// Enabled flag
    pub enabled: bool,
    /// Suspend state
    pub suspended: bool,
}

impl RgbMatrix {
    pub fn new() -> Self {
        Self {
            buffer: [[0; 3]; LED_COUNT],
            dirty: true,
            mode: 0,
            hue: 255,
            sat: 255,
            val: 223,  // QMK default: 255 - VAL_STEP*2 = 223
            speed: 223, // QMK default: 255 - SPD_STEP*2 = 223
            enabled: true,
            suspended: false,
        }
    }

    /// Set a single LED (port of rgb_matrix_set_color)
    pub fn set_color(&mut self, index: usize, r: u8, g: u8, b: u8) {
        if index < LED_COUNT {
            let prev = self.buffer[index];
            if prev != [r, g, b] {
                self.buffer[index] = [r, g, b];
                self.dirty = true;
            }
        }
    }

    /// Set all LEDs (port of rgb_matrix_set_color_all)
    pub fn set_all(&mut self, r: u8, g: u8, b: u8) {
        for led in &mut self.buffer {
            *led = [r, g, b];
        }
        self.dirty = true;
    }

    /// Convert HSV to RGB and set all (port of rgb_matrix_sethsv)
    pub fn set_hsv(&mut self, h: u8, s: u8, v: u8) {
        self.hue = h;
        self.sat = s;
        self.val = v;
        let (r, g, b) = hsv_to_rgb(h, s, v);
        self.set_all(r, g, b);
    }

    /// Check if dirty and needs I2C flush
    pub fn needs_flush(&self) -> bool { self.dirty }

    /// Build per-driver PWM buffers for batched I2C write.
    /// Returns (driver1_buffer, driver2_buffer) — each 192 bytes covering all PWM registers.
    /// Caller must: select PWM page, then write buf at register 0x00.
    /// Port of rgb_matrix_update_pwm_buffers → IS31FL3733_write_pwm_buffer
    pub fn build_pwm_buffers(&mut self) -> ([u8; 192], [u8; 192]) {
        let mut buf1 = [0u8; 192];
        let mut buf2 = [0u8; 192];

        if !self.dirty && !self.enabled {
            return (buf1, buf2);
        }

        for (i, led) in self.buffer.iter().enumerate() {
            let map = &LED_MAP[i];
            let buf = if map.driver == 0 { &mut buf1 } else { &mut buf2 };
            buf[map.r as usize] = led[0];
            buf[map.g as usize] = led[1];
            buf[map.b as usize] = led[2];
        }
        self.dirty = false;
        (buf1, buf2)
    }

    /// Initialize IS31FL3733 drivers over I2C (port of IS31FL3733_init)
    /// Returns list of (addr, reg, value) I2C writes for init sequence.
    pub fn init_sequence() -> Vec<(u8, u8, u8), 8> {
        let mut writes: Vec<(u8, u8, u8), 8> = Vec::new();
        let _ = writes.push((DRIVER_ADDR_1, REG_SHUTDOWN, 0x01));
        let _ = writes.push((DRIVER_ADDR_1, REG_CONFIG, CONFIG_NORMAL));
        let _ = writes.push((DRIVER_ADDR_2, REG_SHUTDOWN, 0x01));
        let _ = writes.push((DRIVER_ADDR_2, REG_CONFIG, CONFIG_NORMAL));
        writes
    }
}

/// HSV → RGB conversion (standard algorithm, port of QMK hsv_to_rgb)
fn hsv_to_rgb(h: u8, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 { return (v, v, v); }
    let region = (h as u16 * 6) / 256;
    let remainder = ((h as u16 * 6) % 256) as u8;
    let p = ((v as u16 * (255 - s as u16)) / 255) as u8;
    let q = ((v as u16 * (255 - (s as u16 * remainder as u16 / 256))) / 255) as u8;
    let t = ((v as u16 * (255 - (s as u16 * (255 - remainder as u16) / 256))) / 255) as u8;
    match region {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}
