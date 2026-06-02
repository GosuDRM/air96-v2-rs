//! QMK RGB matrix animation framework — port of lib8tion math + all 44 QMK animations
//!
//! Math helpers ported from lib/lib8tion/{trig8.h, scale8.h, math8.h}:
//!   sin8, cos8, abs8, scale8, scale16by8, sqrt16, qadd8, qsub8, atan2_8
//!
//! Animations ported from quantum/rgb_matrix/animations/

#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::needless_late_init)]
#![allow(dead_code)]
#![allow(unused_parens)]

use super::rgb::{RgbMatrix, LED_COUNT, hsv_to_rgb};

// ── Matrix dimensions (for framebuffer effects) ──────────────────────
pub const MATRIX_ROWS: usize = 6;
pub const MATRIX_COLS: usize = 19;
pub const NO_LED: u8 = 0xFF;

// ── Math helpers ──────────────────────────────────────────────────────

/// sin8: Fast 8-bit sin approximation. Input 0-255, output 0-255.
pub fn sin8(theta: u8) -> u8 {
    let mut offset = theta;
    if theta & 0x40 != 0 {
        offset = 255u8.wrapping_sub(offset);
    }
    offset &= 0x3F;

    let secoffset = if theta & 0x40 != 0 {
        (offset & 0x0F).wrapping_add(1)
    } else {
        offset & 0x0F
    };

    let section = (offset >> 4) as usize;
    let s2 = section * 2;

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
    if i < 0 { (-i) as u8 } else { i as u8 }
}

/// scale8: Scale i by fraction scale/256.
pub fn scale8(i: u8, scale: u8) -> u8 {
    ((i as u16 * scale as u16) >> 8) as u8
}

/// scale16by8: Scale 16-bit i by 8-bit fraction scale/256.
pub fn scale16by8(i: u16, scale: u8) -> u16 {
    ((i as u32 * scale as u32) >> 8) as u16
}

/// qadd8: Saturating unsigned 8-bit add.
pub fn qadd8(i: u8, j: u8) -> u8 {
    let t = i as u16 + j as u16;
    if t > 255 { 255 } else { t as u8 }
}

/// qsub8: Saturating unsigned 8-bit subtract (clamp to 0).
pub fn qsub8(i: u8, j: u8) -> u8 {
    i.saturating_sub(j)
}

/// sqrt16: Integer square root for 16-bit values.
pub fn sqrt16(x: u16) -> u8 {
    if x <= 1 { return x as u8; }
    let mut low: u8 = 1;
    let mut hi: u8;
    if x > 7904 { hi = 255; }
    else { hi = ((x >> 5) + 8) as u8; }
    loop {
        let mid = ((low as u16 + hi as u16) >> 1) as u8;
        if (mid as u16 * mid as u16) > x {
            if mid == 0 { return 0; }
            hi = mid - 1;
        } else {
            if mid == 255 { return 255; }
            low = mid + 1;
        }
        if hi < low { break; }
    }
    low.wrapping_sub(1)
}

/// atan2_8: Fast 16-bit approximation of atan2(dy,dx), output 0-255.
/// Port of lib8tion trig8.h atan2_8
pub fn atan2_8(dy: i16, dx: i16) -> u8 {
    if dy == 0 {
        return if dx >= 0 { 0 } else { 128 };
    }
    let abs_y: i16 = if dy > 0 { dy } else { -dy };
    let a: i8;
    if dx >= 0 {
        a = 32i8.wrapping_sub((32 * (dx - abs_y) / (dx + abs_y)) as i8);
    } else {
        a = 96i8.wrapping_sub((32 * (dx + abs_y) / (abs_y - dx)) as i8);
    }
    if dy < 0 {
        return (-a) as u8;
    }
    a as u8
}

// ── PRNG (FastLED algorithm) ─────────────────────────────────────────
static mut RAND16_SEED: u16 = 0xACE1;

/// Generate an 8-bit random number (FastLED rand algorithm)
pub fn random8() -> u8 {
    unsafe {
        RAND16_SEED = (RAND16_SEED.wrapping_mul(2053)).wrapping_add(13849);
        ((RAND16_SEED & 0xFF) as u8).wrapping_add((RAND16_SEED >> 8) as u8)
    }
}

/// Generate an 8-bit random number between 0 and lim (exclusive)
pub fn random8_max(lim: u8) -> u8 {
    let r = random8() as u16;
    ((r * lim as u16) >> 8) as u8
}

/// Generate an 8-bit random number between min and lim (exclusive)
pub fn random8_min_max(min: u8, lim: u8) -> u8 {
    let delta = lim.wrapping_sub(min);
    random8_max(delta).wrapping_add(min)
}

// ── LED position table ────────────────────────────────────────────────
// Extracted from keyboard.json rgb_matrix.layout for NuPhy Air96 V2.
// Indices 0-99: main keyboard matrix LEDs
// Indices 100-104: side LEDs LEFT (x=0)
// Indices 105-109: side LEDs RIGHT (x=224)

pub const LED_POSITIONS: [(u8, u8); LED_COUNT] = [
    // Row 0 (F-key row, y=0): indices 0-18
    (0, 0), (10, 0), (20, 0), (30, 0), (40, 0), (50, 0), (60, 0), (70, 0), (80, 0), (90, 0),
    (100, 0), (110, 0), (120, 0), (130, 0), (140, 0), (150, 0), (160, 0), (170, 0), (180, 0),
    // Row 1 (y=10): indices 19-36
    (0, 10), (10, 10), (20, 10), (30, 10), (40, 10), (50, 10), (60, 10), (70, 10), (80, 10), (90, 10),
    (100, 10), (110, 10), (120, 10), (130, 10), (150, 10), (160, 10), (170, 10), (180, 10),
    // Row 2 (y=20): indices 37-54
    (0, 20), (15, 20), (25, 20), (35, 20), (45, 20), (55, 20), (65, 20), (75, 20), (85, 20), (95, 20),
    (105, 20), (115, 20), (125, 20), (135, 20), (150, 20), (160, 20), (170, 20), (180, 20),
    // Row 3 (y=30): indices 55-70
    (0, 30), (18, 30), (28, 30), (38, 30), (48, 30), (58, 30), (68, 30), (78, 30), (88, 30), (98, 30),
    (108, 30), (118, 30), (128, 30), (150, 30), (160, 30), (170, 30),
    // Row 4 (y=40): indices 71-87
    (0, 40), (23, 40), (33, 40), (43, 40), (53, 40), (63, 40), (73, 40), (83, 40), (93, 40),
    (103, 40), (113, 40), (123, 40), (140, 40), (150, 40), (160, 40), (170, 40), (180, 40),
    // Row 5 (y=50): indices 88-99
    (0, 50), (13, 50), (25, 50), (66, 50), (100, 50), (110, 50), (120, 50), (130, 50), (140, 50),
    (150, 50), (160, 50), (170, 50),
    // Side LEDs LEFT: indices 100-104
    (0, 5), (0, 15), (0, 25), (0, 35), (0, 45),
    // Side LEDs RIGHT: indices 105-109
    (224, 5), (224, 15), (224, 25), (224, 35), (224, 45),
];

/// RGB matrix center point
pub const CENTER_X: u8 = 90;
pub const CENTER_Y: u8 = 25;

/// Matrix (row, col) → LED index mapping for framebuffer effects.
/// Returns NO_LED if no LED at that position.
pub fn matrix_co(row: usize, col: usize) -> u8 {
    if row >= MATRIX_ROWS || col >= MATRIX_COLS { return NO_LED; }
    // Build table at runtime (first call), then cache in static
    use core::sync::atomic::{AtomicBool, Ordering};
    static INIT: AtomicBool = AtomicBool::new(false);
    static mut MATRIX_TO_LED: [[u8; MATRIX_COLS]; MATRIX_ROWS] = [[NO_LED; MATRIX_COLS]; MATRIX_ROWS];
    
    if !INIT.load(Ordering::Relaxed) {
        // Safety: only written once before any reads, then read-only
        unsafe {
            let mut table = [[NO_LED; MATRIX_COLS]; MATRIX_ROWS];
            for i in 0..100usize { // main keyboard LEDs only
                let (x, y) = LED_POSITIONS[i];
                let r = (y / 10) as usize;
                if r < MATRIX_ROWS {
                    let mut col_idx: usize = 0;
                    for j in 0..i {
                        let (jx, jy) = LED_POSITIONS[j];
                        if (jy / 10) as usize == r && jx < x {
                            col_idx += 1;
                        }
                    }
                    if col_idx < MATRIX_COLS {
                        table[r][col_idx] = i as u8;
                    }
                }
            }
            MATRIX_TO_LED = table;
            INIT.store(true, Ordering::Relaxed);
        }
    }
    unsafe { MATRIX_TO_LED[row][col] }
}

/// Find an LED index for a given row/col, or NO_LED if none.
/// Slightly smarter version that uses x-position ordering.
pub fn matrix_co_ordered(row: usize, col: usize) -> u8 {
    if row >= MATRIX_ROWS || col >= MATRIX_COLS { return NO_LED; }
    use core::sync::atomic::{AtomicBool, Ordering};
    static INIT: AtomicBool = AtomicBool::new(false);
    static mut ORDERED_TABLE: [[u8; MATRIX_COLS]; MATRIX_ROWS] = [[NO_LED; MATRIX_COLS]; MATRIX_ROWS];

    if !INIT.load(Ordering::Relaxed) {
        unsafe {
            let mut table = [[NO_LED; MATRIX_COLS]; MATRIX_ROWS];
            for r in 0..MATRIX_ROWS {
                let target_y = (r * 10) as u8;
                for c in 0..MATRIX_COLS {
                    let mut col_count: usize = 0;
                    let mut found = NO_LED;
                    for i in 0..100usize {
                        let (_px, py) = LED_POSITIONS[i];
                        if py == target_y || (py > target_y.wrapping_sub(5) && py < target_y.wrapping_add(5)) {
                            if col_count == c {
                                found = i as u8;
                                break;
                            }
                            col_count += 1;
                        }
                    }
                    table[r][c] = found;
                }
            }
            ORDERED_TABLE = table;
            INIT.store(true, Ordering::Relaxed);
        }
    }
    unsafe { ORDERED_TABLE[row][col] }
}

/// Map row/col to LED index(es). Returns count and led indices.
pub fn map_row_column_to_led(row: u8, col: u8, out: &mut [u8; 8]) -> u8 {
    let target_y = (row * 10);
    let target_x = (col * 10);
    let mut count: u8 = 0;
    for i in 0..100usize {
        let (px, py) = LED_POSITIONS[i];
        if py == target_y && px == target_x {
            out[count as usize] = i as u8;
            count += 1;
            if count >= 8 { break; }
        }
    }
    count
}

// ── Common time helpers ──────────────────────────────────────────────

fn compute_time(anim_tick: u32, speed: u8, divisor: u8) -> u8 {
    let scaled = scale16by8(anim_tick as u16, qadd8(speed.wrapping_div(divisor), 1));
    scaled as u8
}

fn compute_time_speed_div(anim_tick: u32, speed: u8) -> u8 {
    scale16by8(anim_tick as u16, speed.wrapping_div(2)) as u8
}

// ── Runners (inlined into each animation for simplicity) ─────────────

/// Effect runner i: iterates LEDs, calls fn(hsv, i, time) -> hsv
fn runner_i(rgb: &mut RgbMatrix, effect: fn(u8, u8) -> u8) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let hue = effect(i as u8, time);
        let (r, g, b) = hsv_to_rgb(hue.wrapping_add(rgb.hue), sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// Effect runner dx_dy: iterates LEDs, calls fn(hsv, dx, dy, time) -> hsv
fn runner_dx_dy(rgb: &mut RgbMatrix, effect: fn(i16, i16, u8) -> u8) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let hue = effect(dx, dy, time);
        let (r, g, b) = hsv_to_rgb(hue.wrapping_add(rgb.hue), sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// Effect runner dx_dy_dist: iterates LEDs, calls fn(hsv, dx, dy, dist, time) -> hsv
fn runner_dx_dy_dist(rgb: &mut RgbMatrix, effect: fn(i16, i16, u8, u8) -> u8) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        let hue = effect(dx, dy, dist, time);
        let (r, g, b) = hsv_to_rgb(hue.wrapping_add(rgb.hue), sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// Effect runner sin_cos_i: calls fn(hsv, sin, cos, i, time) -> hsv
fn runner_sin_cos_i(rgb: &mut RgbMatrix, effect: fn(i8, i8, u8, u8) -> u8) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let cos_value: i8 = (cos8(time) as i16 - 128) as i8;
    let sin_value: i8 = (sin8(time) as i16 - 128) as i8;
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let hue = effect(sin_value, cos_value, i as u8, time);
        let (r, g, b) = hsv_to_rgb(hue.wrapping_add(rgb.hue), sat, val);
        rgb.set_color(i, r, g, b);
    }
}

/// Reactive runner: uses hit_tracker to find recent key hits
fn runner_reactive(rgb: &mut RgbMatrix, effect: fn(u16) -> (u8, u8)) {
    let max_tick = 65535u32 / qadd8(rgb.speed, 1) as u32;
    for i in 0..LED_COUNT {
        let mut tick = max_tick;
        let current = rgb.anim_tick;
        // Reverse search for most recent hit at this LED
        for j in (0..rgb.hit_count).rev() {
            if j as usize >= rgb.hit_index.len() { break; }
            if rgb.hit_index[j as usize] == i as u8 {
                let e = current.wrapping_sub(rgb.hit_tick[j as usize]);
                if e < tick { tick = e; }
                break;
            }
        }
        let offset = scale16by8(tick as u16, qadd8(rgb.speed, 1));
        let (h, v) = effect(offset);
        let (r, g, b) = hsv_to_rgb(h, rgb.sat, v);
        rgb.set_color(i, r, g, b);
    }
}

/// Reactive splash runner: uses spatial distance from hit positions
fn runner_reactive_splash(rgb: &mut RgbMatrix, start: u8, effect: fn(i16, i16, u8, u16) -> (u8, u8)) {
    let count = rgb.hit_count;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let (h, v) = effect(dx, dy, dist, tick);
            hsv_h = h;
            hsv_v = qadd8(hsv_v, v);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Animation render functions ───────────────────────────────────────

// ── Mode 1: BREATHING ───────────────────────────────────────────────
pub fn render_breathing(rgb: &mut RgbMatrix) {
    let hsv_h = rgb.hue;
    let hsv_s = rgb.sat;
    let hsv_v_base = rgb.val;
    let time = compute_time(rgb.anim_tick, rgb.speed, 8);
    let raw = sin8(time);
    let diff: i8 = (raw as i16 - 128i16) as i8;
    let v_scaled = abs8(diff).wrapping_mul(2);
    let v = scale8(v_scaled, hsv_v_base);
    let (r, g, b) = hsv_to_rgb(hsv_h, hsv_s, v);
    rgb.set_all(r, g, b);
}

// ── Mode 2: CYCLE_ALL ───────────────────────────────────────────────
pub fn render_cycle_all(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let (r, g, b) = hsv_to_rgb(time, rgb.sat, rgb.val);
    rgb.set_all(r, g, b);
}

// ── Mode 3: CYCLE_UP_DOWN ───────────────────────────────────────────
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

// ── Mode 4: CYCLE_LEFT_RIGHT ────────────────────────────────────────
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

// ── Mode 5: CYCLE_OUT_IN ────────────────────────────────────────────
pub fn render_cycle_out_in(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 2);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        let hue = ((3u16 * dist as u16) >> 1).wrapping_add(time as u16);
        let (r, g, b) = hsv_to_rgb(hue as u8, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 6: GRADIENT_UP_DOWN ────────────────────────────────────────
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

// ── Mode 7: GRADIENT_LEFT_RIGHT ─────────────────────────────────────
pub fn render_gradient_left_right(rgb: &mut RgbMatrix) {
    let scale = scale8(64, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let hue = rgb.hue.wrapping_add(((scale as u16 * px as u16) >> 5) as u8);
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 8: RAINBOW_MOVING_CHEVRON ──────────────────────────────────
pub fn render_rainbow_moving_chevron(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dy = py.abs_diff(CENTER_Y);
        let hue = rgb.hue.wrapping_add(dy).wrapping_add(px.wrapping_sub(time));
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 9: ALPHAS_MODS ─────────────────────────────────────────────
// Alphas = current color, mods = hue shifted by speed
pub fn render_alphas_mods(rgb: &mut RgbMatrix) {
    let (r1, g1, b1) = hsv_to_rgb(rgb.hue, rgb.sat, rgb.val);
    let h2 = rgb.hue.wrapping_add(rgb.speed);
    let (r2, g2, b2) = hsv_to_rgb(h2, rgb.sat, rgb.val);
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        // Modifier keys are typically at the edges (x=0, x=max)
        let is_mod = px <= 15 || px >= 170 || py >= 45;
        if is_mod {
            rgb.set_color(i, r2, g2, b2);
        } else {
            rgb.set_color(i, r1, g1, b1);
        }
    }
}

// ── Mode 10: BAND_PINWHEEL_SAT ──────────────────────────────────────
pub fn render_band_pinwheel_sat(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let s_raw = rgb.sat.wrapping_sub(time)
            .wrapping_sub(atan2_8(dy, dx).wrapping_mul(3));
        let sat = scale8(s_raw, rgb.sat);
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 11: BAND_PINWHEEL_VAL ──────────────────────────────────────
pub fn render_band_pinwheel_val(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let v_raw = rgb.val.wrapping_sub(time)
            .wrapping_sub(atan2_8(dy, dx).wrapping_mul(3));
        let val = scale8(v_raw, rgb.val);
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 12: BAND_SAT ───────────────────────────────────────────────
// Port of colorband_sat_anim.h — uses i16 for s to allow negative clamping
pub fn render_band_sat(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let s_mid = scale8(px, 228).wrapping_add(28);
        let dist = s_mid.abs_diff(time);
        // QMK: int16_t s = hsv.s - abs(scale8(x,228)+28-time)*8; hsv.s = scale8(s<0?0:s, hsv.s)
        let s = (rgb.sat as i16) - (dist as i16 * 8);
        let sat = if s < 0 { 0 } else { scale8(s as u8, rgb.sat) };
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 13: BAND_SPIRAL_SAT ────────────────────────────────────────
pub fn render_band_spiral_sat(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        let s_raw = rgb.sat.wrapping_add(dist)
            .wrapping_sub(time)
            .wrapping_sub(atan2_8(dy, dx));
        let sat = scale8(s_raw, rgb.sat);
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 14: BAND_SPIRAL_VAL ────────────────────────────────────────
pub fn render_band_spiral_val(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        let v_raw = rgb.val.wrapping_add(dist)
            .wrapping_sub(time)
            .wrapping_sub(atan2_8(dy, dx));
        let val = scale8(v_raw, rgb.val);
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 15: BAND_VAL ───────────────────────────────────────────────
// Port of colorband_val_anim.h — uses i16 for v to allow negative clamping
pub fn render_band_val(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let v_mid = scale8(px, 228).wrapping_add(28);
        let dist = v_mid.abs_diff(time);
        // QMK: int16_t v = hsv.v - abs(scale8(x,228)+28-time)*8; hsv.v = scale8(v<0?0:v, hsv.v)
        let v = (rgb.val as i16) - (dist as i16 * 8);
        let val = if v < 0 { 0 } else { scale8(v as u8, rgb.val) };
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 16: CYCLE_OUT_IN_DUAL ──────────────────────────────────────
pub fn render_cycle_out_in_dual(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    let half_center_x = (CENTER_X as i16) / 2;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = half_center_x - abs8((px as i16 - CENTER_X as i16) as i8) as i16;
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        let hue = (3u16 * dist as u16).wrapping_add(time as u16);
        let (r, g, b) = hsv_to_rgb(hue as u8, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 17: CYCLE_PINWHEEL ─────────────────────────────────────────
pub fn render_cycle_pinwheel(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let hue = atan2_8(dy, dx).wrapping_add(time);
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 18: CYCLE_SPIRAL ───────────────────────────────────────────
pub fn render_cycle_spiral(rgb: &mut RgbMatrix) {
    let time = compute_time_speed_div(rgb.anim_tick, rgb.speed);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let dist = sqrt16((dx * dx + dy * dy) as u16);
        let hue = dist.wrapping_sub(time).wrapping_sub(atan2_8(dy, dx));
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 19: DIGITAL_RAIN ───────────────────────────────────────────
const DIGITAL_RAIN_DROPS: u8 = 24;

pub fn render_digital_rain(rgb: &mut RgbMatrix) {
    let drop_ticks: u8 = 28;
    let pure_green_intensity = ((rgb.val as u16 * 3) >> 2) as u8;
    let max_brightness_boost = ((rgb.val as u16 * 3) >> 2) as u8;
    let max_intensity = rgb.val;
    let decay_ticks = if max_intensity > 0 { 0xff / max_intensity } else { 1 };

    rgb.digital_rain_drop = rgb.digital_rain_drop.wrapping_add(1);
    rgb.digital_rain_decay = rgb.digital_rain_decay.wrapping_add(1);

    for col in 0..MATRIX_COLS {
        for row in 0..MATRIX_ROWS {
            if row == 0 && rgb.digital_rain_drop == 0 {
                if random8() < (255u16 / DIGITAL_RAIN_DROPS as u16) as u8 {
                    rgb.frame_buffer[row][col] = max_intensity;
                }
            } else if rgb.frame_buffer[row][col] > 0 && rgb.frame_buffer[row][col] < max_intensity
                && rgb.digital_rain_decay == decay_ticks {
                    rgb.frame_buffer[row][col] = rgb.frame_buffer[row][col].wrapping_sub(1);
                }

            let fb_val = rgb.frame_buffer[row][col];
            let mut led_buf = [NO_LED; 8];
            let led_count = map_row_column_to_led(row as u8, col as u8, &mut led_buf);
            if led_count > 0 {
                let led_idx = led_buf[0] as usize;
                if led_idx < LED_COUNT {
                    if fb_val > pure_green_intensity {
                        let boost = ((max_brightness_boost as u16
                            * (fb_val - pure_green_intensity) as u16)
                            / (max_intensity - pure_green_intensity).max(1) as u16) as u8;
                        rgb.set_color(led_idx, boost, max_intensity, boost);
                    } else {
                        let green = if pure_green_intensity > 0 {
                            ((max_intensity as u16 * fb_val as u16)
                                / pure_green_intensity as u16) as u8
                        } else { 0 };
                        rgb.set_color(led_idx, 0, green, 0);
                    }
                }
            }
        }
    }

    if rgb.digital_rain_decay == decay_ticks {
        rgb.digital_rain_decay = 0;
    }

    if rgb.digital_rain_drop > drop_ticks {
        rgb.digital_rain_drop = 0;
        for col in 0..MATRIX_COLS {
            for row in (1..MATRIX_ROWS).rev() {
                if row == MATRIX_ROWS - 1 && rgb.frame_buffer[row][col] == max_intensity {
                    rgb.frame_buffer[row][col] = rgb.frame_buffer[row][col].wrapping_sub(1);
                }
                if rgb.frame_buffer[row - 1][col] >= max_intensity {
                    rgb.frame_buffer[row - 1][col] = max_intensity.wrapping_sub(1);
                    rgb.frame_buffer[row][col] = max_intensity;
                }
            }
        }
    }
}

// ── Mode 20: DUAL_BEACON ────────────────────────────────────────────
pub fn render_dual_beacon(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let cos_val: i8 = (cos8(time) as i16 - 128) as i8;
    let sin_val: i8 = (sin8(time) as i16 - 128) as i8;
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let delta = ((dy * cos_val as i16 + dx * sin_val as i16) / 128) as i8;
        let hue = (rgb.hue as i16 + delta as i16) as u8;
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 21: FLOWER_BLOOMING ────────────────────────────────────────
pub fn render_flower_blooming(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 10);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let hue = if py > CENTER_Y {
            (px as u16 * 3).wrapping_sub(py as u16 * 3).wrapping_add(time as u16)
        } else {
            (px as u16 * 3).wrapping_sub(py as u16 * 3).wrapping_sub(time as u16)
        };
        let (r, g, b) = hsv_to_rgb(hue as u8, sat, val);
        // QMK flower blooming swaps R/B for lower half (BGR), but we output RGB
        // For authenticity: if py > CENTER_Y, swap R and B
        if py > CENTER_Y {
            rgb.set_color(i, b, g, r);
        } else {
            rgb.set_color(i, r, g, b);
        }
    }
}

// ── Mode 22: HUE_BREATHING ──────────────────────────────────────────
pub fn render_hue_breathing(rgb: &mut RgbMatrix) {
    let huedelta: u8 = 12;
    let time = compute_time(rgb.anim_tick, rgb.speed, 8);
    let raw = sin8(time);
    let diff: i8 = (raw as i16 - 128i16) as i8;
    let hue = rgb.hue.wrapping_add(scale8(abs8(diff).wrapping_mul(2), huedelta));
    let (r, g, b) = hsv_to_rgb(hue, rgb.sat, rgb.val);
    rgb.set_all(r, g, b);
}

// ── Mode 23: HUE_PENDULUM ───────────────────────────────────────────
pub fn render_hue_pendulum(rgb: &mut RgbMatrix) {
    let huedelta: u8 = 12;
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let combined = sin8(time).wrapping_add(px);
        let diff: i8 = (combined as i16 - 128i16) as i8;
        let hue = rgb.hue.wrapping_add(scale8(abs8(diff).wrapping_mul(2), huedelta));
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 24: HUE_WAVE ───────────────────────────────────────────────
pub fn render_hue_wave(rgb: &mut RgbMatrix) {
    let huedelta: u8 = 24;
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, _) = LED_POSITIONS[i];
        let diff = px.abs_diff(time);
        let hue = rgb.hue.wrapping_add(scale8(diff, huedelta));
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 25: JELLYBEAN_RAINDROPS ────────────────────────────────────
pub fn render_jellybean_raindrops(rgb: &mut RgbMatrix) {
    let scaled = scale16by8(rgb.anim_tick as u16, qadd8(rgb.speed, 16));
    if scaled.is_multiple_of(5) {
        let idx = random8_max(LED_COUNT as u8) as usize;
        let h = random8();
        let s = random8_min_max(127, 255);
        let (r, g, b) = hsv_to_rgb(h, s, rgb.val);
        rgb.set_color(idx, r, g, b);
    }
}

// ── Mode 26: PIXEL_FLOW ─────────────────────────────────────────────
pub fn render_pixel_flow(rgb: &mut RgbMatrix) {
    let interval = 3000u32 / scale16by8(qadd8(rgb.speed, 16) as u16, 16) as u32;
    if rgb.anim_tick < rgb.pixel_flow_wait_timer {
        return;
    }

    // Shift LED state forward
    for j in 0..LED_COUNT - 1 {
        rgb.pixel_flow_state[j] = rgb.pixel_flow_state[j + 1];
    }

    // Fill last LED
    if random8() & 2 != 0 {
        rgb.pixel_flow_state[LED_COUNT - 1] = (0, 0, 0);
    } else {
        let h = random8();
        let s = random8_min_max(127, 255);
        let (r, g, b) = hsv_to_rgb(h, s, rgb.val);
        rgb.pixel_flow_state[LED_COUNT - 1] = (r, g, b);
    }

    // Light LEDs
    for i in 0..LED_COUNT {
        let (r, g, b) = rgb.pixel_flow_state[i];
        rgb.set_color(i, r, g, b);
    }

    rgb.pixel_flow_wait_timer = rgb.anim_tick.wrapping_add(interval);
}

// ── Mode 27: PIXEL_FRACTAL ──────────────────────────────────────────
const MID_COL: usize = MATRIX_COLS / 2;

pub fn render_pixel_fractal(rgb: &mut RgbMatrix) {
    let interval = 3000u32 / scale16by8(qadd8(rgb.speed, 16) as u16, 16) as u32;
    if rgb.anim_tick <= rgb.pixel_fractal_timer {
        return;
    }

    let (r, g, b) = hsv_to_rgb(rgb.hue, rgb.sat, rgb.val);

    for h in 0..MATRIX_ROWS {
        // Shift and light columns outward
        for l in 0..MID_COL - 1 {
            if rgb.pixel_fractal_state[h][l] {
                let led_l = matrix_co_ordered(h, l);
                let led_r = matrix_co_ordered(h, MATRIX_COLS - 1 - l);
                if led_l != NO_LED { rgb.set_color(led_l as usize, r, g, b); }
                if led_r != NO_LED { rgb.set_color(led_r as usize, r, g, b); }
            } else {
                let led_l = matrix_co_ordered(h, l);
                let led_r = matrix_co_ordered(h, MATRIX_COLS - 1 - l);
                if led_l != NO_LED { rgb.set_color(led_l as usize, 0, 0, 0); }
                if led_r != NO_LED { rgb.set_color(led_r as usize, 0, 0, 0); }
            }
            rgb.pixel_fractal_state[h][l] = rgb.pixel_fractal_state[h][l + 1];
        }

        // Light middle columns
        let mid_c = MID_COL - 1;
        let led_mid_l = matrix_co_ordered(h, mid_c);
        let led_mid_r = matrix_co_ordered(h, MATRIX_COLS - 1 - mid_c);
        if rgb.pixel_fractal_state[h][mid_c] {
            if led_mid_l != NO_LED { rgb.set_color(led_mid_l as usize, r, g, b); }
            if led_mid_r != NO_LED { rgb.set_color(led_mid_r as usize, r, g, b); }
        } else {
            if led_mid_l != NO_LED { rgb.set_color(led_mid_l as usize, 0, 0, 0); }
            if led_mid_r != NO_LED { rgb.set_color(led_mid_r as usize, 0, 0, 0); }
        }

        // Generate new random fractal column
        rgb.pixel_fractal_state[h][mid_c] = (random8() & 3) == 0;
    }

    rgb.pixel_fractal_timer = rgb.anim_tick.wrapping_add(interval);
}

// ── Mode 28: PIXEL_RAIN ─────────────────────────────────────────────
pub fn render_pixel_rain(rgb: &mut RgbMatrix) {
    let interval = 500u32 / scale16by8(qadd8(rgb.speed, 16) as u16, 16) as u32;
    if rgb.anim_tick <= rgb.pixel_rain_timer {
        return;
    }
    let idx = random8_max(LED_COUNT as u8) as usize;
    if random8() & 2 != 0 {
        rgb.set_color(idx, 0, 0, 0);
    } else {
        let h = random8();
        let s = random8_min_max(127, 255);
        let (r, g, b) = hsv_to_rgb(h, s, rgb.val);
        rgb.set_color(idx, r, g, b);
    }
    rgb.pixel_rain_timer = rgb.anim_tick.wrapping_add(interval);
}

// ── Mode 29: RAINBOW_BEACON ─────────────────────────────────────────
pub fn render_rainbow_beacon(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let cos_v: i8 = (cos8(time) as i16 - 128) as i8;
    let sin_v: i8 = (sin8(time) as i16 - 128) as i8;
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dx = (px as i16) - (CENTER_X as i16);
        let dy = (py as i16) - (CENTER_Y as i16);
        let delta = ((dy * 2 * cos_v as i16 + dx * 2 * sin_v as i16) / 128) as i8;
        let hue = (rgb.hue as i16 + delta as i16) as u8;
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 30: RAINBOW_PINWHEELS ──────────────────────────────────────
pub fn render_rainbow_pinwheels(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 4);
    let cos_v: i8 = (cos8(time) as i16 - 128) as i8;
    let sin_v: i8 = (sin8(time) as i16 - 128) as i8;
    let sat = rgb.sat;
    let val = rgb.val;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let dy = (py as i16) - (CENTER_Y as i16);
        let dx_abs = px.abs_diff(CENTER_X);
        let delta = ((dy * 3 * cos_v as i16 + (56 - dx_abs as i16) * 3 * sin_v as i16) / 128) as i8;
        let hue = (rgb.hue as i16 + delta as i16) as u8;
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 31: RAINDROPS ──────────────────────────────────────────────
pub fn render_raindrops(rgb: &mut RgbMatrix) {
    let scaled = scale16by8(rgb.anim_tick as u16, qadd8(rgb.speed, 16));
    if scaled.is_multiple_of(10) {
        let idx = random8_max(LED_COUNT as u8) as usize;
        let delta_h: i8 = ((rgb.hue.wrapping_add(180).wrapping_mul(4)) as i16 / 4) as i8;
        let h = rgb.hue.wrapping_add((delta_h * (random8() as i8 & 0x03)) as u8);
        let (r, g, b) = hsv_to_rgb(h, rgb.sat, rgb.val);
        rgb.set_color(idx, r, g, b);
    }
}

// ── Mode 32: RIVERFLOW ──────────────────────────────────────────────
pub fn render_riverflow(rgb: &mut RgbMatrix) {
    let sat = rgb.sat;
    for i in 0..LED_COUNT {
        let timed = rgb.anim_tick.wrapping_add((i * 315) as u32);
        let t8 = scale16by8(timed as u16, rgb.speed.wrapping_div(8));
        let raw = sin8(t8 as u8);
        let diff: i8 = (raw as i16 - 128i16) as i8;
        let v = scale8(abs8(diff).wrapping_mul(2), rgb.val);
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 33: SOLID_REACTIVE ─────────────────────────────────────────
pub fn render_solid_reactive(rgb: &mut RgbMatrix) {
    let max_tick = 65535u32 / qadd8(rgb.speed, 1) as u32;
    let sat = rgb.sat;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let mut tick = max_tick;
        for j in (0..rgb.hit_count).rev() {
            if j as usize >= rgb.hit_index.len() { break; }
            if rgb.hit_index[j as usize] == i as u8 {
                let e = current.wrapping_sub(rgb.hit_tick[j as usize]);
                if e < tick { tick = e; }
                break;
            }
        }
        let offset = scale16by8(tick as u16, qadd8(rgb.speed, 1));
        let hue = rgb.hue.wrapping_add(scale8(255u8.wrapping_sub(offset as u8), 64));
        let val = rgb.val;
        let (r, g, b) = hsv_to_rgb(hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 34: SOLID_REACTIVE_SIMPLE ──────────────────────────────────
pub fn render_solid_reactive_simple(rgb: &mut RgbMatrix) {
    let max_tick = 65535u32 / qadd8(rgb.speed, 1) as u32;
    let sat = rgb.sat;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let mut tick = max_tick;
        for j in (0..rgb.hit_count).rev() {
            if j as usize >= rgb.hit_index.len() { break; }
            if rgb.hit_index[j as usize] == i as u8 {
                let e = current.wrapping_sub(rgb.hit_tick[j as usize]);
                if e < tick { tick = e; }
                break;
            }
        }
        let offset = scale16by8(tick as u16, qadd8(rgb.speed, 1));
        let val = scale8(255u8.wrapping_sub(offset as u8), rgb.val);
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, val);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 35: SPLASH ─────────────────────────────────────────────────
pub fn render_splash(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let start = qsub8(count, 1);
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            hsv_h = hsv_h.wrapping_add(effect as u8);
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 36: MULTISPLASH ────────────────────────────────────────────
pub fn render_multisplash(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in 0..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            hsv_h = hsv_h.wrapping_add(effect as u8);
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 37: SOLID_SPLASH ───────────────────────────────────────────
pub fn render_solid_splash(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let start = qsub8(count, 1);
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_v: u8 = 0;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(rgb.hue, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 38: SOLID_MULTISPLASH ──────────────────────────────────────
pub fn render_solid_multisplash(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_v: u8 = 0;
        for j in 0..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(rgb.hue, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 39: SOLID_REACTIVE_CROSS ───────────────────────────────────
pub fn fn_solid_reactive_cross_math(dx: i16, dy: i16, dist: u8, tick: u16) -> (u8, u8) {
    let mut effect = tick.wrapping_add(dist as u16);
    let abs_dx: i16 = if dx < 0 { -dx } else { dx };
    let abs_dy: i16 = if dy < 0 { -dy } else { dy };
    let dx_clamped = if abs_dx * 16 > 255 { 255 } else { abs_dx * 16 } as u16;
    let dy_clamped = if abs_dy * 16 > 255 { 255 } else { abs_dy * 16 } as u16;
    effect = effect.wrapping_add(if dx_clamped > dy_clamped { dy_clamped } else { dx_clamped });
    if effect > 255 { effect = 255; }
    (0, 255u16.wrapping_sub(effect) as u8)
}

pub fn render_solid_reactive_cross(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let start = qsub8(count, 1);
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let (_h, v) = fn_solid_reactive_cross_math(dx, dy, dist, tick);
            hsv_v = qadd8(hsv_v, v);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 40: SOLID_REACTIVE_MULTICROSS ──────────────────────────────
pub fn render_solid_reactive_multicross(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in 0..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let (_h, v) = fn_solid_reactive_cross_math(dx, dy, dist, tick);
            hsv_v = qadd8(hsv_v, v);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 41: SOLID_REACTIVE_NEXUS ───────────────────────────────────
pub fn render_solid_reactive_nexus(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let start = qsub8(count, 1);
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_v: u8 = 0;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            if dist > 72 { effect = 255; }
            if !(-8..=8).contains(&dx) && !(-8..=8).contains(&dy) { effect = 255; }
            let _hue = rgb.hue.wrapping_add((dy / 4) as u8);
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
            // Use last processed hue
            rgb.set_color(i, 0, 0, 0); // Placeholder - will be set below
        }
        hsv_v = scale8(hsv_v, rgb.val);
        // Compute average hue from hits
        let hue = if count > start {
            let j = (count - 1) as usize;
            let hy = rgb.hit_y.get(j).copied().unwrap_or(CENTER_Y);
            rgb.hue.wrapping_add((hy as i16 - CENTER_Y as i16).wrapping_div(4) as u8)
        } else { rgb.hue };
        let (r, g, b) = hsv_to_rgb(hue, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}
// Simplified nexus re-do:
pub fn render_solid_reactive_nexus_v2(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let start = qsub8(count, 1);
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        let mut any_hit = false;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            if dist > 72 { effect = 255; }
            if !(-8..=8).contains(&dx) && !(-8..=8).contains(&dy) { effect = 255; }
            hsv_h = rgb.hue.wrapping_add((dy / 4) as u8);
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
            any_hit = true;
        }
        if !any_hit {
            hsv_h = rgb.hue;
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 42: SOLID_REACTIVE_MULTINEXUS ──────────────────────────────
pub fn render_solid_reactive_multinexus(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let mut hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        let mut any_hit = false;
        for j in 0..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_sub(dist as u16);
            if effect > 255 { effect = 255; }
            if dist > 72 { effect = 255; }
            if !(-8..=8).contains(&dx) && !(-8..=8).contains(&dy) { effect = 255; }
            hsv_h = rgb.hue.wrapping_add((dy / 4) as u8);
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
            any_hit = true;
        }
        if !any_hit {
            hsv_h = rgb.hue;
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 43: SOLID_REACTIVE_WIDE ────────────────────────────────────
pub fn render_solid_reactive_wide(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let start = qsub8(count, 1);
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in start..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_add(dist as u16 * 5);
            if effect > 255 { effect = 255; }
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 44: SOLID_REACTIVE_MULTIWIDE ───────────────────────────────
pub fn render_solid_reactive_multiwide(rgb: &mut RgbMatrix) {
    let count = rgb.hit_count;
    let current = rgb.anim_tick;
    for i in 0..LED_COUNT {
        let (px, py) = LED_POSITIONS[i];
        let hsv_h = rgb.hue;
        let mut hsv_v: u8 = 0;
        for j in 0..count {
            if j as usize >= rgb.hit_index.len() { break; }
            let hx = rgb.hit_x[j as usize] as i16;
            let hy = rgb.hit_y[j as usize] as i16;
            let dx = (px as i16) - hx;
            let dy = (py as i16) - hy;
            let dist = sqrt16((dx * dx + dy * dy) as u16);
            let elapsed = current.wrapping_sub(rgb.hit_tick[j as usize]);
            let tick = scale16by8(elapsed as u16, qadd8(rgb.speed, 1));
            let mut effect = tick.wrapping_add(dist as u16 * 5);
            if effect > 255 { effect = 255; }
            hsv_v = qadd8(hsv_v, 255u16.wrapping_sub(effect) as u8);
        }
        hsv_v = scale8(hsv_v, rgb.val);
        let (r, g, b) = hsv_to_rgb(hsv_h, rgb.sat, hsv_v);
        rgb.set_color(i, r, g, b);
    }
}

// ── Mode 45: STARLIGHT ──────────────────────────────────────────────
pub fn render_starlight(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 8);
    let raw = sin8(time);
    let diff: i8 = (raw as i16 - 128i16) as i8;
    let v = scale8(abs8(diff).wrapping_mul(2), rgb.val);
    let (r, g, b) = hsv_to_rgb(rgb.hue, rgb.sat, v);
    if scale16by8(rgb.anim_tick as u16, qadd8(rgb.speed, 5)).is_multiple_of(5) {
        let idx = random8_max(LED_COUNT as u8) as usize;
        rgb.set_color(idx, r, g, b);
    }
}

// ── Mode 46: STARLIGHT_DUAL_HUE ─────────────────────────────────────
pub fn render_starlight_dual_hue(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 8);
    let raw = sin8(time);
    let diff: i8 = (raw as i16 - 128i16) as i8;
    let v = scale8(abs8(diff).wrapping_mul(2), rgb.val);
    if scale16by8(rgb.anim_tick as u16, qadd8(rgb.speed, 5)).is_multiple_of(5) {
        let idx = random8_max(LED_COUNT as u8) as usize;
        // rand() % (30+1 - -30) + -30 = random in [-30, 30]
        let offset: i8 = (random8() % 61) as i8 - 30;
        let hue = (rgb.hue as i16 + offset as i16) as u8;
        let (r, g, b) = hsv_to_rgb(hue, rgb.sat, v);
        rgb.set_color(idx, r, g, b);
    }
}

// ── Mode 47: STARLIGHT_DUAL_SAT ─────────────────────────────────────
pub fn render_starlight_dual_sat(rgb: &mut RgbMatrix) {
    let time = compute_time(rgb.anim_tick, rgb.speed, 8);
    let raw = sin8(time);
    let diff: i8 = (raw as i16 - 128i16) as i8;
    let v = scale8(abs8(diff).wrapping_mul(2), rgb.val);
    if scale16by8(rgb.anim_tick as u16, qadd8(rgb.speed, 5)).is_multiple_of(5) {
        let idx = random8_max(LED_COUNT as u8) as usize;
        let offset: i8 = (random8() % 61) as i8 - 30;
        let sat = (rgb.sat as i16 + offset as i16).clamp(0, 255) as u8;
        let (r, g, b) = hsv_to_rgb(rgb.hue, sat, v);
        rgb.set_color(idx, r, g, b);
    }
}

// ── Mode 48: TYPING_HEATMAP ─────────────────────────────────────────
const HEATMAP_INCREASE_STEP: u8 = 32;
const HEATMAP_DECREASE_DELAY_MS: u16 = 25;
const HEATMAP_SPREAD: u8 = 40;
const HEATMAP_AREA_LIMIT: u8 = 16;

/// Called when a key is pressed to update the heatmap frame buffer
pub fn typing_heatmap_keypress(rgb: &mut RgbMatrix, row: u8, col: u8) {
    let led_idx = matrix_co_ordered(row as usize, col as usize);
    if led_idx == NO_LED { return; }

    let (tx, ty) = LED_POSITIONS[led_idx as usize];

    for i_row in 0..MATRIX_ROWS {
        for i_col in 0..MATRIX_COLS {
            let target_led = matrix_co_ordered(i_row, i_col);
            if target_led == NO_LED { continue; }
            let (lx, ly) = LED_POSITIONS[target_led as usize];

            if target_led == led_idx {
                rgb.frame_buffer[i_row][i_col] = qadd8(rgb.frame_buffer[i_row][i_col], HEATMAP_INCREASE_STEP);
            } else {
                let dx = (lx as i16 - tx as i16);
                let dy = (ly as i16 - ty as i16);
                let distance = sqrt16((dx * dx + dy * dy) as u16);
                if distance <= HEATMAP_SPREAD {
                    let mut amount = qsub8(HEATMAP_SPREAD, distance);
                    if amount > HEATMAP_AREA_LIMIT {
                        amount = HEATMAP_AREA_LIMIT;
                    }
                    rgb.frame_buffer[i_row][i_col] = qadd8(rgb.frame_buffer[i_row][i_col], amount);
                }
            }
        }
    }
}

pub fn render_typing_heatmap(rgb: &mut RgbMatrix) {
    // Decrease timer logic: decrease every HEATMAP_DECREASE_DELAY_MS ms
    if rgb.heatmap_decrease_timer >= HEATMAP_DECREASE_DELAY_MS {
        rgb.decrease_heatmap = true;
        rgb.heatmap_decrease_timer = 0;
    } else {
        rgb.heatmap_decrease_timer = rgb.heatmap_decrease_timer.wrapping_add(1);
    }

    for row in 0..MATRIX_ROWS {
        for col in 0..MATRIX_COLS {
            let val = rgb.frame_buffer[row][col];
            let led_idx = matrix_co_ordered(row, col);
            if led_idx == NO_LED { continue; }

            // Map heat value to HSV: hue from blue(170) to red(0), val scales up
            let hue = 170u8.wrapping_sub(qsub8(val, 85));
            let heat_val = qadd8(170, val).wrapping_sub(170).wrapping_mul(3);
            let v = scale8(heat_val, rgb.val);
            let (r, g, b) = hsv_to_rgb(hue, rgb.sat, v);
            rgb.set_color(led_idx as usize, r, g, b);

            if rgb.decrease_heatmap {
                rgb.frame_buffer[row][col] = qsub8(val, 1);
            }
        }
    }
    rgb.decrease_heatmap = false;
}

// ── Mode 49: SOLID_COLOR ────────────────────────────────────────────
pub fn render_solid_color(rgb: &mut RgbMatrix) {
    let (r, g, b) = hsv_to_rgb(rgb.hue, rgb.sat, rgb.val);
    rgb.set_all(r, g, b);
}

// ── Total animation count (mode 0 = manual, 1-49 = animations) ──────
pub const ANIMATION_COUNT: u8 = 50;

/// Dispatch an animation tick to the current mode.
pub fn tick_animation(rgb: &mut RgbMatrix) {
    if rgb.mode == 0 {
        return; // Solid — manual set_hsv controls color
    }

    match rgb.mode {
        1 => render_breathing(rgb),
        2 => render_cycle_all(rgb),
        3 => render_cycle_up_down(rgb),
        4 => render_cycle_left_right(rgb),
        5 => render_cycle_out_in(rgb),
        6 => render_gradient_up_down(rgb),
        7 => render_gradient_left_right(rgb),
        8 => render_rainbow_moving_chevron(rgb),
        9 => render_alphas_mods(rgb),
        10 => render_band_pinwheel_sat(rgb),
        11 => render_band_pinwheel_val(rgb),
        12 => render_band_sat(rgb),
        13 => render_band_spiral_sat(rgb),
        14 => render_band_spiral_val(rgb),
        15 => render_band_val(rgb),
        16 => render_cycle_out_in_dual(rgb),
        17 => render_cycle_pinwheel(rgb),
        18 => render_cycle_spiral(rgb),
        19 => render_digital_rain(rgb),
        20 => render_dual_beacon(rgb),
        21 => render_flower_blooming(rgb),
        22 => render_hue_breathing(rgb),
        23 => render_hue_pendulum(rgb),
        24 => render_hue_wave(rgb),
        25 => render_jellybean_raindrops(rgb),
        26 => render_pixel_flow(rgb),
        27 => render_pixel_fractal(rgb),
        28 => render_pixel_rain(rgb),
        29 => render_rainbow_beacon(rgb),
        30 => render_rainbow_pinwheels(rgb),
        31 => render_raindrops(rgb),
        32 => render_riverflow(rgb),
        33 => render_solid_reactive(rgb),
        34 => render_solid_reactive_simple(rgb),
        35 => render_splash(rgb),
        36 => render_multisplash(rgb),
        37 => render_solid_splash(rgb),
        38 => render_solid_multisplash(rgb),
        39 => render_solid_reactive_cross(rgb),
        40 => render_solid_reactive_multicross(rgb),
        41 => render_solid_reactive_nexus_v2(rgb),
        42 => render_solid_reactive_multinexus(rgb),
        43 => render_solid_reactive_wide(rgb),
        44 => render_solid_reactive_multiwide(rgb),
        45 => render_starlight(rgb),
        46 => render_starlight_dual_hue(rgb),
        47 => render_starlight_dual_sat(rgb),
        48 => render_typing_heatmap(rgb),
        49 => render_solid_color(rgb),
        _ => render_cycle_left_right(rgb), // fallback
    }
}
