//! RGB matrix — 1:1 port of `quantum/rgb_matrix/rgb_matrix.c`, the effect
//! runners and the 40 effects enabled in keyboard.json, plus the lib8tion
//! math they use, as compiled for the C firmware (Cortex-M0: C lib8tion,
//! FASTLED_SCALE8_FIXED off, no CIE curve, newlib `rand()`).
//!
//! Verified frame-for-frame against QMK's own code by tools/rgb_harness.
//! Integer conversions mirror C: `as u8` / `as i8` truncate like C's implicit
//! conversions, `i32` stands in for C's int promotion.

use super::led_config::{FLAGS, MATRIX_CO, NO_LED, POINT};
use super::rgb::{hsv_to_rgb, RgbMatrix, LED_COUNT};

pub const LED_FLAG_ALL: u8 = 0xFF;
pub const RGB_MATRIX_EFFECT_MAX: u8 = 41;
const LED_HITS_TO_REMEMBER: usize = 8;
const RGB_MATRIX_LED_PROCESS_LIMIT: u8 = ((LED_COUNT + 4) / 5) as u8;
const RGB_MATRIX_LED_FLUSH_LIMIT: u32 = 16;
const RGB_MATRIX_MAXIMUM_BRIGHTNESS: u8 = 255;
/// `RGB_MATRIX_CENTER` {90, 25}.
const CENTER_X: i32 = 90;
const CENTER_Y: i32 = 25;
const MATRIX_ROWS: usize = 6;
const MATRIX_COLS: usize = 21;

// enum rgb_matrix_effects (enabled set, rgb_matrix_effects.inc order)
pub const RGB_MATRIX_NONE: u8 = 0;
pub const RGB_MATRIX_TYPING_HEATMAP: u8 = 27;

// ── lib8tion ─────────────────────────────────────────────────────────────

pub fn scale8(i: u8, scale: u8) -> u8 {
    ((i as u16 * scale as u16) >> 8) as u8
}
pub fn scale16by8(i: u16, scale: u8) -> u16 {
    ((i as u32 * scale as u32) / 256) as u16
}
pub fn qadd8(i: u8, j: u8) -> u8 {
    i.saturating_add(j)
}
pub fn qsub8(i: u8, j: u8) -> u8 {
    i.saturating_sub(j)
}
pub fn abs8(i: i8) -> i8 {
    if i < 0 {
        i.wrapping_neg()
    } else {
        i
    }
}
const B_M16_INTERLEAVE: [u8; 8] = [0, 49, 49, 41, 90, 27, 117, 10];
/// `sin8_C()`.
pub fn sin8(theta: u8) -> u8 {
    let mut offset = theta;
    if theta & 0x40 != 0 {
        offset = 255 - offset;
    }
    offset &= 0x3F;
    let mut secoffset = offset & 0x0F;
    if theta & 0x40 != 0 {
        secoffset += 1;
    }
    let s2 = (offset >> 4) as usize * 2;
    let b = B_M16_INTERLEAVE[s2];
    let m16 = B_M16_INTERLEAVE[s2 + 1];
    let mx = ((m16 as u16 * secoffset as u16) >> 4) as u8;
    let mut y = (mx as i32 + b as i32) as i8;
    if theta & 0x80 != 0 {
        y = (-(y as i32)) as i8;
    }
    y = (y as i32 + 128) as i8;
    y as u8
}
pub fn cos8(theta: u8) -> u8 {
    sin8(theta.wrapping_add(64))
}
pub fn atan2_8(dy: i16, dx: i16) -> u8 {
    if dy == 0 {
        return if dx >= 0 { 0 } else { 128 };
    }
    let abs_y = if dy > 0 { dy as i32 } else { -(dy as i32) };
    let dx = dx as i32;
    let a = if dx >= 0 { (32 - (32 * (dx - abs_y) / (dx + abs_y))) as i8 } else { (96 - (32 * (dx + abs_y) / (abs_y - dx))) as i8 };
    if dy < 0 {
        return (-(a as i32)) as u8;
    }
    a as u8
}
pub fn sqrt16(x: u16) -> u8 {
    if x <= 1 {
        return x as u8;
    }
    let mut low: u8 = 1;
    let mut hi: u8 = if x > 7904 { 255 } else { ((x >> 5) + 8) as u8 };
    loop {
        let mid = ((low as u16 + hi as u16) >> 1) as u8;
        if (mid as u16).wrapping_mul(mid as u16) > x {
            hi = mid.wrapping_sub(1);
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
    low - 1
}

#[derive(Clone, Copy, Debug)]
pub struct Hsv {
    pub h: u8,
    pub s: u8,
    pub v: u8,
}

fn rgb_of(hsv: Hsv) -> (u8, u8, u8) {
    hsv_to_rgb(hsv.h, hsv.s, hsv.v)
}

/// `last_hit_t`.
#[derive(Clone, Copy, Debug)]
pub struct LastHit {
    pub count: u8,
    pub x: [u8; LED_HITS_TO_REMEMBER],
    pub y: [u8; LED_HITS_TO_REMEMBER],
    pub index: [u8; LED_HITS_TO_REMEMBER],
    pub tick: [u16; LED_HITS_TO_REMEMBER],
}

impl LastHit {
    const fn new() -> Self {
        Self { count: 0, x: [0; 8], y: [0; 8], index: [0; 8], tick: [u16::MAX; 8] }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TaskState {
    Starting,
    Rendering,
    Flushing,
    Syncing,
}

/// rgb_matrix.c file statics, effect statics and the RNG states.
pub struct QmkState {
    task_state: TaskState,
    iter: u8,
    init: bool,
    params_flags: u8,
    last_effect: u8,
    last_enable: bool,
    pub g_rgb_timer: u32,
    rgb_timer_buffer: u32,
    pub g_last_hit_tracker: LastHit,
    pub last_hit_buffer: LastHit,
    pub frame_buffer: [[u8; MATRIX_COLS]; MATRIX_ROWS],
    /// lib8tion `rand16seed` (RAND16_SEED 1337).
    pub rand16seed: u16,
    /// newlib `rand()` state (`_rand_next`, initially 1).
    pub rand_next: u64,
    heatmap_decrease_timer: u16,
    decrease_heatmap_values: bool,
    digital_rain_drop: u8,
    digital_rain_decay: u8,
}

impl QmkState {
    pub const fn new() -> Self {
        Self {
            task_state: TaskState::Starting,
            iter: 0,
            init: false,
            params_flags: LED_FLAG_ALL,
            last_effect: 0,
            last_enable: false,
            g_rgb_timer: 0,
            rgb_timer_buffer: 0,
            g_last_hit_tracker: LastHit::new(),
            last_hit_buffer: LastHit::new(),
            frame_buffer: [[0; MATRIX_COLS]; MATRIX_ROWS],
            rand16seed: 1337,
            rand_next: 1,
            heatmap_decrease_timer: 0,
            decrease_heatmap_values: false,
            digital_rain_drop: 0,
            digital_rain_decay: 0,
        }
    }
}

/// What `rgb_matrix_task()` asks of the caller this pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskEvent {
    None,
    /// Rendering finished: run `rgb_matrix_indicators()` (kb indicators).
    Indicators,
}

fn limits(iter: u8) -> (u8, u8) {
    let min = RGB_MATRIX_LED_PROCESS_LIMIT as u16 * iter as u16;
    let max = (min + RGB_MATRIX_LED_PROCESS_LIMIT as u16).min(LED_COUNT as u16);
    (min.min(255) as u8, max as u8)
}

fn finished(led_max: u8) -> bool {
    (led_max as usize) < LED_COUNT
}

impl RgbMatrix {
    fn rand8(&mut self) -> u8 {
        let s = &mut self.qmk.rand16seed;
        *s = s.wrapping_mul(2053).wrapping_add(13849);
        ((*s & 0xFF) as u8).wrapping_add((*s >> 8) as u8)
    }
    fn random8_max(&mut self, lim: u8) -> u8 {
        ((self.rand8() as u16 * lim as u16) >> 8) as u8
    }
    fn random8_min_max(&mut self, min: u8, lim: u8) -> u8 {
        self.random8_max(lim.wrapping_sub(min)).wrapping_add(min)
    }
    /// newlib `rand()`.
    fn c_rand(&mut self) -> i32 {
        self.qmk.rand_next = self.qmk.rand_next.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.qmk.rand_next >> 32) & 0x7FFF_FFFF) as i32
    }

    fn hsv(&self) -> Hsv {
        Hsv { h: self.hue, s: self.sat, v: self.val }
    }

    fn led_on(&self, i: usize) -> bool {
        FLAGS[i] & self.qmk.params_flags != 0
    }

    fn set_rgb(&mut self, i: usize, c: (u8, u8, u8)) {
        self.set_color(i, c.0, c.1, c.2);
    }

    // ── rgb_matrix.c ─────────────────────────────────────────────────────

    /// `rgb_matrix_handle_key_event()` (RGB_MATRIX_KEYPRESSES).
    pub fn handle_key_event(&mut self, row: u8, col: u8, pressed: bool) {
        if !pressed || row as usize >= MATRIX_ROWS || col as usize >= MATRIX_COLS {
            return;
        }
        let led = MATRIX_CO[row as usize][col as usize];
        let led_count = (led != NO_LED) as usize;
        let b = &mut self.qmk.last_hit_buffer;
        if b.count as usize + led_count > LED_HITS_TO_REMEMBER {
            let keep = LED_HITS_TO_REMEMBER - led_count;
            b.x.copy_within(led_count..led_count + keep, 0);
            b.y.copy_within(led_count..led_count + keep, 0);
            b.tick.copy_within(led_count..led_count + keep, 0);
            b.index.copy_within(led_count..led_count + keep, 0);
            b.count = keep as u8;
        }
        if led_count == 1 {
            let i = b.count as usize;
            b.x[i] = POINT[led as usize].0;
            b.y[i] = POINT[led as usize].1;
            b.index[i] = led;
            b.tick[i] = 0;
            b.count += 1;
        }
        if self.mode == RGB_MATRIX_TYPING_HEATMAP {
            self.process_rgb_matrix_typing_heatmap(row, col);
        }
    }

    fn rgb_task_timers(&mut self, now: u32) {
        let delta = now.wrapping_sub(self.qmk.rgb_timer_buffer);
        self.qmk.rgb_timer_buffer = now;
        let b = &mut self.qmk.last_hit_buffer;
        let count = b.count as usize;
        for i in 0..count {
            if (u16::MAX as u32).wrapping_sub(delta) < b.tick[i] as u32 {
                b.count = b.count.wrapping_sub(1);
                continue;
            }
            b.tick[i] = b.tick[i].wrapping_add(delta as u16);
        }
    }

    /// `rgb_matrix_task()`. `flush` is the incremental driver flush
    /// (`is31fl3733_flush_continue()`): returns true once complete.
    #[inline(never)]
    pub fn rgb_matrix_task(&mut self, now: u32, mut flush: impl FnMut(&mut RgbMatrix) -> bool) -> TaskEvent {
        self.rgb_task_timers(now);
        let effect = if self.suspended || !self.enabled { 0 } else { self.mode };
        let mut event = TaskEvent::None;
        match self.qmk.task_state {
            TaskState::Starting => {
                self.qmk.iter = 0;
                self.qmk.g_rgb_timer = self.qmk.rgb_timer_buffer;
                self.qmk.g_last_hit_tracker = self.qmk.last_hit_buffer;
                self.qmk.task_state = TaskState::Rendering;
            }
            TaskState::Rendering => {
                self.rgb_task_render(effect, now);
                if effect != 0 && self.qmk.task_state == TaskState::Flushing {
                    event = TaskEvent::Indicators;
                }
            }
            TaskState::Flushing => {
                self.qmk.last_effect = effect;
                self.qmk.last_enable = self.enabled;
                if flush(self) {
                    self.qmk.task_state = TaskState::Syncing;
                }
            }
            TaskState::Syncing => {
                if now.wrapping_sub(self.qmk.g_rgb_timer) >= RGB_MATRIX_LED_FLUSH_LIMIT {
                    self.qmk.task_state = TaskState::Starting;
                }
            }
        }
        event
    }

    fn rgb_task_render(&mut self, effect: u8, now: u32) {
        self.qmk.init = effect != self.qmk.last_effect || self.enabled != self.qmk.last_enable;
        if self.qmk.params_flags != self.flags {
            self.qmk.params_flags = self.flags;
            self.set_all(0, 0, 0);
        }
        let rendering = match effect {
            0 => {
                if self.qmk.init {
                    self.set_all(0, 0, 0);
                }
                false
            }
            1 => self.solid_color(),
            2 => self.gradient_up_down(),
            3 => self.gradient_left_right(),
            4 => self.breathing(),
            5 => self.runner_i(band_sat),
            6 => self.runner_i(band_val),
            7 => self.runner_dx_dy(band_pinwheel_sat),
            8 => self.runner_dx_dy(band_pinwheel_val),
            9 => self.runner_dx_dy_dist(band_spiral_sat),
            10 => self.runner_dx_dy_dist(band_spiral_val),
            11 => self.runner_i(|h, _, t| Hsv { h: t, ..h }),
            12 => self.runner_i(|h, i, t| Hsv { h: (POINT[i].0 as i32 - t as i32) as u8, ..h }),
            13 => self.runner_i(|h, i, t| Hsv { h: (POINT[i].1 as i32 - t as i32) as u8, ..h }),
            14 => self.runner_i(rainbow_moving_chevron),
            15 => self.runner_dx_dy_dist(|h, _, _, d, t| Hsv { h: (3 * d as i32 / 2 + t as i32) as u8, ..h }),
            16 => self.runner_dx_dy(cycle_out_in_dual),
            17 => self.runner_dx_dy(|h, dx, dy, t| Hsv { h: atan2_8(dy, dx).wrapping_add(t), ..h }),
            18 => self.runner_dx_dy_dist(|h, dx, dy, d, t| Hsv { h: (d as i32 - t as i32 - atan2_8(dy, dx) as i32) as u8, ..h }),
            19 => self.runner_sin_cos_i(dual_beacon),
            20 => self.runner_sin_cos_i(rainbow_beacon),
            21 => self.runner_sin_cos_i(rainbow_pinwheels),
            22 => self.raindrops(),
            23 => self.jellybean_raindrops(),
            24 => self.hue_breathing(),
            25 => self.runner_i(hue_pendulum),
            26 => self.runner_i(hue_wave),
            27 => self.typing_heatmap(now),
            28 => self.digital_rain(),
            29 => self.runner_reactive(|h, off| Hsv { v: scale8((255 - off as i32) as u8, h.v), ..h }),
            30 => self.runner_reactive(|h, off| Hsv { h: h.h.wrapping_add(scale8((255 - off as i32) as u8, 64)), ..h }),
            31 => self.runner_splash(qsub8(self.qmk.g_last_hit_tracker.count, 1), reactive_wide),
            32 => self.runner_splash(0, reactive_wide),
            33 => self.runner_splash(qsub8(self.qmk.g_last_hit_tracker.count, 1), reactive_cross),
            34 => self.runner_splash(0, reactive_cross),
            35 => self.runner_splash(qsub8(self.qmk.g_last_hit_tracker.count, 1), reactive_nexus),
            36 => self.runner_splash(0, reactive_nexus),
            37 => self.runner_splash(qsub8(self.qmk.g_last_hit_tracker.count, 1), splash),
            38 => self.runner_splash(0, splash),
            39 => self.runner_splash(qsub8(self.qmk.g_last_hit_tracker.count, 1), solid_splash),
            40 => self.runner_splash(0, solid_splash),
            _ => false,
        };
        self.qmk.iter = self.qmk.iter.wrapping_add(1);
        if !rendering {
            self.qmk.task_state = TaskState::Flushing;
            if !self.qmk.init && effect == RGB_MATRIX_NONE {
                self.qmk.task_state = TaskState::Syncing;
            }
        }
    }

    // ── config setters (rgb_matrix.c) ────────────────────────────────────

    /// `rgb_matrix_mode_noeeprom()` / `rgb_matrix_mode()`.
    pub fn set_mode(&mut self, mode: u8) {
        if !self.enabled {
            return;
        }
        self.mode = mode.clamp(1, RGB_MATRIX_EFFECT_MAX - 1);
        self.qmk.task_state = TaskState::Starting;
    }

    /// `rgb_matrix_step()`.
    pub fn next_mode(&mut self) {
        let mode = self.mode.wrapping_add(1);
        self.set_mode(if mode < RGB_MATRIX_EFFECT_MAX { mode } else { 1 });
    }

    /// `rgb_matrix_step_reverse()`.
    pub fn prev_mode(&mut self) {
        let mode = self.mode.wrapping_sub(1);
        self.set_mode(if mode < 1 { RGB_MATRIX_EFFECT_MAX - 1 } else { mode });
    }

    /// `rgb_matrix_sethsv_noeeprom()` / `rgb_matrix_sethsv()`.
    pub fn sethsv_noeeprom(&mut self, h: u8, s: u8, v: u8) {
        if !self.enabled {
            return;
        }
        self.hue = h;
        self.sat = s;
        self.val = v.min(RGB_MATRIX_MAXIMUM_BRIGHTNESS);
    }

    /// `rgb_matrix_toggle()`.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.qmk.task_state = TaskState::Starting;
    }

    /// `rgb_matrix_enable_noeeprom()`.
    pub fn enable(&mut self) {
        if !self.enabled {
            self.qmk.task_state = TaskState::Starting;
        }
        self.enabled = true;
    }

    /// `rgb_matrix_disable_noeeprom()`.
    pub fn disable(&mut self) {
        if self.enabled {
            self.qmk.task_state = TaskState::Starting;
        }
        self.enabled = false;
    }

    pub fn increase_hue(&mut self) {
        self.sethsv_noeeprom(self.hue.wrapping_add(8), self.sat, self.val);
    }
    pub fn decrease_hue(&mut self) {
        self.sethsv_noeeprom(self.hue.wrapping_sub(8), self.sat, self.val);
    }
    pub fn increase_sat(&mut self) {
        self.sethsv_noeeprom(self.hue, qadd8(self.sat, 16), self.val);
    }
    pub fn decrease_sat(&mut self) {
        self.sethsv_noeeprom(self.hue, qsub8(self.sat, 16), self.val);
    }
    pub fn increase_val(&mut self) {
        self.sethsv_noeeprom(self.hue, self.sat, qadd8(self.val, 52));
    }
    pub fn decrease_val(&mut self) {
        self.sethsv_noeeprom(self.hue, self.sat, qsub8(self.val, 52));
    }
    pub fn increase_speed(&mut self) {
        self.speed = qadd8(self.speed, 52);
    }
    pub fn decrease_speed(&mut self) {
        self.speed = qsub8(self.speed, 52);
    }

    // ── runners ──────────────────────────────────────────────────────────

    fn runner_i(&mut self, f: fn(Hsv, usize, u8) -> Hsv) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let time = scale16by8(self.qmk.g_rgb_timer as u16, qadd8(self.speed / 4, 1)) as u8;
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let c = rgb_of(f(self.hsv(), i, time));
            self.set_rgb(i, c);
        }
        finished(max)
    }

    fn runner_dx_dy(&mut self, f: fn(Hsv, i16, i16, u8) -> Hsv) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let time = scale16by8(self.qmk.g_rgb_timer as u16, self.speed / 2) as u8;
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let dx = (POINT[i].0 as i32 - CENTER_X) as i16;
            let dy = (POINT[i].1 as i32 - CENTER_Y) as i16;
            let c = rgb_of(f(self.hsv(), dx, dy, time));
            self.set_rgb(i, c);
        }
        finished(max)
    }

    fn runner_dx_dy_dist(&mut self, f: fn(Hsv, i16, i16, u8, u8) -> Hsv) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let time = scale16by8(self.qmk.g_rgb_timer as u16, self.speed / 2) as u8;
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let dx = (POINT[i].0 as i32 - CENTER_X) as i16;
            let dy = (POINT[i].1 as i32 - CENTER_Y) as i16;
            let dist = sqrt16((dx as i32 * dx as i32 + dy as i32 * dy as i32) as u16);
            let c = rgb_of(f(self.hsv(), dx, dy, dist, time));
            self.set_rgb(i, c);
        }
        finished(max)
    }

    /// `effect_runner_sin_cos_i()`: note the math function's `sin` parameter
    /// receives the cosine value and vice versa, as in QMK.
    fn runner_sin_cos_i(&mut self, f: fn(Hsv, i8, i8, usize, u8) -> Hsv) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let time = scale16by8(self.qmk.g_rgb_timer as u16, self.speed / 4);
        let cos_value = (cos8(time as u8) as i32 - 128) as i8;
        let sin_value = (sin8(time as u8) as i32 - 128) as i8;
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let c = rgb_of(f(self.hsv(), cos_value, sin_value, i, time as u8));
            self.set_rgb(i, c);
        }
        finished(max)
    }

    fn runner_reactive(&mut self, f: fn(Hsv, u16) -> Hsv) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let max_tick = 65535 / qadd8(self.speed, 1) as u16;
        let hit = self.qmk.g_last_hit_tracker;
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let mut tick = max_tick;
            for j in (0..hit.count as usize).rev() {
                if hit.index[j] as usize == i && hit.tick[j] < tick {
                    tick = hit.tick[j];
                    break;
                }
            }
            let offset = scale16by8(tick, qadd8(self.speed, 1));
            let c = rgb_of(f(self.hsv(), offset));
            self.set_rgb(i, c);
        }
        finished(max)
    }

    fn runner_splash(&mut self, start: u8, f: fn(Hsv, i16, i16, u8, u16, u8) -> Hsv) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let hit = self.qmk.g_last_hit_tracker;
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let mut hsv = self.hsv();
            hsv.v = 0;
            for j in start as usize..hit.count as usize {
                let dx = (POINT[i].0 as i32 - hit.x[j] as i32) as i16;
                let dy = (POINT[i].1 as i32 - hit.y[j] as i32) as i16;
                let dist = sqrt16((dx as i32 * dx as i32 + dy as i32 * dy as i32) as u16);
                let tick = scale16by8(hit.tick[j], qadd8(self.speed, 1));
                hsv = f(hsv, dx, dy, dist, tick, self.hue);
            }
            hsv.v = scale8(hsv.v, self.val);
            let c = rgb_of(hsv);
            self.set_rgb(i, c);
        }
        finished(max)
    }

    // ── effects with their own loops ─────────────────────────────────────

    fn fill_all(&mut self, c: (u8, u8, u8)) -> bool {
        let (min, max) = limits(self.qmk.iter);
        for i in min as usize..max as usize {
            if self.led_on(i) {
                self.set_rgb(i, c);
            }
        }
        finished(max)
    }

    fn solid_color(&mut self) -> bool {
        let c = rgb_of(self.hsv());
        self.fill_all(c)
    }

    fn gradient_up_down(&mut self) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let scale = scale8(64, self.speed);
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let h = (self.hue as i32 + scale as i32 * (POINT[i].1 >> 4) as i32) as u8;
            let c = rgb_of(Hsv { h, ..self.hsv() });
            self.set_rgb(i, c);
        }
        finished(max)
    }

    fn gradient_left_right(&mut self) -> bool {
        let (min, max) = limits(self.qmk.iter);
        let scale = scale8(64, self.speed);
        for i in min as usize..max as usize {
            if !self.led_on(i) {
                continue;
            }
            let h = (self.hue as i32 + ((scale as i32 * POINT[i].0 as i32) >> 5)) as u8;
            let c = rgb_of(Hsv { h, ..self.hsv() });
            self.set_rgb(i, c);
        }
        finished(max)
    }

    fn breathing(&mut self) -> bool {
        let time = scale16by8(self.qmk.g_rgb_timer as u16, self.speed / 8);
        let mut hsv = self.hsv();
        hsv.v = scale8((abs8((sin8(time as u8) as i32 - 128) as i8) as i32 * 2) as u8, hsv.v);
        self.fill_all(rgb_of(hsv))
    }

    fn hue_breathing(&mut self) -> bool {
        let time = scale16by8(self.qmk.g_rgb_timer as u16, self.speed / 8);
        let mut hsv = self.hsv();
        hsv.h = (hsv.h as i32 + scale8((abs8((sin8(time as u8) as i32 - 128) as i8) as i32 * 2) as u8, 12) as i32) as u8;
        self.fill_all(rgb_of(hsv))
    }

    fn raindrops_set_color(&mut self, i: usize) {
        if !self.led_on(i) {
            return;
        }
        let h = self.hue as i32;
        let mut delta_h = ((h + 180) % 360 - h) / 4;
        if delta_h > 127 {
            delta_h -= 256;
        } else if delta_h < -127 {
            delta_h += 256;
        }
        let r = (self.rand8() & 0x03) as i32;
        let hsv = Hsv { h: (h + delta_h * r) as u8, s: self.sat, v: self.val };
        let c = rgb_of(hsv);
        self.set_rgb(i, c);
    }

    fn raindrops(&mut self) -> bool {
        let (min, max) = limits(self.qmk.iter);
        if !self.qmk.init {
            if scale16by8(self.qmk.g_rgb_timer as u16, qadd8(self.speed, 16)) % 10 == 0 {
                let i = self.random8_max(LED_COUNT as u8) as usize;
                self.raindrops_set_color(i);
            }
        } else {
            for i in min as usize..max as usize {
                self.raindrops_set_color(i);
            }
        }
        finished(max)
    }

    fn jellybean_set_color(&mut self, i: usize) {
        if !self.led_on(i) {
            return;
        }
        let h = self.rand8();
        let s = self.random8_min_max(127, 255);
        let c = rgb_of(Hsv { h, s, v: self.val });
        self.set_rgb(i, c);
    }

    fn jellybean_raindrops(&mut self) -> bool {
        let (min, max) = limits(self.qmk.iter);
        if !self.qmk.init {
            if scale16by8(self.qmk.g_rgb_timer as u16, qadd8(self.speed, 16)) % 5 == 0 {
                let i = self.random8_max(LED_COUNT as u8) as usize;
                self.jellybean_set_color(i);
            }
        } else {
            for i in min as usize..max as usize {
                self.jellybean_set_color(i);
            }
        }
        finished(max)
    }

    /// `process_rgb_matrix_typing_heatmap()`.
    fn process_rgb_matrix_typing_heatmap(&mut self, row: u8, col: u8) {
        let (row, col) = (row as usize, col as usize);
        if MATRIX_CO[row][col] == NO_LED {
            return;
        }
        let a = POINT[MATRIX_CO[row][col] as usize];
        for i_row in 0..MATRIX_ROWS {
            for i_col in 0..MATRIX_COLS {
                if MATRIX_CO[i_row][i_col] == NO_LED {
                    continue;
                }
                let fb = &mut self.qmk.frame_buffer;
                if i_row == row && i_col == col {
                    fb[row][col] = qadd8(fb[row][col], 32);
                } else {
                    let b = POINT[MATRIX_CO[i_row][i_col] as usize];
                    let dx = (a.0 as i32 - b.0 as i32) as i16 as i32;
                    let dy = (a.1 as i32 - b.1 as i32) as i16 as i32;
                    let distance = sqrt16((dx * dx + dy * dy) as u16);
                    if distance <= 40 {
                        let amount = qsub8(40, distance).min(16);
                        fb[i_row][i_col] = qadd8(fb[i_row][i_col], amount);
                    }
                }
            }
        }
    }

    fn typing_heatmap(&mut self, now: u32) -> bool {
        let (min, max) = limits(self.qmk.iter);
        if self.qmk.init {
            self.set_all(0, 0, 0);
            self.qmk.frame_buffer = [[0; MATRIX_COLS]; MATRIX_ROWS];
        }
        if self.qmk.iter == 0 {
            // timer_elapsed(heatmap_decrease_timer) >= 25 (16-bit timer)
            self.qmk.decrease_heatmap_values = (now as u16).wrapping_sub(self.qmk.heatmap_decrease_timer) >= 25;
            if self.qmk.decrease_heatmap_values {
                self.qmk.heatmap_decrease_timer = now as u16;
            }
        }
        let mut count = 0u8;
        let mut row = 0;
        while row < MATRIX_ROWS && count < RGB_MATRIX_LED_PROCESS_LIMIT {
            for col in 0..MATRIX_COLS {
                let led = MATRIX_CO[row][col];
                if led >= min && led < max {
                    count = count.wrapping_add(1);
                    let val = self.qmk.frame_buffer[row][col];
                    if !self.led_on(led as usize) {
                        continue;
                    }
                    let hsv = Hsv {
                        h: (170 - qsub8(val, 85) as i32) as u8,
                        s: self.sat,
                        v: scale8(((qadd8(170, val) as i32 - 170) * 3) as u8, self.val),
                    };
                    let c = rgb_of(hsv);
                    self.set_rgb(led as usize, c);
                    if self.qmk.decrease_heatmap_values {
                        self.qmk.frame_buffer[row][col] = qsub8(val, 1);
                    }
                }
            }
            row += 1;
        }
        finished(max)
    }

    fn digital_rain(&mut self) -> bool {
        const DROP_TICKS: u8 = 28;
        let v = self.val;
        let pure_green_intensity = ((v as u16 * 3) >> 2) as u8;
        let max_brightness_boost = ((v as u16 * 3) >> 2) as u8;
        let max_intensity = v;
        // Integer division by zero yields 0 on the Cortex-M0 runtime.
        let decay_ticks = 0xFFu8.checked_div(max_intensity).unwrap_or(0);

        if self.qmk.init {
            self.set_all(0, 0, 0);
            self.qmk.frame_buffer = [[0; MATRIX_COLS]; MATRIX_ROWS];
            self.qmk.digital_rain_drop = 0;
        }
        self.qmk.digital_rain_decay = self.qmk.digital_rain_decay.wrapping_add(1);
        for col in 0..MATRIX_COLS {
            for row in 0..MATRIX_ROWS {
                if row == 0 && self.qmk.digital_rain_drop == 0 && self.c_rand() < 0x7FFF_FFFF / 24 {
                    self.qmk.frame_buffer[row][col] = max_intensity;
                } else if self.qmk.frame_buffer[row][col] > 0 && self.qmk.frame_buffer[row][col] < max_intensity && self.qmk.digital_rain_decay == decay_ticks {
                    self.qmk.frame_buffer[row][col] -= 1;
                }
                let led = MATRIX_CO[row][col];
                if led != NO_LED {
                    let fb = self.qmk.frame_buffer[row][col];
                    if fb > pure_green_intensity {
                        let den = (max_intensity - pure_green_intensity) as u16;
                        let boost = ((max_brightness_boost as u16 * (fb - pure_green_intensity) as u16).checked_div(den).unwrap_or(0)) as u8;
                        self.set_color(led as usize, boost, max_intensity, boost);
                    } else {
                        let green = ((max_intensity as u16 * fb as u16).checked_div(pure_green_intensity as u16).unwrap_or(0)) as u8;
                        self.set_color(led as usize, 0, green, 0);
                    }
                }
            }
        }
        if self.qmk.digital_rain_decay == decay_ticks {
            self.qmk.digital_rain_decay = 0;
        }
        self.qmk.digital_rain_drop = self.qmk.digital_rain_drop.wrapping_add(1);
        if self.qmk.digital_rain_drop > DROP_TICKS {
            self.qmk.digital_rain_drop = 0;
            for row in (1..MATRIX_ROWS).rev() {
                for col in 0..MATRIX_COLS {
                    let fb = &mut self.qmk.frame_buffer;
                    if row == MATRIX_ROWS - 1 && fb[row][col] == max_intensity {
                        fb[row][col] = fb[row][col].wrapping_sub(1);
                    }
                    if fb[row - 1][col] >= max_intensity {
                        fb[row - 1][col] = max_intensity.wrapping_sub(1);
                        fb[row][col] = max_intensity;
                    }
                }
            }
        }
        false
    }
}

// ── effect math functions ────────────────────────────────────────────────

fn band_sat(mut hsv: Hsv, i: usize, time: u8) -> Hsv {
    let s = (hsv.s as i32 - (scale8(POINT[i].0, 228) as i32 + 28 - time as i32).abs() * 8) as i16;
    hsv.s = scale8(if s < 0 { 0 } else { s } as u8, hsv.s);
    hsv
}
fn band_val(mut hsv: Hsv, i: usize, time: u8) -> Hsv {
    let v = (hsv.v as i32 - (scale8(POINT[i].0, 228) as i32 + 28 - time as i32).abs() * 8) as i16;
    hsv.v = scale8(if v < 0 { 0 } else { v } as u8, hsv.v);
    hsv
}
fn band_pinwheel_sat(mut hsv: Hsv, dx: i16, dy: i16, time: u8) -> Hsv {
    hsv.s = scale8((hsv.s as i32 - time as i32 - atan2_8(dy, dx) as i32 * 3) as u8, hsv.s);
    hsv
}
fn band_pinwheel_val(mut hsv: Hsv, dx: i16, dy: i16, time: u8) -> Hsv {
    hsv.v = scale8((hsv.v as i32 - time as i32 - atan2_8(dy, dx) as i32 * 3) as u8, hsv.v);
    hsv
}
fn band_spiral_sat(mut hsv: Hsv, dx: i16, dy: i16, dist: u8, time: u8) -> Hsv {
    hsv.s = scale8((hsv.s as i32 + dist as i32 - time as i32 - atan2_8(dy, dx) as i32) as u8, hsv.s);
    hsv
}
fn band_spiral_val(mut hsv: Hsv, dx: i16, dy: i16, dist: u8, time: u8) -> Hsv {
    hsv.v = scale8((hsv.v as i32 + dist as i32 - time as i32 - atan2_8(dy, dx) as i32) as u8, hsv.v);
    hsv
}
fn rainbow_moving_chevron(mut hsv: Hsv, i: usize, time: u8) -> Hsv {
    let a = abs8((POINT[i].1 as i32 - CENTER_Y) as i8) as i32;
    hsv.h = (hsv.h as i32 + a + (POINT[i].0 as i32 - time as i32)) as u8;
    hsv
}
fn cycle_out_in_dual(mut hsv: Hsv, dx: i16, dy: i16, time: u8) -> Hsv {
    let dx = ((CENTER_X / 2) - abs8(dx as i8) as i32) as i16;
    let dist = sqrt16((dx as i32 * dx as i32 + dy as i32 * dy as i32) as u16);
    hsv.h = (3 * dist as i32 + time as i32) as u8;
    hsv
}
fn dual_beacon(mut hsv: Hsv, sin: i8, cos: i8, i: usize, _time: u8) -> Hsv {
    let d = ((POINT[i].1 as i32 - CENTER_Y) * cos as i32 + (POINT[i].0 as i32 - CENTER_X) * sin as i32) / 128;
    hsv.h = (hsv.h as i32 + d) as u8;
    hsv
}
fn rainbow_beacon(mut hsv: Hsv, sin: i8, cos: i8, i: usize, _time: u8) -> Hsv {
    let d = ((POINT[i].1 as i32 - CENTER_Y) * 2 * cos as i32 + (POINT[i].0 as i32 - CENTER_X) * 2 * sin as i32) / 128;
    hsv.h = (hsv.h as i32 + d) as u8;
    hsv
}
fn rainbow_pinwheels(mut hsv: Hsv, sin: i8, cos: i8, i: usize, _time: u8) -> Hsv {
    let ax = abs8((POINT[i].0 as i32 - CENTER_X) as i8) as i32;
    let d = ((POINT[i].1 as i32 - CENTER_Y) * 3 * cos as i32 + (56 - ax) * 3 * sin as i32) / 128;
    hsv.h = (hsv.h as i32 + d) as u8;
    hsv
}
fn hue_pendulum(mut hsv: Hsv, i: usize, time: u8) -> Hsv {
    let a = abs8((sin8(time) as i32 + POINT[i].0 as i32 - 128) as i8) as i32;
    hsv.h = (hsv.h as i32 + scale8((a * 2) as u8, 12) as i32) as u8;
    hsv
}
fn hue_wave(mut hsv: Hsv, i: usize, time: u8) -> Hsv {
    let a = abs8((POINT[i].0 as i32 - time as i32) as i8) as u8;
    hsv.h = (hsv.h as i32 + scale8(a, 24) as i32) as u8;
    hsv
}
fn reactive_wide(mut hsv: Hsv, _dx: i16, _dy: i16, dist: u8, tick: u16, _base_h: u8) -> Hsv {
    let mut effect = (tick as i32 + dist as i32 * 5) as u16;
    if effect > 255 {
        effect = 255;
    }
    hsv.v = qadd8(hsv.v, (255 - effect as i32) as u8);
    hsv
}
fn reactive_cross(mut hsv: Hsv, dx: i16, dy: i16, dist: u8, tick: u16, _base_h: u8) -> Hsv {
    let mut effect = (tick as i32 + dist as i32) as u16;
    let mut dx = dx as i32;
    let mut dy = dy as i32;
    dx = if dx < 0 { -dx } else { dx };
    dy = if dy < 0 { -dy } else { dy };
    dx = if dx * 16 > 255 { 255 } else { dx * 16 };
    dy = if dy * 16 > 255 { 255 } else { dy * 16 };
    effect = (effect as i32 + if dx > dy { dy } else { dx }) as u16;
    if effect > 255 {
        effect = 255;
    }
    hsv.v = qadd8(hsv.v, (255 - effect as i32) as u8);
    hsv
}
fn reactive_nexus(mut hsv: Hsv, dx: i16, dy: i16, dist: u8, tick: u16, base_h: u8) -> Hsv {
    let mut effect = (tick as i32 - dist as i32) as u16;
    if effect > 255 {
        effect = 255;
    }
    if dist > 72 {
        effect = 255;
    }
    if (dx > 8 || dx < -8) && (dy > 8 || dy < -8) {
        effect = 255;
    }
    // hsv.h = rgb_matrix_config.hsv.h + dy / 4 (config hue, not the running one)
    hsv.h = (base_h as i32 + dy as i32 / 4) as u8;
    hsv.v = qadd8(hsv.v, (255 - effect as i32) as u8);
    hsv
}
fn splash(mut hsv: Hsv, _dx: i16, _dy: i16, dist: u8, tick: u16, _base_h: u8) -> Hsv {
    let mut effect = (tick as i32 - dist as i32) as u16;
    if effect > 255 {
        effect = 255;
    }
    hsv.h = (hsv.h as i32 + effect as i32) as u8;
    hsv.v = qadd8(hsv.v, (255 - effect as i32) as u8);
    hsv
}
fn solid_splash(mut hsv: Hsv, _dx: i16, _dy: i16, dist: u8, tick: u16, _base_h: u8) -> Hsv {
    let mut effect = (tick as i32 - dist as i32) as u16;
    if effect > 255 {
        effect = 255;
    }
    hsv.v = qadd8(hsv.v, (255 - effect as i32) as u8);
    hsv
}
