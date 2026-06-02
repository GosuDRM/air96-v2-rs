//! Emulated EEPROM using the last page of STM32F072 flash memory.
//!
//! The last 2KB flash page (page 63, address 0x0801F800) is used to store
//! a small user config block. This survives power cycles but has limited
//! write endurance (~10k cycles per page).
//!
//! Config layout (16 bytes at offset 0):
//!   [0] = magic flag (0xA8 = valid v4: RGB defaults, max speed; 0xA5 = v1)
//!   [1] = side_mode
//!   [2] = side_brightness
//!   [3] = side_speed
//!   [4] = side_colour
//!   [5] = side_rgb (bool)
//!   [6] = sleep_enable (bool)
//!   [7] = rgb_mode (u8)
//!   [8] = rgb_hue (u8)
//!   [9] = rgb_sat (u8)
//!   [10] = rgb_val (u8)
//!   [11] = rgb_speed (u8)
//!   [12] = rgb_enabled (bool)
//!   [13..15] = reserved / padding

use stm32f0xx_hal::pac;

/// Config page address: last 2KB page (page 63 of 64, 128K flash)
const CONFIG_PAGE_ADDR: u32 = 0x0801_F800;
const CONFIG_PAGE_SIZE: usize = 2048;
const MAGIC_VALID_V1: u8 = 0xA5;
const MAGIC_VALID_V4: u8 = 0xA8;

/// User config stored in flash
#[derive(Debug, Clone, Copy)]
pub struct UserConfig {
    pub side_mode: u8,
    pub side_brightness: u8,
    pub side_speed: u8,
    pub side_colour: u8,
    pub side_rgb: bool,
    pub sleep_enable: bool,
    // ── Main RGB matrix settings ──────────────────────────────────────
    pub rgb_mode: u8,
    pub rgb_hue: u8,
    pub rgb_sat: u8,
    pub rgb_val: u8,
    pub rgb_speed: u8,
    pub rgb_enabled: bool,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            side_mode: 0,
            side_brightness: 3,
            side_speed: 2,
            side_colour: 0,
            side_rgb: true,
            sleep_enable: true,
            rgb_mode: 4,     // CYCLE_LEFT_RIGHT — matches C reference default
            rgb_hue: 0,     // RGB_MATRIX_DEFAULT_HUE
            rgb_sat: 255,   // RGB_MATRIX_DEFAULT_SAT
            rgb_val: 255,   // RGB_MATRIX_DEFAULT_VAL
            rgb_speed: 255, // max speed (default)
            rgb_enabled: true,
        }
    }
}

/// Load user config from emulated EEPROM.
/// Returns None if flash page is uninitialized.
pub fn load() -> Option<UserConfig> {
    let addr = CONFIG_PAGE_ADDR as *const u8;
    unsafe {
        let magic = core::ptr::read_volatile(addr);
        if magic == MAGIC_VALID_V4 {
            let ptr = addr.add(1);
            Some(UserConfig {
                side_mode:       core::ptr::read_volatile(ptr),
                side_brightness: core::ptr::read_volatile(ptr.add(1)),
                side_speed:      core::ptr::read_volatile(ptr.add(2)),
                side_colour:     core::ptr::read_volatile(ptr.add(3)),
                side_rgb:        core::ptr::read_volatile(ptr.add(4)) != 0,
                sleep_enable:    core::ptr::read_volatile(ptr.add(5)) != 0,
                rgb_mode:        core::ptr::read_volatile(ptr.add(6)),
                rgb_hue:         core::ptr::read_volatile(ptr.add(7)),
                rgb_sat:         core::ptr::read_volatile(ptr.add(8)),
                rgb_val:         core::ptr::read_volatile(ptr.add(9)),
                rgb_speed:       core::ptr::read_volatile(ptr.add(10)),
                rgb_enabled:     core::ptr::read_volatile(ptr.add(11)) != 0,
            })
        } else if magic == MAGIC_VALID_V1 {
            let ptr = addr.add(1);
            Some(UserConfig {
                side_mode:       core::ptr::read_volatile(ptr),
                side_brightness: core::ptr::read_volatile(ptr.add(1)),
                side_speed:      core::ptr::read_volatile(ptr.add(2)),
                side_colour:     core::ptr::read_volatile(ptr.add(3)),
                side_rgb:        core::ptr::read_volatile(ptr.add(4)) != 0,
                sleep_enable:    core::ptr::read_volatile(ptr.add(5)) != 0,
                ..Default::default()
            })
        } else {
            None
        }
    }
}

/// Save user config to emulated EEPROM.
/// Erases the config page first, then writes 16 bytes.
/// WARNING: Must not be interrupted — disables interrupts during write.
pub fn save(cfg: &UserConfig) {
    cortex_m::interrupt::free(|_| {
        unsafe {
            let flash = &*pac::FLASH::ptr();

            // 1. Unlock flash
            flash.keyr.write(|w| w.bits(0x4567_0123));
            flash.keyr.write(|w| w.bits(0xCDEF_89AB));

            // 2. Wait until not busy
            while flash.sr.read().bsy().bit() {}

            // 3. Page erase
            flash.cr.modify(|_, w| w.per().set_bit());
            flash.ar.write(|w| w.bits(CONFIG_PAGE_ADDR));
            flash.cr.modify(|_, w| w.strt().set_bit());
            while flash.sr.read().bsy().bit() {}
            flash.cr.modify(|_, w| w.per().clear_bit());

            // 4. Program 8 halfwords (16 bytes)
            flash.cr.modify(|_, w| w.pg().set_bit());

            let ptr = CONFIG_PAGE_ADDR as *mut u16;
            
            // Halfword 0: magic (low) | side_mode (high)
            let hw0 = MAGIC_VALID_V4 as u16 | ((cfg.side_mode as u16) << 8);
            core::ptr::write_volatile(ptr, hw0);
            while flash.sr.read().bsy().bit() {}

            // Halfword 1: side_brightness | side_speed
            let hw1 = (cfg.side_brightness as u16) | ((cfg.side_speed as u16) << 8);
            core::ptr::write_volatile(ptr.add(1), hw1);
            while flash.sr.read().bsy().bit() {}

            // Halfword 2: side_colour | side_rgb
            let hw2 = (cfg.side_colour as u16) | ((cfg.side_rgb as u16) << 8);
            core::ptr::write_volatile(ptr.add(2), hw2);
            while flash.sr.read().bsy().bit() {}

            // Halfword 3: sleep_enable | rgb_mode
            let hw3 = (cfg.sleep_enable as u16) | ((cfg.rgb_mode as u16) << 8);
            core::ptr::write_volatile(ptr.add(3), hw3);
            while flash.sr.read().bsy().bit() {}

            // Halfword 4: rgb_hue | rgb_sat
            let hw4 = (cfg.rgb_hue as u16) | ((cfg.rgb_sat as u16) << 8);
            core::ptr::write_volatile(ptr.add(4), hw4);
            while flash.sr.read().bsy().bit() {}

            // Halfword 5: rgb_val | rgb_speed
            let hw5 = (cfg.rgb_val as u16) | ((cfg.rgb_speed as u16) << 8);
            core::ptr::write_volatile(ptr.add(5), hw5);
            while flash.sr.read().bsy().bit() {}

            // Halfword 6: rgb_enabled | reserved (0)
            let hw6 = cfg.rgb_enabled as u16;
            core::ptr::write_volatile(ptr.add(6), hw6);
            while flash.sr.read().bsy().bit() {}

            // Halfword 7: reserved (0) | reserved (0)
            core::ptr::write_volatile(ptr.add(7), 0);
            while flash.sr.read().bsy().bit() {}

            flash.cr.modify(|_, w| w.pg().clear_bit());

            // 5. Lock flash
            flash.cr.modify(|_, w| w.lock().set_bit());
        }
    });
}

/// Read a byte from the config page at the given offset.
pub fn read_byte(offset: usize) -> u8 {
    if offset >= CONFIG_PAGE_SIZE { return 0xFF; }
    unsafe {
        core::ptr::read_volatile((CONFIG_PAGE_ADDR + offset as u32) as *const u8)
    }
}

/// Write a byte to the config page at the given offset.
/// Performs a read/modify/erase/write cycle on the entire page.
/// WARNING: Must not be interrupted — disables interrupts during write.
pub fn write_byte(offset: usize, value: u8) {
    if offset >= CONFIG_PAGE_SIZE { return; }

    cortex_m::interrupt::free(|_| {
        unsafe {
            // 1. Read entire page into RAM
            let mut buf = [0u8; CONFIG_PAGE_SIZE];
            for i in 0..CONFIG_PAGE_SIZE {
                buf[i] = core::ptr::read_volatile(
                    (CONFIG_PAGE_ADDR + i as u32) as *const u8
                );
            }

            // 2. Modify the target byte
            buf[offset] = value;

            let flash = &*pac::FLASH::ptr();

            // 3. Unlock flash
            flash.keyr.write(|w| w.bits(0x4567_0123));
            flash.keyr.write(|w| w.bits(0xCDEF_89AB));

            // 4. Wait until not busy
            while flash.sr.read().bsy().bit() {}

            // 5. Page erase
            flash.cr.modify(|_, w| w.per().set_bit());
            flash.ar.write(|w| w.bits(CONFIG_PAGE_ADDR));
            flash.cr.modify(|_, w| w.strt().set_bit());
            while flash.sr.read().bsy().bit() {}
            flash.cr.modify(|_, w| w.per().clear_bit());

            // 6. Program page (halfword writes)
            flash.cr.modify(|_, w| w.pg().set_bit());
            let ptr = CONFIG_PAGE_ADDR as *mut u16;
            for i in 0..CONFIG_PAGE_SIZE / 2 {
                let hw = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
                core::ptr::write_volatile(ptr.add(i), hw);
                while flash.sr.read().bsy().bit() {}
            }
            flash.cr.modify(|_, w| w.pg().clear_bit());

            // 7. Lock flash
            flash.cr.modify(|_, w| w.lock().set_bit());
        }
    });
}

/// Reset the dynamic keymap and user config to defaults.
/// Writes the default keymap to EEPROM and resets the config.
pub fn reset_to_defaults() {
    // Write default config first
    save(&UserConfig::default());

    // Write default keymap layers
    use crate::keyboard::keymap;
    use crate::via::{DYNAMIC_KEYMAP_LAYER_COUNT, VIA_MATRIX_ROWS, VIA_MATRIX_COLS};

    let layers: [&[[u16; VIA_MATRIX_COLS]; VIA_MATRIX_ROWS]; 5] = [
        &keymap::LAYER_MAC,
        &keymap::LAYER_MAC_FN,
        &keymap::LAYER_WIN,
        &keymap::LAYER_WIN_FN,
        &keymap::LAYER_FN,
    ];

    for (layer_idx, layer) in layers.iter().enumerate() {
        for row in 0..VIA_MATRIX_ROWS {
            for col in 0..VIA_MATRIX_COLS {
                let kc = layer[row][col];
                let addr = 64 + layer_idx * VIA_MATRIX_ROWS * VIA_MATRIX_COLS * 2
                    + row * VIA_MATRIX_COLS * 2 + col * 2;
                write_byte(addr, (kc >> 8) as u8);
                write_byte(addr + 1, (kc & 0xFF) as u8);
            }
        }
    }

    // Fill remaining layers with KC_NO (0x0000)
    for layer_idx in 5..DYNAMIC_KEYMAP_LAYER_COUNT {
        for row in 0..VIA_MATRIX_ROWS {
            for col in 0..VIA_MATRIX_COLS {
                let addr = 64 + layer_idx * VIA_MATRIX_ROWS * VIA_MATRIX_COLS * 2
                    + row * VIA_MATRIX_COLS * 2 + col * 2;
                write_byte(addr, 0);
                write_byte(addr + 1, 0);
            }
        }
    }
}
