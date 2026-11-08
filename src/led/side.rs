//! Side light bars and indicators — 1:1 port of `keyboards/air96_v2/ansi/side.c`.
//!
//! The ten side LEDs are RGB matrix indices 100..109 (left bar 100..104,
//! right bar 105..109) and are written into the shared RGB buffer, as
//! `rgb_matrix_set_color()` does in C.

use crate::hal::Hal;
use crate::kb::*;
use crate::led::side_tables::*;

pub const SIDE_WAVE: u8 = 0;
pub const SIDE_MIX: u8 = 1;
pub const SIDE_STATIC: u8 = 2;
pub const SIDE_BREATH: u8 = 3;
pub const SIDE_OFF: u8 = 4;

const LIGHT_COLOUR_MAX: u8 = 8;
const LIGHT_SPEED_MAX: u8 = 4;
const SIDE_BLINK_LIGHT: u8 = 128;
const SIDE_LINE: usize = 5;
const SIDE_INDEX: usize = 100;

const SIDE_SPEED_TABLE: [[u8; 5]; 5] = [
    [14, 19, 25, 32, 40], // SIDE_WAVE
    [14, 19, 25, 32, 40], // SIDE_MIX
    [50, 50, 50, 50, 50], // SIDE_STATIC
    [14, 19, 25, 32, 40], // SIDE_BREATH
    [50, 50, 50, 50, 50], // SIDE_OFF
];

const SIDE_LIGHT_TABLE: [u8; 6] = [0, 32, 64, 96, 128, 160];

const SIDE_LED_INDEX_TAB: [[usize; 2]; SIDE_LINE] = [
    [SIDE_INDEX + 4, SIDE_INDEX + 5],
    [SIDE_INDEX + 3, SIDE_INDEX + 6],
    [SIDE_INDEX + 2, SIDE_INDEX + 7],
    [SIDE_INDEX + 1, SIDE_INDEX + 8],
    [SIDE_INDEX, SIDE_INDEX + 9],
];

const RF_LED_LINK_PERIOD: u32 = 500;
const RF_LED_PAIR_PERIOD: u32 = 250;

/// side.c globals and function-local statics.
pub struct SideState {
    pub side_mode: u8,
    pub side_light: u8,
    pub side_speed: u8,
    pub side_rgb: u8,
    pub side_colour: u8,
    pub side_play_point: u8,
    pub side_play_cnt: u16,
    pub side_play_timer: u32,
    r_temp: u8,
    g_temp: u8,
    b_temp: u8,
    sys_show_timer: u32,
    sys_show_flag: bool,
    sleep_show_timer: u32,
    sleep_show_flag: bool,
    breathe_play_point: u8,
    rf_blink_timer: u32,
    bat_play_point: u8,
    bat_play_timer: u32,
    bat_show_time: u32,
    bat_show_flag: bool,
    bat_show_breath: bool,
    bat_sts_debounce: u32,
    bat_per_debounce: u32,
    charge_state: u8,
    bat_percent: u8,
    bat_f_init: bool,
    bat_full_shown: bool,
    bat_end_led: u8,
    bat_r: u8,
    bat_g: u8,
    bat_b: u8,
    f_bat_hold_prev: bool,
    f_bat_hold_timer: u32,
}

impl Default for SideState {
    fn default() -> Self {
        Self::new()
    }
}

impl SideState {
    pub const fn new() -> Self {
        Self {
            side_mode: 0,
            side_light: 3,
            side_speed: 2,
            side_rgb: 1,
            side_colour: 0,
            side_play_point: 0,
            side_play_cnt: 0,
            side_play_timer: 0,
            r_temp: 0,
            g_temp: 0,
            b_temp: 0,
            sys_show_timer: 0,
            sys_show_flag: false,
            sleep_show_timer: 0,
            sleep_show_flag: false,
            breathe_play_point: 0,
            rf_blink_timer: 0,
            bat_play_point: 0,
            bat_play_timer: 0,
            bat_show_time: 0,
            bat_show_flag: true,
            bat_show_breath: false,
            bat_sts_debounce: 0,
            bat_per_debounce: 0,
            charge_state: 0,
            bat_percent: 0,
            bat_f_init: true,
            bat_full_shown: false,
            bat_end_led: 0,
            bat_r: 0,
            bat_g: 0,
            bat_b: 0,
            f_bat_hold_prev: false,
            f_bat_hold_timer: 0,
        }
    }

    /// `count_rgb_light()`.
    fn count_rgb_light(&mut self, light_temp: u8) {
        let l = light_temp as u16;
        self.r_temp = ((l * self.r_temp as u16 + self.r_temp as u16) >> 8) as u8;
        self.g_temp = ((l * self.g_temp as u16 + self.g_temp as u16) >> 8) as u8;
        self.b_temp = ((l * self.b_temp as u16 + self.b_temp as u16) >> 8) as u8;
    }

    fn set_temp(&mut self, rgb: [u8; 3]) {
        self.r_temp = rgb[0];
        self.g_temp = rgb[1];
        self.b_temp = rgb[2];
    }

    /// Shared prologue of every `side_*_mode_show()`: run once per speed tick.
    fn play_tick(&mut self) -> bool {
        let step = SIDE_SPEED_TABLE[self.side_mode as usize][self.side_speed as usize] as u16;
        if self.side_play_cnt <= step {
            return false;
        }
        self.side_play_cnt -= step;
        if self.side_play_cnt > 20 {
            self.side_play_cnt = 0;
        }
        true
    }
}

/// `light_point_playing()`.
fn light_point_playing(trend: bool, step: u8, len: u8, point: &mut u8) {
    if trend {
        *point = point.wrapping_add(step);
        if *point >= len {
            *point -= len;
        }
    } else if *point < step {
        *point = len - (step - *point);
    } else {
        *point -= step;
    }
}

impl Kb {
    /// `light_level_control()`.
    pub fn light_level_control(&mut self, hal: &mut impl Hal, brighten: bool) {
        if brighten {
            if self.side.side_light == 5 {
                return;
            }
            self.side.side_light += 1;
        } else {
            if self.side.side_light == 0 {
                return;
            }
            self.side.side_light -= 1;
        }
        self.user_config.ee_side_light = self.side.side_light;
        self.eeconfig_update_user_datablock(hal);
    }

    /// `light_speed_control()`: lower side_speed = faster animation.
    pub fn light_speed_control(&mut self, hal: &mut impl Hal, fast: bool) {
        if self.side.side_speed > LIGHT_SPEED_MAX {
            self.side.side_speed = LIGHT_SPEED_MAX / 2;
        }
        if fast {
            if self.side.side_speed != 0 {
                self.side.side_speed -= 1;
            }
        } else if self.side.side_speed < LIGHT_SPEED_MAX {
            self.side.side_speed += 1;
        }
        self.user_config.ee_side_speed = self.side.side_speed;
        self.eeconfig_update_user_datablock(hal);
    }

    /// `side_colour_control()`.
    pub fn side_colour_control(&mut self, hal: &mut impl Hal, dir: bool) {
        let s = &mut self.side;
        if s.side_mode != SIDE_WAVE && s.side_rgb != 0 {
            s.side_rgb = 0;
            s.side_colour = 0;
        }

        if dir {
            if s.side_rgb != 0 {
                s.side_rgb = 0;
                s.side_colour = 0;
            } else {
                s.side_colour += 1;
                if s.side_colour >= LIGHT_COLOUR_MAX {
                    s.side_rgb = 1;
                    s.side_colour = 0;
                }
            }
        } else if s.side_rgb != 0 {
            s.side_rgb = 0;
            s.side_colour = LIGHT_COLOUR_MAX - 1;
        } else if s.side_colour == 0 {
            s.side_rgb = 1;
            s.side_colour = 0;
        } else {
            s.side_colour -= 1;
        }
        self.user_config.ee_side_rgb = self.side.side_rgb;
        self.user_config.ee_side_colour = self.side.side_colour;
        self.eeconfig_update_user_datablock(hal);
    }

    /// `side_mode_control()`.
    pub fn side_mode_control(&mut self, hal: &mut impl Hal, dir: bool) {
        let s = &mut self.side;
        if dir {
            s.side_mode += 1;
            if s.side_mode > SIDE_OFF {
                s.side_mode = 0;
            }
        } else if s.side_mode > 0 {
            s.side_mode -= 1;
        } else {
            s.side_mode = SIDE_OFF;
        }
        s.side_play_point = 0;
        self.user_config.ee_side_mode = self.side.side_mode;
        self.eeconfig_update_user_datablock(hal);
    }

    fn set_left_rgb(&mut self, r: u8, g: u8, b: u8) {
        for i in 0..SIDE_LINE {
            self.rgb.set_color(SIDE_INDEX + i, r, g, b);
        }
    }

    fn set_right_rgb(&mut self, r: u8, g: u8, b: u8) {
        for i in 0..SIDE_LINE {
            self.rgb.set_color(SIDE_INDEX + SIDE_LINE + i, r, g, b);
        }
    }

    fn set_side_temp(&mut self, i: usize) {
        let (r, g, b) = (self.side.r_temp, self.side.g_temp, self.side.b_temp);
        for j in 0..2 {
            self.rgb.set_color(SIDE_LED_INDEX_TAB[i][j], r, g, b);
        }
    }

    /// `sys_sw_led_show()`.
    fn sys_sw_led_show(&mut self, hal: &impl Hal) {
        if self.f_sys_show {
            self.f_sys_show = false;
            self.side.sys_show_timer = hal.now();
            self.side.sys_show_flag = true;
        }

        if self.side.sys_show_flag {
            let c = if self.dev_info.sys_sw_state == SYS_SW_MAC {
                [SIDE_BLINK_LIGHT, SIDE_BLINK_LIGHT, SIDE_BLINK_LIGHT]
            } else {
                [0x00, 0x00, SIDE_BLINK_LIGHT]
            };
            self.side.set_temp(c);
            let elapsed = hal.now().wrapping_sub(self.side.sys_show_timer);
            if (elapsed / 500) % 2 == 0 {
                self.set_right_rgb(c[0], c[1], c[2]);
            } else {
                self.set_right_rgb(0, 0, 0);
            }
            if elapsed >= 3000 {
                self.side.sys_show_flag = false;
            }
        }
    }

    /// `sleep_sw_led_show()`.
    fn sleep_sw_led_show(&mut self, hal: &impl Hal) {
        if self.f_sleep_show {
            self.f_sleep_show = false;
            self.side.sleep_show_timer = hal.now();
            self.side.sleep_show_flag = true;
        }

        if self.side.sleep_show_flag {
            let c = if self.user_config.sleep_enable {
                [0x00, SIDE_BLINK_LIGHT, 0x00]
            } else {
                [SIDE_BLINK_LIGHT, 0x00, 0x00]
            };
            self.side.set_temp(c);
            let elapsed = hal.now().wrapping_sub(self.side.sleep_show_timer);
            if (elapsed / 500) % 2 == 0 {
                self.set_right_rgb(c[0], c[1], c[2]);
            } else {
                self.set_right_rgb(0, 0, 0);
            }
            if elapsed >= 3000 {
                self.side.sleep_show_flag = false;
            }
        }
    }

    /// `sys_led_show()`: host Caps Lock on the left bar.
    fn sys_led_show(&mut self, hal: &impl Hal) {
        let caps = if self.dev_info.link_mode == LINK_USB {
            self.host_keyboard_leds(hal) & 0x02 != 0
        } else {
            self.dev_info.rf_led & 0x02 != 0
        };
        if caps {
            self.set_left_rgb(0x00, SIDE_BLINK_LIGHT, SIDE_BLINK_LIGHT);
        }
    }

    /// `side_wave_mode_show()`.
    fn side_wave_mode_show(&mut self) {
        if !self.side.play_tick() {
            return;
        }
        let rainbow = self.side.side_rgb != 0;
        if rainbow {
            light_point_playing(false, 3, FLOW_COLOUR_TAB_LEN as u8, &mut self.side.side_play_point);
        } else {
            light_point_playing(false, 2, WAVE_TAB_LEN as u8, &mut self.side.side_play_point);
        }

        let mut play_index = self.side.side_play_point;
        for i in 0..SIDE_LINE {
            if rainbow {
                self.side.set_temp(FLOW_RAINBOW_COLOUR_TAB[play_index as usize]);
                light_point_playing(true, 16, FLOW_COLOUR_TAB_LEN as u8, &mut play_index);
            } else {
                self.side.set_temp(COLOUR_LIB[self.side.side_colour as usize]);
                light_point_playing(true, 12, WAVE_TAB_LEN as u8, &mut play_index);
                self.side.count_rgb_light(WAVE_DATA_TAB[play_index as usize]);
            }
            self.side.count_rgb_light(SIDE_LIGHT_TABLE[self.side.side_light as usize]);
            self.set_side_temp(i);
        }
    }

    /// `side_spectrum_mode_show()`.
    fn side_spectrum_mode_show(&mut self) {
        if !self.side.play_tick() {
            return;
        }
        light_point_playing(true, 1, FLOW_COLOUR_TAB_LEN as u8, &mut self.side.side_play_point);
        self.side.set_temp(FLOW_RAINBOW_COLOUR_TAB[self.side.side_play_point as usize]);
        self.side.count_rgb_light(SIDE_LIGHT_TABLE[self.side.side_light as usize]);
        for i in 0..SIDE_LINE {
            self.set_side_temp(i);
        }
    }

    /// `side_breathe_mode_show()`.
    fn side_breathe_mode_show(&mut self) {
        if !self.side.play_tick() {
            return;
        }
        light_point_playing(false, 1, BREATHE_TAB_LEN as u8, &mut self.side.breathe_play_point);
        self.side.set_temp(COLOUR_LIB[self.side.side_colour as usize]);
        self.side.count_rgb_light(BREATHE_DATA_TAB[self.side.breathe_play_point as usize]);
        self.side.count_rgb_light(SIDE_LIGHT_TABLE[self.side.side_light as usize]);
        for i in 0..SIDE_LINE {
            self.set_side_temp(i);
        }
    }

    /// `side_static_mode_show()`.
    fn side_static_mode_show(&mut self) {
        if !self.side.play_tick() {
            return;
        }
        for i in 0..SIDE_LINE {
            self.side.set_temp(COLOUR_LIB[self.side.side_colour as usize]);
            self.side.count_rgb_light(SIDE_LIGHT_TABLE[self.side.side_light as usize]);
            self.set_side_temp(i);
        }
    }

    /// `side_off_mode_show()`.
    fn side_off_mode_show(&mut self) {
        if !self.side.play_tick() {
            return;
        }
        self.side.set_temp([0, 0, 0]);
        for i in 0..SIDE_LINE {
            self.set_side_temp(i);
        }
    }

    /// `rf_led_show()`: link state on the left bar.
    fn rf_led_show(&mut self, hal: &impl Hal) {
        let mut c = if self.dev_info.link_mode == LINK_RF_24 {
            [0x00, SIDE_BLINK_LIGHT, 0x00]
        } else if self.dev_info.link_mode == LINK_USB {
            [SIDE_BLINK_LIGHT, SIDE_BLINK_LIGHT, 0x00]
        } else {
            [0x00, 0x00, SIDE_BLINK_LIGHT]
        };

        if self.rf_blink_cnt != 0 {
            let period = if self.dev_info.rf_state == RF_PAIRING { RF_LED_PAIR_PERIOD } else { RF_LED_LINK_PERIOD };
            let elapsed = hal.now().wrapping_sub(self.side.rf_blink_timer);
            if elapsed >= period >> 1 {
                c = [0, 0, 0];
            }
            if elapsed >= period {
                self.rf_blink_cnt -= 1;
                self.side.rf_blink_timer = hal.now();
            }
        } else if self.rf_link_show_time < RF_LINK_SHOW_TIME {
        } else {
            self.side.rf_blink_timer = hal.now();
            return;
        }

        self.side.set_temp(c);
        self.set_left_rgb(c[0], c[1], c[2]);
    }

    /// `bat_num_led()`.
    fn bat_num_led(&mut self, bat_percent: u8) {
        let (r, g, b) = if bat_percent <= 15 {
            (0xFF, 0x00, 0x00)
        } else if bat_percent <= 50 {
            (0xFF, 0x40, 0x00)
        } else if bat_percent <= 80 {
            (0xFF, 0xFF, 0x00)
        } else {
            (0x00, 0xFF, 0x00)
        };

        const BATTERY_LEDS: [usize; 10] = [29, 28, 27, 26, 25, 24, 23, 22, 21, 20];
        if bat_percent >= 1 {
            self.rgb.set_color(BATTERY_LEDS[0], r, g, b);
        }
        for i in 1..10u8 {
            if bat_percent > i * 10 {
                self.rgb.set_color(BATTERY_LEDS[i as usize], r, g, b);
            }
        }
    }

    /// `num_led_show()`.
    pub fn num_led_show(&mut self) {
        let num_bat_temp = self.dev_info.rf_baterry;
        self.bat_num_led(num_bat_temp);
    }

    /// `bat_percent_led()`.
    fn bat_percent_led(&mut self, bat_percent: u8) {
        let s = &mut self.side;
        let half = SIDE_BLINK_LIGHT / 2;
        let (end, r, g, b) = if bat_percent <= 20 {
            (0, SIDE_BLINK_LIGHT, 0, 0)
        } else if bat_percent <= 30 {
            (1, SIDE_BLINK_LIGHT, half, 0)
        } else if bat_percent <= 50 {
            (2, SIDE_BLINK_LIGHT, half, 0)
        } else if bat_percent <= 70 {
            (3, SIDE_BLINK_LIGHT, half, 0)
        } else if bat_percent <= 95 {
            (4, SIDE_BLINK_LIGHT, half, 0)
        } else {
            (4, 0, SIDE_BLINK_LIGHT, 0)
        };
        s.bat_end_led = end;
        s.bat_r = r;
        s.bat_g = g;
        s.bat_b = b;

        let mut i = 0usize;
        while i <= end as usize {
            self.rgb.set_color(SIDE_INDEX + 9 - i, r, g, b);
            i += 1;
        }
        while i < SIDE_LINE {
            self.rgb.set_color(SIDE_INDEX + 9 - i, 0, 0, 0);
            i += 1;
        }
    }

    /// `bat_led_show()`: battery state on the right bar.
    fn bat_led_show(&mut self, hal: &impl Hal) {
        /* Skip battery indicator in wired USB mode — keep rainbow */
        if self.dev_info.link_mode == LINK_USB {
            return;
        }
        let now = hal.now();
        let s = &mut self.side;

        if s.bat_f_init {
            s.bat_f_init = false;
            s.bat_show_time = now;
            s.charge_state = self.dev_info.rf_charge;
            s.bat_percent = self.dev_info.rf_baterry;
        }

        if s.charge_state != self.dev_info.rf_charge {
            if now.wrapping_sub(s.bat_sts_debounce) > 1000 {
                if (s.charge_state & 0x01) == 0 && (self.dev_info.rf_charge & 0x01) != 0 {
                    s.bat_show_flag = true;
                    s.bat_show_breath = true;
                    s.bat_show_time = now;
                }
                s.charge_state = self.dev_info.rf_charge;
            }
        } else {
            s.bat_sts_debounce = now;
            if now.wrapping_sub(s.bat_show_time) > 5000 {
                s.bat_show_flag = false;
                s.bat_show_breath = false;
            }
            if s.charge_state == 0x03 && !s.bat_full_shown {
                s.bat_show_breath = true;
                s.bat_show_flag = true;
                s.bat_show_time = now;
                s.bat_full_shown = true;
            }
            if s.charge_state != 0x03 {
                s.bat_full_shown = false;
            }
        }

        if s.bat_percent != self.dev_info.rf_baterry {
            if now.wrapping_sub(s.bat_per_debounce) > 1000 {
                s.bat_percent = self.dev_info.rf_baterry;
            }
        } else {
            s.bat_per_debounce = now;
            if s.bat_percent < 15 {
                s.bat_show_flag = true;
                s.bat_show_time = now;
            }
        }

        if self.f_bat_hold || self.side.bat_show_flag {
            if self.side.bat_show_breath {
                if now.wrapping_sub(self.side.bat_play_timer) > 10 {
                    self.side.bat_play_timer = now;
                    light_point_playing(false, 1, BREATHE_TAB_LEN as u8, &mut self.side.bat_play_point);
                }
                self.side.set_temp([0xFF, 0x40, 0x00]);
                self.side.count_rgb_light(BREATHE_DATA_TAB[self.side.bat_play_point as usize]);
                let (r, g, b) = (self.side.r_temp, self.side.g_temp, self.side.b_temp);
                self.set_right_rgb(r, g, b);
            } else {
                let p = self.side.bat_percent;
                self.bat_percent_led(p);
            }
        }
    }

    /// `device_reset_show()`: three white blinks.
    pub fn device_reset_show(&mut self, hal: &mut impl Hal) {
        hal.led_power(true);
        for _ in 0..3 {
            self.rgb.set_all(0xFF, 0xFF, 0xFF);
            hal.rgb_update_pwm_buffers(&mut self.rgb);
            hal.wait_ms(200);

            self.rgb.set_all(0x00, 0x00, 0x00);
            hal.rgb_update_pwm_buffers(&mut self.rgb);
            hal.wait_ms(200);
        }
    }

    /// `device_reset_init()`.
    pub fn device_reset_init(&mut self, hal: &mut impl Hal) {
        let s = &mut self.side;
        s.side_mode = 0;
        s.side_light = 3;
        s.side_speed = 2;
        s.side_rgb = 1;
        s.side_colour = 0;
        s.side_play_point = 0;
        s.side_play_cnt = 0;
        s.side_play_timer = hal.now();

        self.f_bat_hold = false;

        self.rgb.enable();
        self.rgb.set_mode(RGB_MATRIX_DEFAULT_MODE);
        self.rgb.speed = 255 - RGB_MATRIX_SPD_STEP * 2;
        self.rgb.sethsv_noeeprom(255, 255, RGB_MATRIX_MAXIMUM_BRIGHTNESS - RGB_MATRIX_VAL_STEP * 2);

        self.user_config.default_brightness_flag = 0xA5;
        self.user_config.ee_side_mode = self.side.side_mode;
        self.user_config.ee_side_light = self.side.side_light;
        self.user_config.ee_side_speed = self.side.side_speed;
        self.user_config.ee_side_rgb = self.side.side_rgb;
        self.user_config.ee_side_colour = self.side.side_colour;
        self.user_config.sleep_enable = true;
        self.eeconfig_update_user_datablock(hal);
    }

    /// `rgb_test_show()`: seven solid colours, 500ms each.
    pub fn rgb_test_show(&mut self, hal: &mut impl Hal) {
        hal.led_power(true);
        const COLOURS: [[u8; 3]; 7] = [
            [0xFF, 0x00, 0x00],
            [0x00, 0xFF, 0x00],
            [0x00, 0x00, 0xFF],
            [0x80, 0x80, 0x80],
            [0x80, 0x80, 0x00],
            [0x80, 0x00, 0x80],
            [0x00, 0x80, 0x80],
        ];
        for c in COLOURS {
            self.rgb.set_all(c[0], c[1], c[2]);
            hal.rgb_update_pwm_buffers(&mut self.rgb);
            hal.wait_ms(500);
        }
    }

    /// `m_side_led_show()`.
    pub fn m_side_led_show(&mut self, hal: &mut impl Hal) {
        /* Auto-clear f_bat_hold after 10 seconds */
        if self.f_bat_hold && !self.side.f_bat_hold_prev {
            self.side.f_bat_hold_timer = hal.now();
        }
        self.side.f_bat_hold_prev = self.f_bat_hold;
        if self.f_bat_hold && hal.now().wrapping_sub(self.side.f_bat_hold_timer) > 10000 {
            self.f_bat_hold = false;
        }

        let elapsed = hal.now().wrapping_sub(self.side.side_play_timer);
        self.side.side_play_cnt = self.side.side_play_cnt.wrapping_add(elapsed as u16);
        self.side.side_play_timer = hal.now();

        match self.side.side_mode {
            SIDE_WAVE => self.side_wave_mode_show(),
            SIDE_MIX => self.side_spectrum_mode_show(),
            SIDE_BREATH => self.side_breathe_mode_show(),
            SIDE_STATIC => self.side_static_mode_show(),
            SIDE_OFF => self.side_off_mode_show(),
            _ => {}
        }

        self.bat_led_show(hal);
        self.sleep_sw_led_show(hal);
        self.sys_sw_led_show(hal);

        self.sys_led_show(hal);
        self.rf_led_show(hal);
    }
}
