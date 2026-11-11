//! Persistent storage: two logical 2KB pages, each kept in two flash slots.
//!
//!   page 63 (config): [0..16) user + RGB matrix config,
//!                     [16..2032) VIA dynamic keymap
//!   page 62 (VIA):    [0..4) VIA magic + layout options,
//!                     [4..2036) VIA macro buffer
//!   both:             [2040..2046) slot footer (magic, sequence number)
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
/// Flash slots per logical page. Slot 0 is where the single-copy layout
/// (<= v4.9.0) kept the page, so existing settings are read from there.
#[cfg(all(target_arch = "arm", not(test)))]
const SLOT_BASE: [[u32; 2]; 2] = [
    [0x0801_F000, 0x0801_E000], // VIA
    [0x0801_F800, 0x0801_E800], // config
];
/// Footer: magic (u32 LE) then sequence number (u16 LE), written last.
const FOOTER: usize = PAGE_SIZE - 8;
const FOOTER_MAGIC: u32 = 0xA96B_C0DE;
/// Quiet time before collected changes are written (one erase per burst).
const FLUSH_DELAY_MS: u32 = 500;

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
// Power-loss safe: an update erases the older slot, programs the page there
// and writes the footer last, so an interrupted update leaves the previous
// copy current. With no valid footer in either slot (never written by this
// layout) slot 0 is read as-is.
//
// Low wear: changes collect in a 2KB RAM copy of one page and reach flash
// after FLUSH_DELAY_MS without another change, or on `flush()`. A VIA keymap
// or macro upload (~70 writes of 28 bytes) costs one erase instead of ~70.
// Changes made in the last FLUSH_DELAY_MS before power loss are lost.

/// Unwritten changes for one logical page.
struct Cache {
    page: Option<usize>,
    buf: [u8; PAGE_SIZE],
    /// Set by each change; `task()` restarts the flush delay on it.
    touched: bool,
    since: u32,
}

impl Cache {
    const fn new() -> Self {
        Self { page: None, buf: [0; PAGE_SIZE], touched: false, since: 0 }
    }
}

#[cfg(all(target_arch = "arm", not(test)))]
fn flash_read(page: usize, slot: usize, off: usize) -> u8 {
    unsafe { core::ptr::read_volatile((SLOT_BASE[page][slot] + off as u32) as *const u8) }
}

/// Erase the slot, program `buf[..FOOTER]` (halfword writes, erased
/// halfwords skipped), then the footer: sequence, then magic. Interrupts stay
/// enabled, as in QMK's legacy_flash_ops.c: the USB interrupt runs between
/// halfword writes (it only stalls while the flash is busy). No interrupt
/// handler touches the flash.
#[cfg(all(target_arch = "arm", not(test)))]
fn flash_program(page: usize, slot: usize, buf: &[u8; PAGE_SIZE], seq: u16) {
    let base = SLOT_BASE[page][slot];
    let magic = FOOTER_MAGIC.to_le_bytes();
    unsafe {
        let flash = &*pac::FLASH::ptr();
        flash.keyr.write(|w| w.bits(0x4567_0123));
        flash.keyr.write(|w| w.bits(0xCDEF_89AB));
        while flash.sr.read().bsy().bit() {}

        flash.cr.modify(|_, w| w.per().set_bit());
        flash.ar.write(|w| w.bits(base));
        flash.cr.modify(|_, w| w.strt().set_bit());
        while flash.sr.read().bsy().bit() {}
        flash.cr.modify(|_, w| w.per().clear_bit());

        flash.cr.modify(|_, w| w.pg().set_bit());
        let ptr = base as *mut u16;
        let write = |i: usize, hw: u16| {
            if hw != 0xFFFF {
                core::ptr::write_volatile(ptr.add(i), hw);
                while flash.sr.read().bsy().bit() {}
            }
        };
        for i in 0..FOOTER / 2 {
            write(i, u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]));
        }
        write((FOOTER + 4) / 2, seq);
        write((FOOTER + 2) / 2, u16::from_le_bytes([magic[2], magic[3]]));
        write(FOOTER / 2, u16::from_le_bytes([magic[0], magic[1]]));
        flash.cr.modify(|_, w| w.pg().clear_bit());
        flash.cr.modify(|_, w| w.lock().set_bit());
    }
}

#[cfg(all(target_arch = "arm", not(test)))]
fn with_cache<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    static mut CACHE: Cache = Cache::new();
    // Single-threaded main loop; never re-entered (no flash access from ISRs).
    f(unsafe { &mut *core::ptr::addr_of_mut!(CACHE) })
}

// Host builds: per-thread flash and cache, so each test starts erased.
#[cfg(not(all(target_arch = "arm", not(test))))]
std::thread_local! {
    static HOST_FLASH: core::cell::RefCell<[[[u8; PAGE_SIZE]; 2]; 2]> = const { core::cell::RefCell::new([[[0xFF; PAGE_SIZE]; 2]; 2]) };
    static HOST_CACHE: core::cell::RefCell<Cache> = const { core::cell::RefCell::new(Cache::new()) };
    static HOST_PROGRAMS: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

#[cfg(not(all(target_arch = "arm", not(test))))]
fn flash_read(page: usize, slot: usize, off: usize) -> u8 {
    HOST_FLASH.with(|f| f.borrow()[page][slot][off])
}

#[cfg(not(all(target_arch = "arm", not(test))))]
fn flash_program(page: usize, slot: usize, buf: &[u8; PAGE_SIZE], seq: u16) {
    HOST_PROGRAMS.with(|n| n.set(n.get() + 1));
    HOST_FLASH.with(|f| {
        let s = &mut f.borrow_mut()[page][slot];
        s.fill(0xFF);
        s[..FOOTER].copy_from_slice(&buf[..FOOTER]);
        s[FOOTER..FOOTER + 4].copy_from_slice(&FOOTER_MAGIC.to_le_bytes());
        s[FOOTER + 4..FOOTER + 6].copy_from_slice(&seq.to_le_bytes());
    });
}

#[cfg(not(all(target_arch = "arm", not(test))))]
fn with_cache<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    HOST_CACHE.with(|c| f(&mut c.borrow_mut()))
}

/// Sequence number of a slot with a complete footer.
fn slot_seq(page: usize, slot: usize) -> Option<u16> {
    let r = |i: usize| flash_read(page, slot, FOOTER + i);
    let magic = u32::from_le_bytes([r(0), r(1), r(2), r(3)]);
    let seq = u16::from_le_bytes([r(4), r(5)]);
    (magic == FOOTER_MAGIC && seq != 0xFFFF).then_some(seq)
}

/// The slot holding the current copy of `page`, and its sequence number.
fn current_slot(page: usize) -> (usize, Option<u16>) {
    match (slot_seq(page, 0), slot_seq(page, 1)) {
        (Some(a), Some(b)) if (b.wrapping_sub(a) as i16) > 0 => (1, Some(b)),
        (Some(a), _) => (0, Some(a)),
        (None, b) => (b.is_some() as usize, b),
    }
}

fn page_read(page: usize, off: usize) -> u8 {
    with_cache(|c| (c.page == Some(page)).then(|| c.buf[off])).unwrap_or_else(|| flash_read(page, current_slot(page).0, off))
}

/// Write collected changes now (before a reset, or for `erase()`).
pub fn flush() {
    with_cache(|c| {
        if let Some(page) = c.page.take() {
            let (slot, seq) = current_slot(page);
            let next = seq.map_or(0, |s| s.wrapping_add(1));
            flash_program(page, 1 - slot, &c.buf, if next == 0xFFFF { 0 } else { next });
        }
    });
}

/// Main-loop hook: write collected changes once FLUSH_DELAY_MS pass
/// without another change.
pub fn task(now: u32) {
    let due = with_cache(|c| {
        if c.touched {
            c.touched = false;
            c.since = now;
        }
        c.page.is_some() && now.wrapping_sub(c.since) >= FLUSH_DELAY_MS
    });
    if due {
        flush();
    }
}

/// Set `len` bytes at `off` from `src(i)` (which must not read storage);
/// nothing is queued when no byte changes.
fn page_update_with(page: usize, off: usize, len: usize, src: impl Fn(usize) -> u8) {
    if with_cache(|c| c.page.is_some_and(|p| p != page)) {
        flush();
    }
    with_cache(|c| {
        if c.page != Some(page) {
            let slot = current_slot(page).0;
            for (i, b) in c.buf.iter_mut().enumerate() {
                *b = flash_read(page, slot, i);
            }
        }
        let mut changed = false;
        for i in off..off + len {
            let v = src(i - off);
            changed |= c.buf[i] != v;
            c.buf[i] = v;
        }
        if changed {
            c.page = Some(page);
            c.touched = true;
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
    flush();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn programs() -> u32 {
        HOST_PROGRAMS.with(|n| n.get())
    }

    /// Flash survives ~10k erases per page: a VIA upload of many 28-byte
    /// chunks must cost one erase, and only after the writes go quiet.
    #[test]
    fn burst_of_writes_is_one_erase() {
        for i in 0..72u16 {
            ee_update(48 + i * 28, &[i as u8; 28]);
        }
        assert_eq!(programs(), 0);
        assert_eq!(ee_read(48 + 71 * 28), 71, "reads see unwritten changes");
        task(1000);
        task(1000 + FLUSH_DELAY_MS - 1);
        assert_eq!(programs(), 0, "flushed before the quiet period");
        task(1000 + FLUSH_DELAY_MS);
        assert_eq!(programs(), 1);
        assert_eq!(ee_read(48 + 71 * 28), 71);
    }

    /// Power loss mid-update (footer not yet written) must leave the
    /// previous copy in use, not a half-erased keymap.
    #[test]
    fn interrupted_update_keeps_previous_copy() {
        ee_update(48, &[0x12, 0x34]);
        flush();
        ee_update(48, &[0x56, 0x78]);
        flush();
        let (slot, _) = current_slot(PAGE_CONFIG);
        // Tear the newest write: its slot was erased and partly programmed.
        HOST_FLASH.with(|f| f.borrow_mut()[PAGE_CONFIG][slot][FOOTER..].fill(0xFF));
        assert_eq!([ee_read(48), ee_read(49)], [0x12, 0x34]);
    }

    /// Settings written by the single-copy layout (slot 0, no footer) are
    /// read and carried into the first new write.
    #[test]
    fn legacy_page_is_read_and_migrated() {
        HOST_FLASH.with(|f| {
            let s = &mut f.borrow_mut()[PAGE_CONFIG][0];
            s[0] = MAGIC_VALID_V5;
            s[16] = 0xAB; // keymap byte
        });
        assert!(load().is_some());
        keymap_config_save(0x1234);
        flush();
        assert_eq!(current_slot(PAGE_CONFIG), (1, Some(0)));
        assert_eq!(ee_read(48), 0xAB);
        assert_eq!(keymap_config_load(), 0x1234);
    }

    /// Writes alternate slots, so each page wears half as fast.
    #[test]
    fn writes_alternate_slots() {
        let mut slots = vec![];
        for v in 0..4u8 {
            ee_update(48, &[v]);
            flush();
            slots.push(current_slot(PAGE_CONFIG).0);
        }
        assert_eq!(slots, [1, 0, 1, 0]);
        erase();
        assert_eq!(ee_read(48), 0xFF);
        assert!(load().is_none());
    }
}
