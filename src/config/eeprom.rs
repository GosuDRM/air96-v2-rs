//! Persistent storage in the last two 2KB flash pages.
//!
//!   page 63 (0x0801_F800): [0..16) user + RGB matrix config,
//!                          [16..2032) VIA dynamic keymap
//!   page 62 (0x0801_F000): [0..4) VIA magic + layout options,
//!                          [4..2036) VIA macro buffer
//!
//! VIA code addresses this storage with the C VIA build's EEPROM addresses
//! (`ee_read` / `ee_update`): VIA magic at 44, layout options at 47, dynamic
//! keymap at 48..2064, macros at 2064..4096 — see `via.rs`.
//!
//! User config record (page 63, bytes 0..16):
//!   [0] magic (0xA9 = current, 0xA5 = v1), [1] side_mode, [2] side_brightness,
//!   [3] side_speed, [4] side_colour, [5] side_rgb, [6] sleep_enable,
//!   [7] rgb_mode, [8] rgb_hue, [9] rgb_sat, [10] rgb_val, [11] rgb_speed,
//!   [12] rgb_enabled, [13..15) keymap_config (QMK magic), [15] reserved

#[cfg(all(target_arch = "arm", not(test)))]
use stm32f0xx_hal::pac;

const PAGE_SIZE: usize = 2048;
const PAGE_CONFIG: usize = 1; // page 63
const PAGE_VIA: usize = 0; // page 62
#[cfg(all(target_arch = "arm", not(test)))]
const PAGE_BASE: [u32; 2] = [0x0801_F000, 0x0801_F800];

const MAGIC_VALID_V1: u8 = 0xA5;
const MAGIC_VALID_V5: u8 = 0xA9;

/// User config stored in flash.
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
            rgb_mode: 12,   // CYCLE_LEFT_RIGHT — C RGB_MATRIX_DEFAULT_MODE
            rgb_hue: 255,   // C first-boot rgb_matrix_sethsv(255, …)
            rgb_sat: 255,
            rgb_val: 151,   // 255 − RGB_MATRIX_VAL_STEP*2
            rgb_speed: 127, // RGB_MATRIX_DEFAULT_SPD
            rgb_enabled: true,
        }
    }
}

// ── Flash backend ────────────────────────────────────────────────────────
//
// A page update reads the page into one 2KB scratch buffer, patches it and
// reprograms the page. On the MCU the buffer is a static: a 2KB stack array
// does not fit next to the main loop in 16KB of RAM.

#[cfg(all(target_arch = "arm", not(test)))]
fn page_read(page: usize, off: usize) -> u8 {
    unsafe { core::ptr::read_volatile((PAGE_BASE[page] + off as u32) as *const u8) }
}

/// Erase the page and program it from `buf` (halfword writes, erased
/// halfwords skipped). Interrupts stay enabled, as in QMK's
/// legacy_flash_ops.c: the USB interrupt runs between halfword writes (it only
/// stalls while the flash is busy). No interrupt handler touches the flash.
#[cfg(all(target_arch = "arm", not(test)))]
fn page_program(page: usize, buf: &[u8; PAGE_SIZE]) {
    unsafe {
        let flash = &*pac::FLASH::ptr();
        flash.keyr.write(|w| w.bits(0x4567_0123));
        flash.keyr.write(|w| w.bits(0xCDEF_89AB));
        while flash.sr.read().bsy().bit() {}

        flash.cr.modify(|_, w| w.per().set_bit());
        flash.ar.write(|w| w.bits(PAGE_BASE[page]));
        flash.cr.modify(|_, w| w.strt().set_bit());
        while flash.sr.read().bsy().bit() {}
        flash.cr.modify(|_, w| w.per().clear_bit());

        flash.cr.modify(|_, w| w.pg().set_bit());
        let ptr = PAGE_BASE[page] as *mut u16;
        for i in 0..PAGE_SIZE / 2 {
            let hw = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
            if hw != 0xFFFF {
                core::ptr::write_volatile(ptr.add(i), hw);
                while flash.sr.read().bsy().bit() {}
            }
        }
        flash.cr.modify(|_, w| w.pg().clear_bit());
        flash.cr.modify(|_, w| w.lock().set_bit());
    }
}

#[cfg(all(target_arch = "arm", not(test)))]
fn with_scratch<R>(f: impl FnOnce(&mut [u8; PAGE_SIZE]) -> R) -> R {
    static mut SCRATCH: [u8; PAGE_SIZE] = [0; PAGE_SIZE];
    // Single-threaded main loop; never re-entered (no flash access from ISRs).
    f(unsafe { &mut *core::ptr::addr_of_mut!(SCRATCH) })
}

// Host builds: per-thread pages, so each test starts from erased flash.
#[cfg(not(all(target_arch = "arm", not(test))))]
std::thread_local! {
    static HOST_PAGES: core::cell::RefCell<[[u8; PAGE_SIZE]; 2]> = const { core::cell::RefCell::new([[0xFF; PAGE_SIZE]; 2]) };
}

#[cfg(not(all(target_arch = "arm", not(test))))]
fn page_read(page: usize, off: usize) -> u8 {
    HOST_PAGES.with(|p| p.borrow()[page][off])
}

#[cfg(not(all(target_arch = "arm", not(test))))]
fn page_program(page: usize, buf: &[u8; PAGE_SIZE]) {
    HOST_PAGES.with(|p| p.borrow_mut()[page] = *buf);
}

#[cfg(not(all(target_arch = "arm", not(test))))]
fn with_scratch<R>(f: impl FnOnce(&mut [u8; PAGE_SIZE]) -> R) -> R {
    let mut buf = [0u8; PAGE_SIZE];
    f(&mut buf)
}

/// Read-modify-write `len` bytes at `off` from `src(i)`; skips the flash
/// write when nothing changes.
fn page_update_with(page: usize, off: usize, len: usize, src: impl Fn(usize) -> u8) {
    with_scratch(|buf| {
        let mut changed = false;
        for (i, b) in buf.iter_mut().enumerate() {
            let old = page_read(page, i);
            *b = if i >= off && i < off + len { src(i - off) } else { old };
            changed |= *b != old;
        }
        if changed {
            page_program(page, buf);
        }
    });
}

fn page_update(page: usize, off: usize, data: &[u8]) {
    page_update_with(page, off, data.len(), |i| data[i]);
}

// ── User config ──────────────────────────────────────────────────────────

/// Load the user config; None if never written.
pub fn load() -> Option<UserConfig> {
    let r = |i: usize| page_read(PAGE_CONFIG, i);
    match r(0) {
        MAGIC_VALID_V5 => Some(UserConfig {
            side_mode: r(1),
            side_brightness: r(2),
            side_speed: r(3),
            side_colour: r(4),
            side_rgb: r(5) != 0,
            sleep_enable: r(6) != 0,
            rgb_mode: r(7),
            rgb_hue: r(8),
            rgb_sat: r(9),
            rgb_val: r(10),
            rgb_speed: r(11),
            rgb_enabled: r(12) != 0,
        }),
        MAGIC_VALID_V1 => Some(UserConfig {
            side_mode: r(1),
            side_brightness: r(2),
            side_speed: r(3),
            side_colour: r(4),
            side_rgb: r(5) != 0,
            sleep_enable: r(6) != 0,
            ..Default::default()
        }),
        _ => None,
    }
}

/// Save the user config, keeping the dynamic keymap that shares the page.
pub fn save(cfg: &UserConfig) {
    let rec = [
        MAGIC_VALID_V5, cfg.side_mode, cfg.side_brightness, cfg.side_speed,
        cfg.side_colour, cfg.side_rgb as u8, cfg.sleep_enable as u8, cfg.rgb_mode,
        cfg.rgb_hue, cfg.rgb_sat, cfg.rgb_val, cfg.rgb_speed,
        cfg.rgb_enabled as u8,
    ];
    page_update(PAGE_CONFIG, 0, &rec);
}

/// `eeconfig_read_keymap()`: keymap_config.raw at record bytes 13..15
/// (`eeconfig_init_quantum()` default 0x1400 when erased).
pub fn keymap_config_load() -> u16 {
    let raw = page_read(PAGE_CONFIG, 13) as u16 | (page_read(PAGE_CONFIG, 14) as u16) << 8;
    if raw == 0xFFFF {
        crate::action::KEYMAP_CONFIG_DEFAULT
    } else {
        raw
    }
}

/// `eeconfig_update_keymap()`.
pub fn keymap_config_save(raw: u16) {
    page_update(PAGE_CONFIG, 13, &raw.to_le_bytes());
}

/// `eeconfig_init()` / `eeconfig_disable()`: forget every stored setting,
/// VIA data included.
pub fn erase() {
    for page in [PAGE_CONFIG, PAGE_VIA] {
        page_update_with(page, 0, PAGE_SIZE, |_| 0xFF);
    }
}

// ── QMK EEPROM address space used by VIA ─────────────────────────────────

/// QMK EEPROM regions (start, end, page, offset in page).
const REGIONS: [(u16, u16, usize, usize); 3] = [
    (44, 48, PAGE_VIA, 0),       // VIA magic (3) + layout options (1)
    (48, 2064, PAGE_CONFIG, 16), // dynamic keymap
    (2064, 4096, PAGE_VIA, 4),   // macro buffer
];

/// `eeprom_read_byte()` at a QMK EEPROM address (VIA region only).
pub fn ee_read(addr: u16) -> u8 {
    REGIONS
        .iter()
        .find(|r| addr >= r.0 && addr < r.1)
        .map_or(0xFF, |r| page_read(r.2, r.3 + (addr - r.0) as usize))
}

/// `eeprom_update_byte()` for a run of bytes; one page rewrite per region touched.
pub fn ee_update(addr: u16, data: &[u8]) {
    ee_fill(addr, data.len() as u16, |i| data[i as usize]);
}

/// Like `ee_update()` with bytes from `src(i)`, i = 0..len (no source buffer).
pub fn ee_fill(addr: u16, len: u16, src: impl Fn(u16) -> u8) {
    let mut i = 0u16;
    while i < len {
        let a = addr + i;
        let Some(r) = REGIONS.iter().find(|r| a >= r.0 && a < r.1) else {
            i += 1;
            continue;
        };
        let n = (r.1 - a).min(len - i);
        let base = i;
        page_update_with(r.2, r.3 + (a - r.0) as usize, n as usize, |k| src(base + k as u16));
        i += n;
    }
}
