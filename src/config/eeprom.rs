//! Emulated EEPROM using the last page of STM32F072 flash memory.
//!
//! The last 2KB flash page (page 63, address 0x0801F800) is used to store
//! a small user config block. This survives power cycles but has limited
//! write endurance (~10k cycles per page).
//!
//! Config layout (8 bytes at offset 0):
//!   [0] = magic flag (0xA5 = valid)
//!   [1] = side_mode
//!   [2] = side_brightness (light)
//!   [3] = side_speed
//!   [4] = side_colour
//!   [5] = side_rgb (bool)
//!   [6] = sleep_enable (bool)
//!   [7] = reserved

use stm32f0xx_hal::pac;

/// Config page address: last 2KB page (page 63 of 64, 128K flash)
const CONFIG_PAGE_ADDR: u32 = 0x0801_F800;
const MAGIC_VALID: u8 = 0xA5;

/// User config stored in flash
#[derive(Debug, Clone, Copy)]
pub struct UserConfig {
    pub side_mode: u8,
    pub side_brightness: u8,
    pub side_speed: u8,
    pub side_colour: u8,
    pub side_rgb: bool,
    pub sleep_enable: bool,
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
        }
    }
}

/// Load user config from emulated EEPROM.
/// Returns None if flash page is uninitialized (magic byte invalid).
pub fn load() -> Option<UserConfig> {
    let addr = CONFIG_PAGE_ADDR as *const u8;
    unsafe {
        if *addr != MAGIC_VALID {
            return None;
        }
        let ptr = addr.add(1);
        Some(UserConfig {
            side_mode:       *ptr,
            side_brightness: *ptr.add(1),
            side_speed:      *ptr.add(2),
            side_colour:     *ptr.add(3),
            side_rgb:        *ptr.add(4) != 0,
            sleep_enable:    *ptr.add(5) != 0,
        })
    }
}

/// Save user config to emulated EEPROM.
/// Erases the config page first, then writes 8 bytes.
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

            // 4. Program 4 halfwords (8 bytes)
            flash.cr.modify(|_, w| w.pg().set_bit());

            let ptr = CONFIG_PAGE_ADDR as *mut u16;
            // Halfword 0: magic (low) | side_mode (high)
            let hw0 = MAGIC_VALID as u16 | ((cfg.side_mode as u16) << 8);
            *ptr = hw0;
            while flash.sr.read().bsy().bit() {}

            // Halfword 1: brightness | speed
            let hw1 = (cfg.side_brightness as u16) | ((cfg.side_speed as u16) << 8);
            *ptr.add(1) = hw1;
            while flash.sr.read().bsy().bit() {}

            // Halfword 2: colour | side_rgb
            let hw2 = (cfg.side_colour as u16) | ((cfg.side_rgb as u16) << 8);
            *ptr.add(2) = hw2;
            while flash.sr.read().bsy().bit() {}

            // Halfword 3: sleep_enable | reserved
            let hw3 = cfg.sleep_enable as u16;
            *ptr.add(3) = hw3;
            while flash.sr.read().bsy().bit() {}

            flash.cr.modify(|_, w| w.pg().clear_bit());

            // 5. Lock flash
            flash.cr.modify(|_, w| w.lock().set_bit());
        }
    });
}
