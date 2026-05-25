//! QMK RGB matrix animation framework — port of lib8tion math + 8 animations
//!
//! Math helpers ported from lib/lib8tion/{trig8.h, scale8.h, math8.h}:
//!   sin8, cos8, abs8, scale8, scale16by8, sqrt16, qadd8
//!
//! Effect runners ported from quantum/rgb_matrix/animations/runners/:
//!   effect_runner_i, effect_runner_dx_dy, effect_runner_dx_dy_dist
//!
//! Animations ported from quantum/rgb_matrix/animations/:
//!   breathing, cycle_all, rainbow_moving_chevron, gradient_up_down,
//!   gradient_left_right, cycle_up_down, cycle_left_right, cycle_out_in

use super::rgb::{RgbMatrix, LED_COUNT, hsv_to_rgb};

// ── Math helpers ──────────────────────────────────────────────────────

/// sin8: Fast 8-bit sin approximation. Input 0-255, output 0-255.
/// Port of QMK's sin8_C / lib8tion trig8.h
pub fn sin8(theta: u8) -> u8 {
    // b_m16_interleave table: [b, m, b, m, ...] for 4 sections
    // Each pair: b (intercept), m (slope * 16)
    // Port of the sin8_C algorithm from trig8.h
    let mut offset = theta;
    if theta & 0x40 != 0 {
        offset = 255u8.wrapping_sub(offset);
    }
    offset &= 0x3F; // 0..63

    let secoffset = if theta & 0x40 != 0 {
        (offset & 0x0F).wrapping_add(1) // 1..16
    } else {
        offset & 0x0F // 0..15
    };

    let section = (offset >> 4) as usize; // 0..3
    let s2 = section * 2;

    // b_m16_interleave: [b0, m16_0, b1, m16_1, b2, m16_2, b3, m16_3]
    const B_M16: [u8; 8] = [0, 49, 49, 41, 90, 27, 117, 10];
    let b = B_M16[s2];
    let m16 = B_M16[s2 + 1];

    let mx = ((m16 as u16 * secoffset as u16) >> 4) as u8;

    let mut y: i8 = (mx as i8).wrapping_add(b as i8);
    if theta & 0x80 != 0 {
        y = y.wrapping_neg();
    }

    (y as i16 + 128) as u8
}

/// cos8: Fast 8-bit cos approximation. Input 0-255, output 0-255.
pub fn cos8(theta: u8) -> u8 {
    sin8(theta.wrapping_add(64))
}

/// abs8: Absolute value of a signed 8-bit int.
pub fn abs8(i: i8) -> u8 {
    if i < 0 {
        (-i) as u8
    } else {
        i as u8
    }
}

/// scale8: Scale i by fraction scale/256. i * (scale / 256)
pub fn scale8(i: u8, scale: u8) -> u8 {
    ((i as u16 * scale as u16) >> 8) as u8
}

/// scale16by8: Scale 16-bit i by 8-bit fraction scale/256.
pub fn scale16by8(i: u16, scale: u8) -> u16 {
    ((i as u32 * scale as u32) >> 8) as u16
}

/// qadd8: Saturating unsigned 8-bit add, cap at 255.
pub fn qadd8(i: u8, j: u8) -> u8 {
    let t = i as u16 + j as u16;
    if t > 255 { 255 } else { t as u8 }
}

/// sqrt16: Integer square root for 16-bit values.
/// Port of QMK's sqrt16 from math8.h
pub fn sqrt16(x: u16) -> u8 {
    if x <= 1 {
        return x as u8;
    }

    let mut low: u8 = 1;
    let mut hi: u8;
    if x > 7904 {
        hi = 255;
    } else {
        hi = ((x >> 5) + 8) as u8;
    }

    loop {
        let mid = ((low as u16 + hi as u16) >> 1) as u8;
        if (mid as u16 * mid as u16) > x {
            if mid == 0 { return 0; }
            hi = mid - 1;
        } else {
            if mid == 255 {
                return 255;
            }
            low = mid + 1;
        }
        if hi < low {
            break;
        }
    }
    low.wrapping_sub(1)
}

// ── LED position table ────────────────────────────────────────────────
// Extracted from keyboard.json rgb_matrix.layout for NuPhy Air96 V2.
// Positions are in QMK keyboard units (uint8_t range, ~0-224 x, ~0-64 y).
// Indices 0-99: main keyboard matrix LEDs (extracted from JSON)
// Indices 100-104: side LEDs LEFT  (x=0, evenly spaced y)
// Indices 105-109: side LEDs RIGHT (x=224, evenly spaced y)

pub const LED_POSITIONS: [(u8, u8); LED_COUNT] = [
    // ── Index 0-18: Row 0 (F-key row) ───────────────────────────────
    (0, 0), (10, 0), (20, 0), (30, 0), (40, 0), (50, 0), (60, 0), (70, 0), (80, 0), (90, 0),
    (100, 0), (110, 0), (120, 0), (130, 0), (140, 0), (150, 0), (160, 0), (170, 0), (180, 0),
    // ── Index 19-36: Row 1 (number row) ─────────────────────────────
    (0, 10), (10, 10), (20, 10), (30, 10), (40, 10), (50, 10), (60, 10), (70, 10), (80, 10), (90, 10),
    (100, 10), (110, 10), (120, 10), (130, 10), (150, 10), (160, 10), (170, 10), (180, 10),
    // ── Index 37-54: Row 2 (QWERTY row) ─────────────────────────────
    (0, 20), (15, 20), (25, 20), (35, 20), (45, 20), (55, 20), (65, 20), (75, 20), (85, 20), (95, 20),
    (105, 20), (115, 20), (125, 20), (135, 20), (150, 20), (160, 20), (170, 20), (180, 20),
    // ── Index 55-70: Row 3 (ASDF row) ───────────────────────────────
    (0, 30), (18, 30), (28, 30), (38, 30), (48, 30), (58, 30), (68, 30), (78, 30), (88, 30), (98, 30),
    (108, 30), (118, 30), (128, 30), (150, 30), (160, 30), (170, 30),
    // ── Index 71-82: Row 4 (ZXCV row, left side) ────────────────────
    (0, 40), (23, 40), (33, 40), (43, 40), (53, 40), (63, 40), (73, 40), (83, 40), (93, 40),
    (103, 40), (113, 40), (123, 40),
    // ── Index 83-87: Row 4 (arrow keys + right side) ────────────────
    (140, 40), (150, 40), (160, 40), (170, 40), (180, 40),
    // ── Index 88-99: Row 5 (bottom modifier row) ────────────────────
    (0, 50), (13, 50), (25, 50), (66, 50), (100, 50), (110, 50), (120, 50), (130, 50), (140, 50),
    (150, 50), (160, 50), (170, 50),
    // ── Index 100-104: Side LEDs LEFT ────────────────────────────────
    (0, 5), (0, 15), (0, 25), (0, 35), (0, 45),
    // ── Index 105-109: Side LEDs RIGHT ───────────────────────────────
    (224, 5), (224, 15), (224, 25), (224, 35), (224, 45),
];

/// RGB matrix center point (from keyboard.json: [90, 25])
pub const CENTER_X: u8 = 90;
pub const CENTER_Y: u8 = 25;

// ── Animation functions ───────────────────────────────────────────────

/// Common "time" calculation ported from QMK's effect_runner_i time scaling.
/// Returns a u8 time value driven by anim_tick and speed.
fn compute_time(anim_tick: u16, speed: u8, divisor: u8) -> u8 {
    let scaled = scale16by8(anim_tick, qadd8(speed.wrapping_div(divisor), 1));
    scaled as u8
}

/// render_breathing: QMK BREATHING effect.
/// Pulses brightness using sin8 wave.
pub fn render_breathing(rgb: &mut RgbMatrix) {
    let hsv_h = rgb.hue;
    let hsv_s = rgb.sat;
    let hsv_v_base = rgb.val;
    let time = compute_time(rgb.anim_tick, rgb.speed, 8);
    // QMK: hsv.v = scale8(abs8(sin8(time) - 128) * 2, hsv.v);
    // sin8 returns 0..255, subtract 128 gives -128..127, abs8 gives 0..128
    let raw = sin8(time);
    let diff: i8 = (raw as i16 - 128i16) as i8;
    let v_scaled = abs8(diff).wrapping_mul(2);
    let v = scale8(v_scaled, hsv_v_base);
    let (r, g, b) = hsv_to_rgb(hsv_h, hsv_s, v);
    rgb.set_all(r, g, b);
}

/// render_cycle_all: QMK CYCLE_ALL effect.
/// All LEDs show the same color cycling through hue.
pub fn render_cycle_all(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let (r, g, b) = hsv_to_rgb(time, rgb.sat, rgb.val);
    rgb.set_all(r, g, b);
}

/// render_rainbow_moving_chevron: QMK RAINBOW_MOVING_CHEVRON effect.
/// Chevron-shaped rainbow pattern moving left-to-right.
pub fn render_rainbow_moving_chevron(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dy = if py >= CENTER_Y { py - CENTER_Y } else { CENTER_Y - py };
        let hue = rgb.hue.wrapping_add(dy).wrapping_add(px.wrapping_sub(time));
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// render_gradient_up_down: QMK GRADIENT_UP_DOWN effect.
/// Static gradient from top to bottom.
pub fn render_gradient_up_down(rgb: &mut RgbMatrix) {
    let scale = scale8(64, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (_, py) = LED_POSITIONS[i];
        let hue = rgb.hue.wrapping_add(scale.wrapping_mul(py >> 4));
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// render_gradient_left_right: QMK GRADIENT_LEFT_RIGHT effect.
/// Static gradient from left to right.
pub fn render_gradient_left_right(rgb: &mut RgbMatrix) {
    let scale = scale8(64, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let hue = rgb.hue.wrapping_add((scale as u16 * px as u16 >> 5) as u8);
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// render_cycle_up_down: QMK CYCLE_UP_DOWN effect.
/// Hue varies by Y position, cycles over time.
pub fn render_cycle_up_down(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (_, py) = LED_POSITIONS[i];
        let hue = py.wrapping_sub(time);
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// render_cycle_left_right: QMK CYCLE_LEFT_RIGHT effect.
/// Hue varies by X position, cycles over time.
pub fn render_cycle_left_right(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let hue = px.wrapping_sub(time);
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// render_cycle_out_in: QMK CYCLE_OUT_IN effect.
/// Hue varies by distance from center, cycles outward.
pub fn render_cycle_out_in(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 2);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        // QMK: hsv.h = 3 * dist / 2 + time
        let hue = ((3u16 * dist as u16) >> 1).wrapping_add(time as u16);
        let (r, g, b) = hsv_to_rgb(hue as u8, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// Total number of animation modes (0 = solid/manual, 1-8 = animations, 9+ wraps)
pub const ANIMATION_COUNT: u8 = 9;

/// Dispatch an animation tick to the current mode.
/// mode 0 = solid (manual), 1-8 = animations, 9+ = wraps to 1.
pub fn tick_animation(rgb: &mut RgbMatrix) {
    if rgb.mode == 0 {
        return; // Solid — manual set_hsv controls color
    }
    rgb.anim_tick = rgb.anim_tick.wrapping_add(1);

    match rgb.mode {
        1 => render_breathing(rgb),
        2 => render_cycle_all(rgb),
        3 => render_cycle_up_down(rgb),
        4 => render_cycle_left_right(rgb),
        5 => render_cycle_out_in(rgb),
        6 => render_gradient_up_down(rgb),
        7 => render_gradient_left_right(rgb),
        8 => render_rainbow_moving_chevron(rgb),
        _ => render_cycle_left_right(rgb), // default fallback
    }
}
