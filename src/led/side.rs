//! Side LED animations — full port of side.c
//!
//! 10 side LEDs (5 left + 5 right) at RGB matrix indices 100-109.
//! 5 modes: Wave, Mix (spectrum), Static, Breath, Off.
//! Includes battery indicator, system switch indicator, sleep indicator,
//! RF link indicator, and Caps Lock indicator.
#![allow(clippy::needless_range_loop)]
#![allow(unused_parens)]
#![allow(clippy::if_same_then_else)]
#![allow(dead_code)]

use crate::wireless::uart::{UartProtocol, LinkMode, RfState};

// ── Mode constants ────────────────────────────────────────────────────
pub const SIDE_WAVE: u8 = 0;
pub const SIDE_MIX: u8 = 1;
pub const SIDE_STATIC: u8 = 2;
pub const SIDE_BREATH: u8 = 3;
pub const SIDE_OFF: u8 = 4;

// LED indices
const SIDE_INDEX: usize = 100;
const SIDE_LINE: usize = 5;

// Left/right LED index pairs (port of side_led_index_tab[5][2])
const SIDE_LED_INDICES: [[usize; 2]; 5] = [
    [SIDE_INDEX + 4, SIDE_INDEX + 5],
    [SIDE_INDEX + 3, SIDE_INDEX + 6],
    [SIDE_INDEX + 2, SIDE_INDEX + 7],
    [SIDE_INDEX + 1, SIDE_INDEX + 8],
    [SIDE_INDEX, SIDE_INDEX + 9],
];

// ── Colour library (port of colour_lib[9][3] from side.h) ─────────────
pub const COLOUR_LIB: [[u8; 3]; 9] = [
    [0xFF, 0x00, 0x00], // 0: Red
    [0xFF, 0x40, 0x00], // 1: Orange
    [0xFF, 0xFF, 0x00], // 2: Yellow
    [0x00, 0xFF, 0x00], // 3: Green
    [0x00, 0xFF, 0xFF], // 4: Cyan
    [0x00, 0x00, 0xFF], // 5: Blue
    [0x80, 0x00, 0xFF], // 6: Purple
    [0xC0, 0xC0, 0xFF], // 7: Lavender
    [0x00, 0x00, 0x00], // 8: Black/Off
];

// ── Light value table (port of light_value_tab[101] from side.h) ──────
const LIGHT_VALUE_TAB: [u8; 101] = [
    0, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 36, 37, 39, 40, 42, 43, 45, 47, 49, 51,
    53, 55, 57, 59, 61, 63, 65, 67, 69, 71, 73, 75,
    77, 79, 81, 83, 85, 87, 89, 91, 94, 96, 99, 101,
    104, 106, 109, 111, 114, 116, 119, 121, 124, 126, 129, 131,
    134, 137, 140, 143, 146, 149, 152, 155, 158, 161, 164, 167,
    170, 173, 176, 179, 182, 185, 188, 191, 194, 197, 200, 203,
    206, 209, 213, 216, 220, 223, 227, 230, 234, 237, 241, 245,
    248, 251, 255, 255, 255,
];

const SIDE_LIGHT_TABLE: [u8; 6] = [0, 32, 64, 96, 128, 160];
const SIDE_SPEED_TABLE: [[u8; 5]; 5] = [
    [14, 19, 25, 32, 40],
    [14, 19, 25, 32, 40],
    [50, 50, 50, 50, 50],
    [14, 19, 25, 32, 40],
    [50, 50, 50, 50, 50],
];

const BREATHE_TAB_LEN: usize = 128;
const BREATHE_TAB: [u8; BREATHE_TAB_LEN] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 16, 18, 20,
    22, 24, 27, 30, 33, 36, 39, 42, 45, 49, 53, 57, 61, 65, 69, 73,
    77, 81, 85, 89, 94, 99, 104, 109, 114, 119, 124, 129, 134, 140, 146, 152,
    158, 164, 170, 176, 182, 188, 194, 200, 206, 213, 220, 227, 234, 241, 248, 255,
    255, 248, 241, 234, 227, 220, 213, 206, 200, 194, 188, 182, 176, 170, 164, 158,
    152, 146, 140, 134, 129, 124, 119, 114, 109, 104, 99, 94, 89, 85, 81, 77,
    73, 69, 65, 61, 57, 53, 49, 45, 42, 39, 36, 33, 30, 27, 24, 22,
    20, 18, 16, 14, 12, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// ── Wave data table (port of wave_data_tab[112] from side.h) ──────────
const WAVE_TAB_LEN: usize = 112;
const WAVE_DATA_TAB: [u8; WAVE_TAB_LEN] = [
    22, 23, 24, 25, 27, 28, 30, 31,
    33, 34, 36, 37, 39, 40, 42, 43,
    45, 47, 49, 51, 53, 55, 57, 59,
    61, 63, 65, 76, 69, 71, 73, 75,
    77, 81, 85, 89, 94, 99, 104, 109,
    114, 119, 124, 129, 134, 140, 146, 152,
    158, 164, 170, 176, 182, 188, 194, 200,
    206, 213, 220, 227, 234, 241, 248, 255,
    255, 248, 241, 234, 227, 220, 213, 206,
    200, 194, 188, 182, 176, 170, 164, 158,
    152, 146, 140, 134, 129, 124, 119, 114,
    109, 104, 99, 94, 89, 85, 81, 77,
    73, 69, 65, 61, 57, 53, 49, 45,
    42, 39, 36, 33, 30, 27, 24, 22,
];

// ── Flow rainbow colour table (port of flow_rainbow_colour_tab[224][3] from side.h) ─
const FLOW_COLOUR_TAB_LEN: usize = 224;
const FLOW_RAINBOW_COLOUR_TAB: [[u8; 3]; FLOW_COLOUR_TAB_LEN] = [
    [255, 8, 8], [255, 8, 8], [255, 8, 8], [255, 8, 8],
    [255, 10, 8], [255, 14, 8], [255, 18, 8], [255, 22, 8],
    [255, 26, 8], [255, 32, 8], [255, 38, 8], [255, 44, 8],
    [255, 50, 8], [255, 57, 8], [255, 65, 8], [255, 73, 8],
    [255, 81, 8], [255, 89, 8], [255, 99, 8], [255, 109, 8],
    [255, 119, 8], [255, 129, 8], [255, 140, 8], [255, 152, 8],
    [255, 164, 8], [255, 176, 8], [255, 188, 8], [255, 200, 8],
    [255, 213, 8], [255, 227, 8], [255, 241, 8], [255, 255, 8],
    [248, 255, 8], [234, 255, 8], [220, 255, 8], [206, 255, 8],
    [194, 255, 8], [182, 255, 8], [170, 255, 8], [158, 255, 8],
    [146, 255, 8], [134, 255, 8], [124, 255, 8], [114, 255, 8],
    [104, 255, 8], [94, 255, 8], [85, 255, 8], [77, 255, 8],
    [69, 255, 8], [61, 255, 8], [53, 255, 8], [47, 255, 8],
    [41, 255, 8], [35, 255, 8], [29, 255, 8], [24, 255, 8],
    [20, 255, 8], [16, 255, 8], [12, 255, 8], [8, 255, 8],
    [8, 255, 8], [8, 255, 8], [8, 255, 8], [8, 255, 8],
    [8, 255, 8], [8, 255, 8], [8, 255, 8], [8, 255, 8],
    [8, 255, 10], [8, 255, 14], [8, 255, 18], [8, 255, 22],
    [8, 255, 26], [8, 255, 32], [8, 255, 38], [8, 255, 44],
    [8, 255, 50], [8, 255, 57], [8, 255, 65], [8, 255, 73],
    [8, 255, 81], [8, 255, 89], [8, 255, 99], [8, 255, 109],
    [8, 255, 119], [8, 255, 129], [8, 255, 140], [8, 255, 152],
    [8, 255, 164], [8, 255, 176], [8, 255, 188], [8, 255, 200],
    [8, 255, 213], [8, 255, 227], [8, 255, 241], [8, 255, 255],
    [8, 248, 255], [8, 234, 255], [8, 220, 255], [8, 206, 255],
    [8, 194, 255], [8, 182, 255], [8, 170, 255], [8, 158, 255],
    [8, 146, 255], [8, 134, 255], [8, 124, 255], [8, 114, 255],
    [8, 104, 255], [8, 94, 255], [8, 85, 255], [8, 77, 255],
    [8, 69, 255], [8, 61, 255], [8, 53, 255], [8, 47, 255],
    [8, 41, 255], [8, 35, 255], [8, 29, 255], [8, 24, 255],
    [8, 20, 255], [8, 16, 255], [8, 12, 255], [8, 8, 255],
    [8, 8, 255], [8, 8, 255], [8, 8, 255], [8, 8, 255],
    [8, 8, 255], [8, 8, 255], [8, 8, 255], [8, 8, 255],
    [10, 8, 255], [14, 8, 255], [18, 8, 255], [22, 8, 255],
    [26, 8, 255], [32, 8, 255], [38, 8, 255], [44, 8, 255],
    [50, 8, 255], [57, 8, 255], [65, 8, 255], [73, 8, 255],
    [81, 8, 255], [89, 8, 255], [99, 8, 255], [109, 8, 255],
    [119, 8, 255], [129, 8, 255], [140, 8, 255], [152, 8, 255],
    [164, 8, 255], [176, 8, 255], [188, 8, 255], [200, 8, 255],
    [213, 8, 255], [227, 8, 255], [241, 8, 255], [255, 8, 255],
    [255, 8, 255], [255, 8, 255], [255, 8, 255], [255, 8, 255],
    [255, 10, 255], [255, 14, 255], [255, 18, 255], [255, 22, 255],
    [255, 26, 255], [255, 32, 255], [255, 38, 255], [255, 44, 255],
    [255, 50, 255], [255, 57, 255], [255, 65, 255], [255, 73, 255],
    [255, 81, 255], [255, 89, 255], [255, 99, 255], [255, 109, 255],
    [255, 119, 255], [255, 129, 255], [255, 140, 255], [255, 152, 255],
    [255, 164, 255], [255, 176, 255], [255, 188, 255], [255, 200, 255],
    [255, 213, 255], [255, 227, 255], [255, 241, 255], [255, 255, 255],
    [255, 248, 248], [255, 234, 234], [255, 220, 220], [255, 206, 206],
    [255, 194, 194], [255, 182, 182], [255, 170, 170], [255, 158, 158],
    [255, 146, 146], [255, 134, 134], [255, 124, 124], [255, 114, 114],
    [255, 104, 104], [255, 94, 94], [255, 85, 85], [255, 77, 77],
    [255, 69, 69], [255, 61, 61], [255, 53, 53], [255, 47, 47],
    [255, 41, 41], [255, 35, 35], [255, 29, 29], [255, 24, 24],
    [255, 20, 20], [255, 16, 16], [255, 12, 12], [255, 8, 8],
    [255, 8, 8], [255, 8, 8], [255, 8, 8], [255, 8, 8],
];

// ── Side LED state ────────────────────────────────────────────────────
pub struct SideLeds {
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub colour: u8,
    pub rgb_enabled: bool,

    play_point: u8,
    pub(crate) play_cnt: u8,
    play_timer: u32,

    // Breath mode persistent play_point (port of static uint8_t in C)
    breath_play_point: u8,

    // Battery indicator state
    bat_show_flag: bool,
    bat_show_breath: bool,
    bat_play_timer: u32,
    bat_play_point: u8,
    bat_show_time: u32,
    bat_sts_debounce: u32,
    bat_per_debounce: u32,
    charge_state: u8,
    bat_percent: u8,
    bat_init: bool,

    // RF link blink
    rf_blink_cnt: u8,
    rf_blink_timer: u32,
    pub rf_link_show_time: u16,
    pub link_state_temp: u8,

    // System/sleep show flags
    sys_show_flag: bool,
    sys_show_timer: u32,
    sleep_show_flag: bool,
    sleep_show_timer: u32,
    sleep_enabled: bool,

    /// Output buffer: per-LED RGB to be written to the RGB matrix
    pub output: [[u8; 3]; 10],
}

impl Default for SideLeds {
    fn default() -> Self {
        Self::new()
    }
}

impl SideLeds {
    pub fn new() -> Self {
        Self {
            mode: SIDE_WAVE, brightness: 3, speed: 2, colour: 0, rgb_enabled: true,
            play_point: 0, play_cnt: 0, play_timer: 0,
            breath_play_point: 0,
            bat_show_flag: true, bat_show_breath: false, bat_play_timer: 0, bat_play_point: 0,
            bat_show_time: 0, bat_sts_debounce: 0, bat_per_debounce: 0,
            charge_state: 0, bat_percent: 100, bat_init: true,
            rf_blink_cnt: 0, rf_blink_timer: 0, rf_link_show_time: 0,
            link_state_temp: 0xFF,
            sys_show_flag: false, sys_show_timer: 0,
            sleep_show_flag: false, sleep_show_timer: 0, sleep_enabled: true,
            output: [[0; 3]; 10],
        }
    }

    /// Set left side LEDs (indices 0-4)
    fn set_left(&mut self, r: u8, g: u8, b: u8) {
        for i in 0..5 { self.output[i] = [r, g, b]; }
    }

    /// Set right side LEDs (indices 5-9)
    fn set_right(&mut self, r: u8, g: u8, b: u8) {
        for i in 5..10 { self.output[i] = [r, g, b]; }
    }

    /// Apply brightness scaling (port of count_rgb_light, side.c:334-346)
    /// C formula: component * (light_temp + 1) >> 8
    fn apply_light(r: u8, g: u8, b: u8, light_temp: u8) -> (u8, u8, u8) {
        let scale = (light_temp as u16 + 1);
        (((r as u16 * scale) >> 8) as u8,
         ((g as u16 * scale) >> 8) as u8,
         ((b as u16 * scale) >> 8) as u8)
    }

    /// Point advance helper (port of light_point_playing)
    fn advance_point(point: &mut u8, step: u8, len: usize, forward: bool) {
        if forward {
            *point = ((*point as usize + step as usize) % len) as u8;
        } else {
            let new = (*point as i16) - (step as i16);
            *point = if new < 0 { ((new + len as i16) % len as i16) as u8 } else { new as u8 };
        }
    }

    // ── Wave mode (port of side_wave_mode_show) ───────────────────────
    fn wave_show(&mut self) {
        let speed = SIDE_SPEED_TABLE[SIDE_WAVE as usize][self.speed as usize];
        if self.play_cnt <= speed { return; }
        self.play_cnt -= speed;
        if self.play_cnt > 20 { self.play_cnt = 0; }

        if self.rgb_enabled {
            Self::advance_point(&mut self.play_point, 3, FLOW_COLOUR_TAB_LEN, false);
        } else {
            Self::advance_point(&mut self.play_point, 2, WAVE_TAB_LEN, false);
        }

        let mut play_idx = self.play_point;
        for i in 0..SIDE_LINE {
            if self.rgb_enabled {
                // Use exact FLOW_RAINBOW_COLOUR_TAB (port of H7 fix)
                let col = FLOW_RAINBOW_COLOUR_TAB[play_idx as usize];
                Self::advance_point(&mut play_idx, 16, FLOW_COLOUR_TAB_LEN, true);
                let (r, g, b) = Self::apply_light(col[0], col[1], col[2], SIDE_LIGHT_TABLE[self.brightness as usize]);
                for j in 0..2 {
                    self.output[SIDE_LED_INDICES[i][j] - SIDE_INDEX] = [r, g, b];
                }
            } else {
                // Single colour with wave intensity modulation (port of H8 fix)
                let col = COLOUR_LIB[self.colour as usize];
                let wave_val = WAVE_DATA_TAB[play_idx as usize];
                let (r, g, b) = Self::apply_light(col[0], col[1], col[2], wave_val);
                Self::advance_point(&mut play_idx, 12, WAVE_TAB_LEN, true);
                let (r, g, b) = Self::apply_light(r, g, b, SIDE_LIGHT_TABLE[self.brightness as usize]);
                for j in 0..2 {
                    self.output[SIDE_LED_INDICES[i][j] - SIDE_INDEX] = [r, g, b];
                }
            }
        }
    }

    // ── Mix/Spectrum mode (port of side_spectrum_mode_show) ───────────
    fn mix_show(&mut self) {
        let speed = SIDE_SPEED_TABLE[SIDE_MIX as usize][self.speed as usize];
        if self.play_cnt <= speed { return; }
        self.play_cnt -= speed;
        if self.play_cnt > 20 { self.play_cnt = 0; }

        Self::advance_point(&mut self.play_point, 1, FLOW_COLOUR_TAB_LEN, true);
        // Use exact FLOW_RAINBOW_COLOUR_TAB (H7 fix)
        let col = FLOW_RAINBOW_COLOUR_TAB[self.play_point as usize];
        let (r, g, b) = Self::apply_light(col[0], col[1], col[2], SIDE_LIGHT_TABLE[self.brightness as usize]);
        self.set_left(r, g, b);
        self.set_right(r, g, b);
    }

    // ── Breath mode (port of side_breathe_mode_show, B4 dead code removed) ─
    fn breath_show(&mut self) {
        let speed = SIDE_SPEED_TABLE[SIDE_BREATH as usize][self.speed as usize];
        if self.play_cnt <= speed { return; }
        self.play_cnt -= speed;
        if self.play_cnt > 20 { self.play_cnt = 0; }

        // M17 fix: use persistent breath_play_point (was fresh local)
        Self::advance_point(&mut self.breath_play_point, 1, BREATHE_TAB_LEN, false);

        let col = COLOUR_LIB[self.colour as usize];
        let (r, g, b) = Self::apply_light(col[0], col[1], col[2], BREATHE_TAB[self.breath_play_point as usize]);
        let (r, g, b) = Self::apply_light(r, g, b, SIDE_LIGHT_TABLE[self.brightness as usize]);
        for i in 0..SIDE_LINE {
            for j in 0..2 {
                self.output[SIDE_LED_INDICES[i][j] - SIDE_INDEX] = [r, g, b];
            }
        }
    }

    // ── Static mode (port of side_static_mode_show, B4 dead code removed) ─
    fn static_show(&mut self) {
        let speed = SIDE_SPEED_TABLE[SIDE_STATIC as usize][self.speed as usize];
        if self.play_cnt <= speed { return; }
        self.play_cnt -= speed;
        if self.play_cnt > 20 { self.play_cnt = 0; }

        let col = COLOUR_LIB[self.colour as usize];
        let (r, g, b) = Self::apply_light(col[0], col[1], col[2], SIDE_LIGHT_TABLE[self.brightness as usize]);
        self.set_left(r, g, b);
        self.set_right(r, g, b);
    }

    // ── Off mode ──────────────────────────────────────────────────────
    fn off_show(&mut self) {
        let speed = SIDE_SPEED_TABLE[SIDE_OFF as usize][self.speed as usize];
        if self.play_cnt <= speed { return; }
        self.play_cnt -= speed;
        if self.play_cnt > 20 { self.play_cnt = 0; }
        self.set_left(0, 0, 0);
        self.set_right(0, 0, 0);
    }

    // ── RF link LED indicator (port of rf_led_show, B1 fix applied) ──
    fn rf_led_show(&mut self, proto: &UartProtocol) {
        const RF_LED_LINK_PERIOD: u32 = 500;
        const RF_LED_PAIR_PERIOD: u32 = 250;

        let (r, g, b) = match proto.link_mode {
            LinkMode::Rf24 => (0x00, 0x80, 0x00),       // Green
            LinkMode::Usb  => (0x80, 0x80, 0x00),       // Yellow
            _              => (0x00, 0x00, 0x80),       // Blue
        };
        let mut out_r = r;
        let mut out_g = g;
        let mut out_b = b;

        if self.rf_blink_cnt > 0 {
            let period = if proto.rf_state == RfState::Pairing { RF_LED_PAIR_PERIOD } else { RF_LED_LINK_PERIOD };
            if self.rf_blink_timer < period / 2 {
                // LED on (first half)
            } else {
                out_r = 0; out_g = 0; out_b = 0;
            }
            if self.rf_blink_timer >= period {
                self.rf_blink_cnt -= 1;
                self.rf_blink_timer = 0;
            }
        } else if self.rf_link_show_time < 3000 {
            // Show steady after blink
        } else {
            return; // Don't override — let side animation show
        }
        self.set_left(out_r, out_g, out_b);
    }

    // ── Battery indicator (port of bat_led_show, B5 fix applied) ──────
    fn bat_led_show(&mut self, proto: &UartProtocol, f_bat_hold: bool) {
        let charge = proto.rf_charge;
        let battery = proto.rf_battery;

        if self.bat_init {
            self.bat_init = false;
            self.bat_show_time = 0;
            self.charge_state = charge;
            self.bat_percent = battery;
        }

        // Charge state changed → show indicator
        if self.charge_state != charge {
            if self.bat_sts_debounce > 1000 {
                if (self.charge_state & 0x01) == 0 && (charge & 0x01) != 0 {
                    self.bat_show_flag = true;
                    self.bat_show_breath = true;
                    self.bat_show_time = 0;
                }
                self.charge_state = charge;
            }
        } else {
            self.bat_sts_debounce = 0;
            if self.bat_show_time > 5000 && !f_bat_hold {
                self.bat_show_flag = false;
                self.bat_show_breath = false;
            }
            // M13 fix: full charge (0x03) → breathing indicator
            if self.charge_state == 0x03 {
                self.bat_show_breath = true;
            }
        }

        // Battery percent changed
        if self.bat_percent != battery {
            if self.bat_per_debounce > 1000 {
                self.bat_percent = battery;
            }
        } else {
            self.bat_per_debounce = 0;
            if self.bat_percent < 15 {
                self.bat_show_flag = true;
                self.bat_show_time = 0;
            }
        }

        // Manual hold override
        if f_bat_hold {
            self.bat_show_flag = true;
            self.bat_show_breath = false;
            self.bat_show_time = 0;
        }

        if self.bat_show_flag {
            if self.bat_show_breath {
                if self.bat_play_timer > 10 {
                    self.bat_play_timer = 0;
                    Self::advance_point(&mut self.bat_play_point, 1, BREATHE_TAB_LEN, false);
                }
                let b = BREATHE_TAB[self.bat_play_point as usize];
                let (r, g, _) = (0xFF, 0x40, 0x00); // Orange charging
                let (r, g, _) = Self::apply_light(r, g, 0, b);
                self.set_right(r, g, 0);
            } else {
                self.bat_percent_led(self.bat_percent);
            }
        }
    }

    pub(crate) fn bat_percent_led(&mut self, percent: u8) {
        // C firmware color scheme: (128,0,0) red ≤20%, (128,64,0) orange 21-95%, (0,128,0) green >95%
        let (r, g, b) = if percent <= 20 {
            (128, 0, 0)
        } else if percent <= 95 {
            (128, 64, 0)
        } else {
            (0, 128, 0)
        };
        let end = if percent <= 20 { 0 } else if percent <= 30 { 1 } else if percent <= 50 { 2 } else if percent <= 70 { 3 } else if percent <= 95 { 4 } else { 4 };
        for i in 0..=end {
            self.output[9 - i] = [r, g, b];
        }
        for i in (end + 1)..SIDE_LINE {
            self.output[9 - i] = [0, 0, 0];
        }
    }

    // ── Main update (port of m_side_led_show, side.c:829) ─────────────
    /// elapsed_ms is the number of 1ms ticks passed since last call.
    /// M11 fix: rf_link_show_time increments per 10ms, so divide elapsed by 10.
    pub fn update(&mut self, proto: &UartProtocol, elapsed_ms: u32, keyboard_leds: u8, f_bat_hold: bool) {
        self.play_cnt = self.play_cnt.saturating_add(elapsed_ms as u8);
        self.play_timer = 0;
        self.rf_blink_timer = self.rf_blink_timer.saturating_add(elapsed_ms);
        self.bat_play_timer = self.bat_play_timer.saturating_add(elapsed_ms);
        self.bat_sts_debounce = self.bat_sts_debounce.saturating_add(elapsed_ms);
        self.bat_per_debounce = self.bat_per_debounce.saturating_add(elapsed_ms);
        self.bat_show_time = self.bat_show_time.saturating_add(elapsed_ms);
        self.sys_show_timer = self.sys_show_timer.saturating_add(elapsed_ms);
        self.sleep_show_timer = self.sleep_show_timer.saturating_add(elapsed_ms);

        // 1. Side animation
        match self.mode {
            SIDE_WAVE   => self.wave_show(),
            SIDE_MIX    => self.mix_show(),
            SIDE_BREATH => self.breath_show(),
            SIDE_STATIC => self.static_show(),
            SIDE_OFF    => self.off_show(),
            _ => {}
        }

        // 2. Battery indicator (right side override)
        self.bat_led_show(proto, f_bat_hold);

        // 3. System switch indicator
        if self.sys_show_flag {
            let (r, g, b) = if proto.sys_sw_state == 0xA2 { (0x80, 0x80, 0x80) } else { (0x00, 0x00, 0x80) };
            if (self.sys_show_timer / 500).is_multiple_of(2) {
                self.set_right(r, g, b);
            } else {
                self.set_right(0, 0, 0);
            }
            if self.sys_show_timer >= 3000 { self.sys_show_flag = false; }
        }

        // 4. Sleep indicator (right side, 500ms blink, 3000ms duration)
        if self.sleep_show_flag {
            let (r, g, b) = if self.sleep_enabled { (0x00, 0x80, 0x00) } else { (0x80, 0x00, 0x00) };
            if (self.sleep_show_timer / 500).is_multiple_of(2) {
                self.set_right(r, g, b);
            } else {
                self.set_right(0, 0, 0);
            }
            if self.sleep_show_timer >= 3000 { self.sleep_show_flag = false; }
        }

        // 4. Caps Lock indicator (left side, white glow when on)
        let caps_lock = (keyboard_leds & 0x02) != 0;
        if caps_lock || (proto.rf_led & 0x02) != 0 {
            self.set_left(0xFF, 0xFF, 0xFF); // White
        }

        // 4b. Num Lock indicator (right side, white glow when on)
        let num_lock = (keyboard_leds & 0x01) != 0;
        if num_lock {
            self.set_right(0xFF, 0xFF, 0xFF); // White
        }

        // 5. RF link indicator (left side override)
        self.rf_led_show(proto);

        // M11 fix: rf_link_show_time increments every 1ms
        self.rf_link_show_time = self.rf_link_show_time.saturating_add(elapsed_ms as u16);
    }

    /// Trigger sleep indicator
    pub fn show_sleep(&mut self, enabled: bool) {
        self.sleep_enabled = enabled;
        self.sleep_show_flag = true;
        self.sleep_show_timer = 0;
    }

    /// Trigger system switch indicator
    pub fn show_sys(&mut self) {
        self.sys_show_flag = true;
        self.sys_show_timer = 0;
    }

    /// Trigger RF link blink
    pub fn blink_rf(&mut self, count: u8) {
        self.rf_blink_cnt = count;
        self.rf_blink_timer = 0;
        self.rf_link_show_time = 0;
    }

    /// Full device reset — port of device_reset_init (side.c:735-762)
    pub fn reset(&mut self) {
        self.mode = 0;
        self.brightness = 3;
        self.speed = 2;
        self.rgb_enabled = true;
        self.colour = 0;
        self.play_point = 0;
        self.play_cnt = 0;
        self.play_timer = 0;
    }
}
