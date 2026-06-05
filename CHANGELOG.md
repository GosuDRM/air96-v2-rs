# Changelog

## v4.7.8 (2026-06-06)

Fn-layer hotkey parity with the C firmware — side-LED and wireless-link keys now behave 1:1.

### Fixed

- **Side-LED speed keys were reversed.** `SIDE_SPI` (Fn speed-up) made the side animation *slower* and `SIDE_SPD` made it *faster*. Both firmwares index the same `side_speed_table`; C's `light_speed_control` decrements `side_speed` on speed-up (faster) and increments on speed-down. Swapped to match.
- **Wireless-link keys switched away from USB.** `LNK_RF`/`LNK_BLE1..3` (Fn+4/1/2/3) had no guard, so they'd drop a wired session and go wireless. C ignores these presses while `link_mode == USB`; added the guard.
- **Link-key release ignored which key was pressed.** The release path didn't check `ch == rf_sw_temp`, so holding one link key and tapping another could switch to the wrong channel (the case C's own comment warns about). Added the check.
- **`SIDE_HUI` colour cycle differed in non-wave modes.** C's `side_colour_control` strips rainbow first when the side mode isn't Wave; Rust didn't, so leaving rainbow in Static/Breath/Mix landed on red instead of orange. Added the pre-step.
- **`LNK_USB` re-sent `CMD_SET_LINK` even when already on USB.** Now gated on `link_mode != USB` (the side blink stays unconditional), matching C.

### Verified correct (no change)

- **BLE pairing long-press** — the handler runs every tick (1 ms), so its `3000` threshold = 3 s, exactly C's `RF_LONG_PRESS_DELAY` (60) at its 50 ms cadence. (Clarified the misleading comment.)
- All `MAC_*` macros (incl. NKRO `Cmd+Shift+S` vs `Cmd+Shift+4`), `RF_DFU`, `DEV_RESET`, `SLEEP_MODE`, `BAT_SHOW`, `RGB_TEST`, `BAT_NUM`, `SIDE_VAI/VAD/MOD`, and the RGB_* keys.

### Files

- `src/main.rs` — `SIDE_SPI/SPD` swap, `LNK_*` USB guard + release `ch == rf_sw_temp` check, `SIDE_HUI` non-wave pre-step, `LNK_USB` re-link guard
- `Cargo.toml` / `README.md` / `CHANGELOG.md` — bumped to 4.7.8

## v4.7.7 (2026-06-06)

RGB-matrix animation parity with the C/QMK firmware — speed, brightness, colour math, mode IDs, and per-effect behaviour now match.

> **Upgrading resets RGB/side settings once.** The EEPROM magic is bumped (0xA8 → 0xA9): the mode renumber and corrected defaults make old saved configs invalid, so the keyboard re-initialises to the C-matching defaults on first boot. Re-pick your effect / side-light settings after flashing.

### Fixed — global (every effect)

- **Animations ran ~1.76× too fast.** Default speed was 223; C uses `RGB_MATRIX_DEFAULT_SPD` = `UINT8_MAX/2` = **127**.
- **Too bright out of the box.** Default value was 223; C first-boot is `255 − RGB_MATRIX_VAL_STEP*2` = **151** (`ansi.c:683`).
- **Brightness/speed keys stepped by 16; C uses 52** (`RGB_MATRIX_VAL_STEP`/`SPD_STEP` from `keyboard.json`). `RGB_VAI/VAD/SPI/SPD` now step 52.
- **HSV→RGB was a generic algorithm.** Rewrote `hsv_to_rgb` bit-for-bit to QMK `color.c` (`region = h*6/255`, `(h*2 − region*85)*3` remainder, `>>8` scaling, region-6 fold) — colours now match exactly, including the hue-boundary sectors that were visibly off.
- **VIA selected the wrong effect.** The Rust dispatch used an alphabetical mode order while the shipped VIA definition (and C) use the QMK enum order. Re-numbered the dispatch to the QMK/VIA IDs (0 = Off, 1 = Solid, … 12 = Cycle Left/Right, … 40 = Solid MultiSplash); `RGB_MOD` now cycles in C order and wraps 40 → 1; default mode is 12.
- **Reactive hit memory was 20; C `LED_HITS_TO_REMEMBER` = 8** — splash/reactive density now matches.

### Fixed — per effect

- **Splash / Solid Splash / Reactive Cross / Nexus / Wide** reacted to *all* remembered keys, making them identical to their `Multi…` twins. They now use only the most recent key (`effect_runner_reactive_splash` start = count−1).
- **Dual Beacon, Rainbow Beacon, Rainbow Pinwheels** — `sin`/`cos` were swapped (C `effect_runner_sin_cos_i` passes `cos` into the `sin` slot), rotating/mirroring the pattern; also dropped a stray `+1` on the `speed/4` time factor.
- **Breathing, Hue Breathing** (`speed/8`) and **Cycle Out/In** (`speed/2`) — removed a stray `+1` that made them drift at speed 0 and run slightly fast.
- **Raindrops** — hue came from a hue-dependent sawtooth instead of C's shortest-path `((hue+180)%360 − hue)/4`.
- **Typing Heatmap** — decay counted render frames (~400 ms) instead of milliseconds; now fades at C's 25 ms cadence.

### Changed

- The 9 QMK effects this keyboard never enabled (alpha_mods, flower_blooming, pixel_rain/flow/fractal, riverflow, starlight×3) are no longer selectable — they aren't in C or the VIA menu. Their render code remains but is unreachable.

### Files

- `src/led/rgb.rs` — `hsv_to_rgb` (QMK-exact), `HIT_BUFFER = 8`, `next_mode` (QMK step), C-matching `new()` defaults
- `src/led/animation.rs` — dispatch re-ordered to QMK/VIA IDs, `compute_time_nobias` for the no-`+1` runners, splash single-hit, sin/cos un-swap, raindrops hue, heatmap ms decay
- `src/main.rs` — VAL/SPD step 52, solid-mode checks (`== 1`), heatmap keypress (`== 27`), reset defaults
- `src/config/eeprom.rs` — magic 0xA8 → 0xA9, C-matching defaults
- `src/test.rs` — `eeprom_defaults` updated to the new defaults
- `Cargo.toml` / `README.md` / `CHANGELOG.md` — bumped to 4.7.7

## v4.7.6 (2026-06-05)

LEDs now actually turn off at PC shutdown — the v4.7.2 hardware-kill was self-reversing.

### Fixed

- **LEDs stayed lit after PC shutdown.** The v4.7.2 "hardware LED kill" cut `DC_BOOST`/`SDB1`/`SDB2` on the USB-suspend edge, but the co-located `just_resumed` branch re-powered them on *any* exit-from-Suspend (`main.rs`). At shutdown the host emits a resume/reset transient while tearing the bus down, which flips the device out of `Suspend` exactly once and re-raises the rails — and since USB here is poll-only with no further clean suspend (the host is gone), nothing ever cut them again, so the board ended rails-HIGH with a static lit frame. The off is now **latched** (`leds_killed`), mirroring the C firmware's `f_wakeup_prepare` + `key_wake`/`usb_wake` model: once the rails are cut for suspend/sleep they stay down until a *genuine* wake — a local keypress (C's `key_wake`, `no_act_time < 10`) or the 1 s-debounced host resume (`usb_wake`, `sleep.rs:86`). A bare resume/reset transient no longer re-powers them, so all three shutdown signatures (stays-suspended; suspend→reset→`Default`; suspend→resume-blip→dead) end dark and stay dark. The CPU also halts (`wfi`) while killed-but-not-yet-sleep-latched so a VBUS-live powered-off host doesn't busy-spin.

### Files

- `src/main.rs` — `leds_killed` latch: set on the suspend-edge and 1 s-sleep rail cut, cleared only on a key-driven or host-driven wake; `just_resumed` re-powers the rails only when `no_act_time < 10`; keypress-wake now armed by the latch (not just the sleep flag); `wfi` while killed
- `Cargo.toml` / `README.md` / `CHANGELOG.md` — bumped to 4.7.6

## v4.7.5 (2026-06-05)

Parity fixes from C↔Rust audit — closes the gaps the v4.7.4 baseline left open.

### Fixed

- **VIA `EEPROM_RESET` did not reset the dynamic keymap.** QMK's `eeconfig_init_via()` resets layout options *and* the dynamic keymap. Rust's `ID_EEPROM_RESET` handler only called `eeprom::reset_to_defaults()` — the keymap layer 0..8 stayed whatever the user had set. `ID_EEPROM_RESET` now also calls `via_dynamic_keymap_reset()`.
- **VIA `DYNAMIC_KEYMAP_RESET` did not zero layers 5..7.** Rust's reset init'd the 2016-byte buffer to `0xFF` (uninitialised) and then wrote layers 0..4 from the static `LAYER_*` tables — leaving layers 5..7 as `0xFFFF`, which the read path treats as "use static keymap" (so users could see stale keys on those layers). Now initialised to `0x00` so layers 5..7 are `KC_NO` (matching QMK's `keycode_at_keymap_location_raw` for undefined layers).
- **Side LED `SIDE_HUI` cycle never returned to rainbow.** Rust's handler did `(self.side.colour + 1) % 8`, so the cycle was Red→Orange→…→Lavender→Red with no rainbow state. C reference's `side_colour_control()` uses two variables: `side_rgb` (rainbow flag) and `side_colour` (palette 0..7). The cycle is 0..7 (colours) → 8 (rainbow) → 0 (red). Rust already had `rgb_enabled` as the `side_rgb` equivalent; the handler now increments `colour`, transitions to `rgb_enabled=true, colour=0` after 7, and resets back to `colour=0` on the next press.
- **Caps Lock side-LED was white instead of cyan.** C reference's `sys_led_show()` writes `0x00, SIDE_BLINK_LIGHT, SIDE_BLINK_LIGHT` = `(0, 128, 128)` cyan when Caps Lock is on. Rust had `(0xFF, 0xFF, 0xFF)` white.
- **Full-charge battery indicator never started the breath.** The 5-second `bat_show_time` timeout in `bat_led_show`'s "stable charge state" branch cleared `bat_show_flag` before the "charge == 0x03" branch could trigger the breath. C reference uses a `bat_full_shown` rising-edge latch and resets it on leaving `0x03` — same fix applied: a single breath-trigger fires when the charger first reports full, and stays latched until the device leaves full-charge.
- **Main RGB defaults were eye-searing.** `UserConfig::default()` and the `KC_DEV_RESET` path used `hue=0, val=255, speed=255` — max brightness, fastest, no hue. Now `hue=255, val=223, speed=223` (a sensible "set and forget" baseline matching the C reference's `rgb_matrix_sethsv(255, 255, 255 - 52*2)` first-boot override, with one tick back from max for comfortable long-term use).

### Changed

- **`render_multisplash` OOB is now structurally impossible** (matches `render_solid_multisplash`). The old form iterated `for j in 0..count` with an in-loop `if j >= hit_index.len() { break; }` guard — defensive but noisy. The new form is `min(count, 20)` + a `(count - count + k) % 20` index, the same pattern the solid variants use, so the compiler can prove no OOB.

### Skipped / No-op

- **`SystemControlReport16` is `u16` on purpose** — QMK's `report_extra_t` is also 2 bytes (`uint16_t usage`); descriptor min/max range 0x81..0xB7 requires > 8 bits. Rust matches.
- **Num Lock RGB LED 33→76** — C reference's `rgb_matrix_indicators_kb()` doesn't drive per-LED caps/num highlighting on the main matrix at all (it uses the side LEDs in `side.c:300`). Rust's index 33 for num lock is a Rust-only feature, not a port gap.

### Files

- `src/via.rs` — `ID_EEPROM_RESET` + `via_dynamic_keymap_reset()` init buffer with `0x00`
- `src/main.rs` — `KC_SIDE_HUI` cycle, `KC_DEV_RESET` defaults
- `src/config/eeprom.rs` — `UserConfig::default()` defaults
- `src/led/side.rs` — Caps Lock color (white→cyan), `bat_led_show` rising-edge latch (`bat_full_shown`)
- `src/led/animation.rs` — `render_multisplash` uses `min(count, 20)` + wraparound indexing
- `Cargo.toml` / `README.md` / `CHANGELOG.md` — bumped to 4.7.5

## v4.7.4 (2026-06-05)

Wired keyboard responds to keypresses again — v4.7.3 regression fix.

### Fixed

- **Wired keyboard dead — no key responds after v4.7.3.** The boot-keyboard refactor switched the keyboard interface to IN-only (`HIDClass::new_ep_in_with_settings`), which forces the host to deliver lock-LED state (NumLock / CapsLock / …) via `SET_REPORT` on the control pipe instead of an interrupt OUT endpoint. `usbd-hid` 0.6.2's `SET_REPORT` handler then panics: `let mut buf: [u8; CONTROL_BUF_LEN] = [0; 128]; buf.copy_from_slice(&xfer.data()[..len])` — `copy_from_slice` requires equal-length slices, and `len` is `1` for a boot keyboard LED report (`hid_class.rs:677-678`). With `panic = "abort"` and a `wfe` panic handler, the firmware silently halts the moment the host sets initial LED state at enumeration (or on the first NumLock/CapsLock toggle): matrix scanning stops, USB polling stops, no key responds. Fixed upstream in usbd-hid 0.7+, but 0.7 requires usb-device 0.3 while `stm32-usbd` 0.6 pins usb-device 0.2, so the crate can't simply be bumped. Switched the keyboard interface back to IN+OUT (`HIDClass::new_with_settings`) — Windows / Linux / macOS deliver LED state on the OUT endpoint (`pull_raw_output`), bypassing the buggy SET_REPORT path entirely. This mirrors the v4.7.2 endpoint shape; the v4.7.3 boot subclass/protocol + ForceBoot + no-Report-ID descriptor (which fixed BIOS recognition) are preserved unchanged. Total endpoints: 5 IN + 2 OUT + EP0 — 576 B of the STM32F072's 1024-byte PMA, 5 of 8 EP indices used.

### Files

- `src/usb_hid.rs` — `keyboard` constructed via `new_with_settings` (IN+OUT, Boot/Keyboard/ForceBoot); LED state read via `pull_raw_output` instead of the panicking `pull_raw_report`; long header comment documenting the upstream bug so the OUT endpoint isn't accidentally dropped again
- `Cargo.toml` / `src/usb_hid.rs` / `README.md` / `CHANGELOG.md` — bumped to 4.7.4

## v4.7.3 (2026-06-05)

Wired keyboard now works in BIOS / UEFI / pre-OS — proper HID boot keyboard.

### Fixed

- **Keyboard dead in BIOS / UEFI setup / bootloaders (wired mode)** — the USB keyboard interface was not a USB-spec boot keyboard, so pre-OS firmware (which does not parse the HID report descriptor) never drove it. Two defects: (1) the interface advertised `bInterfaceSubClass=0` / `bInterfaceProtocol=0` instead of `1`/`1` (Boot/Keyboard), so BIOS didn't recognise it as a keyboard at all; (2) every report was prefixed with a Report ID byte (`0x01` for 6KRO, `0x02` for NKRO), but boot protocol reads a fixed `[modifiers, reserved, key0..key5]` with **no Report ID** — BIOS would read the `0x01` Report-ID as the modifier byte and shift every keycode. Wired mode also defaulted to NKRO (Report ID 2), which pre-OS firmware cannot read in any form. The keyboard is now a clean boot keyboard: subclass/protocol `1`/`1`, `ForceBoot`, no Report ID, fixed 8-byte 6KRO report — matching what QMK presents. Works in the BIOS setup screen, boot menu, and disk-encryption prompts.

### Changed

- **USB HID restructured into four interfaces** — boot keyboard (IN-only; host LED state via `SET_REPORT` on the control pipe), VIA RAW HID (own interface, no Report ID, IN+OUT), consumer (IN-only), system (IN-only). Consumer/system dropped their unused OUT endpoints to keep the new boot-keyboard + VIA interfaces within the STM32F072 endpoint-memory budget (the constraint that broke v3.7.0 and v4.4.0).
- **USB is now 6KRO, not NKRO.** `usbd-hid` gates transmission on a single per-interface protocol mode and exposes no way to read the host's `SET_PROTOCOL` choice on a `ForceBoot` interface, so QMK-style dynamic 6KRO(boot)/NKRO(report) switching on one endpoint isn't possible with this crate. Boot protocol is pinned for universal BIOS + OS compatibility. **NKRO over wireless is unchanged.**

### Files

- `src/usb_hid.rs` — `BOOT_KEYBOARD_DESC` + `VIA_DESC` (no Report ID); boot keyboard via `new_ep_in_with_settings(Boot/Keyboard/ForceBoot)`; VIA on its own interface; consumer/system IN-only; `send_keyboard`/`send_nkro` emit the 8-byte boot report
- `src/test.rs` — replaced the merged-descriptor test with boot-keyboard + VIA descriptor assertions (94 total)
- `Cargo.toml` / `src/usb_hid.rs` / `README.md` / `CHANGELOG.md` — bumped to 4.7.3

## v4.7.2 (2026-06-05)

Windows 11 USB enumeration fix + hardware LED-off at shutdown.

### Fixed

- **Windows 11 "USB device not recognized"** — the polled USB stack asserts the D+ pull-up the moment `UsbHid::new()` builds the device (`stm32-usbd` sets `BCDR.DPPU` inside `UsbBus::enable()`, called from `UsbDeviceBuilder::build()`), which tells the host to enumerate. But USB was brought up ~200 ms before the polling loop — ahead of the 100 ms settle delay, EEPROM load, dial scan and the RF handshake (`CMD_HAND`/`CMD_SET_NAME`/`CMD_SET_LINK` with 5 ms gaps), none of which call `usb_hid.poll()`. The device was deaf to the host's `GET_DESCRIPTOR`/`SET_ADDRESS` control transfers for that whole window, so Windows 11 — the least patient host during enumeration — exhausted its retries and declared the device unrecognised, while Linux/macOS retried long enough to catch the loop once it finally polled. Moved USB init to the last statement before the main loop so the pull-up asserts only once continuous polling begins; the unpolled window drops to ~0. The RF handshake now supplies the pre-pull-up settle, so the old 100 ms `asm::delay` is removed.
- **LEDs stayed lit after PC shutdown** — the v4.7.0 render gate zeroes the PWM *buffer* but never cuts LED *power*. The rail power-down only happened via the 1 s USB-suspend debounce, which resets on any suspend flicker — and a board that fails to enumerate (above) never gets a clean, stable suspend, so `is_suspended()` never latched for a full second and `DC_BOOST`/`SDB1`/`SDB2` stayed powered. The suspend edge now cuts those three rails immediately (hardware kill, mirroring C `Sleep_Handle`'s `writePinLow`) and the resume edge restores them when RGB is enabled — independent of the 1 s debounce. The sleep-handler power-down still runs afterward as the complementary NRF shutdown. Relies on the enumeration fix to deliver a stable suspend signal at shutdown.

### Files

- `src/main.rs` — USB init relocated to immediately before the main loop; LED-rail hardware kill on the USB-suspend edge + restore on resume
- `Cargo.toml` / `src/usb_hid.rs` / `README.md` / `CHANGELOG.md` — bumped to 4.7.2

## v4.7.1 (2026-06-04)

Sleep/wake audit — disable-sleep toggle, USB-active guard, and USB remote wakeup.

### Added

- **USB remote wakeup** — a keypress now wakes a sleeping PC. The device advertises `supports_remote_wakeup`, so the host arms `DEVICE_REMOTE_WAKEUP` before suspending; on a keypress while suspended the firmware drives `CNTR.RESUME` (K-state, ~5 ms) then releases it — the same mechanism as C `usb_lld_wakeup_host`. usb-device 0.2 / stm32-usbd expose no device-initiated resume, so this is direct USB-peripheral register access. **Note:** the exact register/timing sequence is not yet hardware-validated; it only affects keypress-to-wake — normal suspend/resume does not depend on it.

### Fixed

- **"Disable sleep" (Fn+ScrLk / `KC_SLEEP_MODE`) did nothing in wireless mode** — `SleepManager.sleep_enabled` was never synced from the user toggle (it stayed `true`), and the RF-connected idle branch didn't check it at all. After ~6 min idle on BT/2.4 GHz the board slept (LEDs off, WFI) regardless of the setting, while the side-LED indicator falsely showed sleep disabled. Now synced each tick and gated on the connected-idle sleep, matching C `Sleep_Handle` (USB-suspend / disconnect / link-timeout still sleep regardless, to save battery).
- **Spurious sleep while actively on USB** — the goto-sleep handler lacked C's guard that rejects a sleep request when USB is the active link and the host hasn't suspended the bus. A stray `CMD_24G_SUSPEND` from the NRF would cut the LED rails + WFI mid-use until the next keypress. Added the guard.
- **Key held across suspend could linger as a phantom after wake** — on a host-driven USB resume (no recent keypress) the firmware now clears key state, mirroring C `m_break_all_key` on `!key_wake`. A key-driven wake keeps its press so the key that woke the host still registers.

### Files

- `src/wireless/sleep.rs` — USB-active sleep guard + `sleep_enabled` gate on the connected-idle sleep
- `src/usb_hid.rs` — `supports_remote_wakeup` + `remote_wakeup()` (device-initiated resume)
- `src/main.rs` — sync `dev.sleep.sleep_enabled` each tick; remote wakeup on keypress-while-suspended; stale-key flush on host-driven resume
- `src/test.rs` — guard + sleep-disable regression tests (94 total)
- `Cargo.toml` / `CHANGELOG.md` — bumped to 4.7.1

## v4.7.0 (2026-06-04)

USB-suspend LEDs now actually stay off at PC shutdown — completes the v4.5.0 fix.

### Fixed

- **LEDs stayed lit after PC shutdown/suspend** — v4.5.0 zeroed the RGB matrix on the suspend edge, but the main loop kept rendering: `tick_animation()` (every 16 ms) and `side.update()` (every 1 ms) repainted the buffer, and `build_pwm_buffers()` re-lit the Caps/Num indicator LEDs (55/33) — so the dark frame was overwritten within ~16 ms and the LEDs never went dark. The C reference avoids this by parking its whole main loop in a suspend spin (`chibios.c:181`) so no rendering runs while `USB_DRIVER.state == USB_SUSPENDED`. Replicated that with a persistent `leds_suspended` gate: the side/animation/indicator render block is skipped while the host holds the bus suspended, the suspend edge also clears `caps_lock`/`num_lock`, and resume repaints (mode 0 via `set_hsv`, animated modes refill on the next tick). Gated on USB link mode so a charge-only Suspend in wireless mode can't blank the board. The 1 s sleep-handler GPIO power-down still runs as a complementary power saver.
- **macOS area screenshot (`MAC_PRTA`) sent the wrong key** — the non-NKRO branch sent `0x23` (`KC_6`) instead of `0x21` (`KC_4`), so on macOS (which runs with NKRO off) the area-screenshot key fired `Cmd+Shift+6` (a no-op) instead of `Cmd+Shift+4`. The Windows/NKRO path (`Win+Shift+S`) was already correct. Caught by a full C-vs-Rust keymap + handler diff across all five layers.

### Files

- `src/main.rs` — persistent USB-suspend render gate (`leds_suspended`) + mode-aware resume repaint; `MAC_PRTA` non-NKRO keycode `0x23` → `0x21`
- `Cargo.toml` / `src/usb_hid.rs` / `CHANGELOG.md` — bumped to 4.7.0

## v4.6.0 (2026-06-03)

VIA support — wired remapping of the dynamic keymap and live RGB matrix control.

### Added

- **VIA configuration** — `via::via_command` handles VIA protocol v12 (0x000C) over the existing boot-kbd HIDClass on a new third collection sharing its OUT endpoint (Report ID 3, vendor usage page 0xFF60, 32-byte IN/OUT). `UsbHid::poll` demuxes OUT reports by Report ID and dispatches Report ID 3 to `via_command`. The descriptor adds 36 bytes to `COMBINED_KEYBOARD_DESC` (107 → 143) and widens the LED/RAW buffer to 33 bytes (Report ID + 32).
- **Dynamic keymap (EEPROM)** — `ID_DYNAMIC_KEYMAP_GET_KEYCODE`, `ID_DYNAMIC_KEYMAP_SET_KEYCODE`, `ID_DYNAMIC_KEYMAP_GET_BUFFER`, `ID_DYNAMIC_KEYMAP_SET_BUFFER`, `ID_DYNAMIC_KEYMAP_GET_LAYER_COUNT`, `ID_DYNAMIC_KEYMAP_RESET` are wired to the 2 KB config page at offset 16 (right after the 16-byte `UserConfig` block). `keymap::resolve_keycode` consults `via::dynamic_keymap_get_keycode` first; uninitialized slots (0xFFFF) fall back to the compiled keymap. Block writes use a new `eeprom::save_keymap` that does one page erase + 2 KB write per change (~100 ms), replacing the old per-byte `eeprom::write_byte` for the keymap area.
- **Live RGB matrix control (channel 3)** — `id_qmk_rgb_matrix_brightness/effect/effect_speed/color` read and write `RgbMatrix.{val, mode, speed, hue, sat}` in place, so the firmware's animation step picks up the change on the next tick. `ID_CUSTOM_SAVE` flips the existing `save_pending` flag; the main loop's deferred-save block then flushes the (already-mirrored) `dev.rgb.*` to flash.
- **`ID_EEPROM_RESET`**, **`ID_BOOTLOADER_JUMP`**, **`ID_GET_PROTOCOL_VERSION`**, **`ID_GET_KEYBOARD_VALUE`** (uptime, layout options, firmware version, switch matrix state) — all return the correct response. Firmware version is now derived from `env!("CARGO_PKG_VERSION")` so it stays in sync with `Cargo.toml` across releases.
- **Host-test stubs** — `eeprom::{read_byte, write_byte, save_keymap}` and the new `via::via_command` are split into `#[cfg(target_arch = "arm")]` and `#[cfg(not(...))]` arms so the 92 host tests don't crash on flash addresses that don't exist on x86.

### Files

- `src/via.rs` — new, 416 lines (was orphaned since v3.7.0 revert, now wired)
- `src/usb_hid.rs` — descriptor append + dispatch in `poll()`
- `src/config/eeprom.rs` — `cfg` guards for host + new `save_keymap`
- `src/keyboard/keymap.rs` — `resolve_keycode` consults the dynamic keymap first
- `src/main.rs` — passes `&mut dev.rgb, &mut dev.save_pending` to `usb_hid.poll`
- `Cargo.toml` / `CHANGELOG.md` — bumped to 4.6.0

## v4.5.0 (2026-06-02)

Immediate USB suspend LED-off — matches C reference's `suspend_power_down_kb` hook.

### Fixed

- **LEDs stay on for 1s after PC shutdown** — The Rust port's `Sleep_Handle` was waiting for the 1s USB suspend debounce before powering off `dc_boost`/`sdb1`/`sdb2`, so the RGB matrix kept writing the live animation to the (still-powered) LED drivers during that window. The C version's QMK `suspend_power_down_kb` hook writes all-zero PWM to the IS31FL3733 on the same cycle the host signals suspend, then `Sleep_Handle` adds the GPIO power-down 1s later. Added a latched suspend/resume edge in `UsbHid::poll()` + `take_suspend_edge()` and a handler in the main loop that zeros the RGB matrix and marks both drivers dirty the instant the edge fires. The 1s debounce still owns the actual GPIO power-down and wakeup state machine — only the visual "off" cue is now immediate.

## v4.3.0 (2026-06-02)

Bug audit — critical animation and sleep fixes.

### Fixed

- **Reactive animations break after 20 key presses** — `hit_count` overflowed the 20-entry ring buffer, causing all single-hit reactive effects (modes 33, 34, 35, 37, 39, 41, 43) to stop working. Fixed with ring-buffer-aware iteration.
- **Digital rain never spawns drops** — `digital_rain_drop == 0` check was unreachable after increment. Changed to `== 1`.
- **Rapid sleep/wake cycling** — `usb_suspend_debounce` not reset on wakeup, causing immediate re-sleep when USB is suspended with key activity.
- **DND system key broken in USB mode** — `SystemControlReport16` descriptor expects 1-based values but code sent raw HID usage codes. Fixed by converting `0x9B` → `27` for USB path.
- **periodic_timer never resets in USB mode** — timer accumulated indefinitely, causing unpredictable first wireless keepalive on mode switch.
- **Both dirty flags set for single lock change** — Caps Lock only needs `dirty1` (driver 0), Num Lock only needs `dirty2` (driver 1). Eliminated unnecessary I2C writes.
- **render_cycle_out_in_dual overflow** — i16→i8 cast corrupted distance for right-side LEDs (px > 217). Fixed with i16 math throughout.
- **Test assertions corrected** — Fixed 4 tests with wrong expected values matching actual keymap layout.

### Changed

- **Release profile: debug=false, panic=abort** — Smaller binaries, faster link times. Kept opt-level="s" for binary size (128KB flash limit).

## v4.2.0 (2026-06-02)

Smooth RGB animation and BLE pairing fixes.

### Changed

- **Smooth RGB animation** — `anim_tick` changed from `u16` (+45 per 20ms) to `u32` real wall-clock milliseconds (matching C `g_rgb_timer`). Frame rate increased from 50 to 62.5 FPS (20ms → 16ms). Eliminates visible stepping in breathing, cycle, and reactive effects.
- **BLE pairing from USB mode** — Removed `link_mode != Usb` gate on LNK_BLE/LNK_RF keycodes. Fn+1/2/3/4 now works regardless of current mode (short tap = channel switch, hold 3s = pairing).
- **Long-press threshold** — Moved `rf_sw_press_delay` from 50ms timer block to 1ms tick path. Threshold 60 now = 60ms (tap) vs 3000ms (pairing), matching C firmware's `RF_LONG_PRESS_DELAY=60` at 50ms intervals.

## v4.1.9 (2026-06-02)

Chunked I2C flush to eliminate keystroke hiccups during rapid typing.

### Changed

- **Chunked RGB LED writes** — Each IS31FL3733 192-byte PWM write split into 3×64-byte chunks. Max I2C blocking per loop iteration reduced from ~1.7ms to ~0.5ms. Matrix scan runs between chunks, so fast keypresses that previously landed inside the I2C window are no longer missed.

## v4.1.8 (2026-06-02)

Split I2C flush across two loop iterations.

### Changed

- **Split driver flush** — Instead of writing both IS31FL3733 drivers in one ~3.5ms block, each driver flushes in a separate loop iteration. Max blocking reduced to ~1.7ms.

## v4.1.7 (2026-06-02)

Initial latency investigation — verified firmware delivers events to kernel within ~2ms.

### Notes

- Used `evtest` to confirm kernel timestamps show <2ms from physical keypress to evdev event. The 80ms latency seen on clickspeedtester.com is key hold duration (how long the user physically holds the key), not firmware latency.

## v4.1.6 (2026-06-02)

Input-latency pass.

### Fixed

- **Media/system-key latency** — Consumer and System HID endpoints polled at 8ms; lowered to 1ms so all three HID endpoints match C's `USB_POLLING_INTERVAL_MS 1`. (The keyboard endpoint was already 1ms.)

### Notes

- The dominant key-press latency was the SysTick 8× clock bug fixed in v4.1.3 — effective debounce was 40ms, now 5ms (QMK `sym_eager_pk`, 0ms first-event). With that flashed, the USB regular-key path is ~1–2ms end to end (≤1ms scan + 0ms eager debounce + ≤1ms USB poll), matching the C reference. **If keys still feel laggy, make sure v4.1.3+ is actually flashed.**
- On wireless (BLE / 2.4G) the remaining latency is the nRF's RF connection interval, which lives in the nRF firmware, not this MCU.

## v4.1.5 (2026-06-02)

Another +50% animation speed, and the Fn-layer hotkeys are now 1:1 with the C reference.

### Changed

- **Animation speed +50% again** — `anim_tick` step raised from 30 to 45 per 20ms tick (~2.25× the real-ms base). Stacks on the v4.1.4 boost.
- **Fn hotkeys 1:1 with C reference** — Re-laid the Mac-Fn, Win-Fn, and Fn (layer 4) maps to match `keyboards/air96_v2/ansi/keymaps/default/keymap.c` exactly (anchored by base-layer keycode, since the Rust matrix has gaps the C `LAYOUT` macro hides):
  - `fn+,` = `RGB_SPD`, `fn+.` = `RGB_SPI` (speed down/up) — previously on the arrows.
  - `fn+←` = `RGB_MOD`, `fn+→` = `RGB_HUI`, `fn+↑` = `RGB_VAI`, `fn+↓` = `RGB_VAD`.
  - `fn+\` = `BAT_SHOW` (was `RGB_MOD` with a tap-hold); `fn+[` = `DEV_RESET`, `fn+]` = `SLEEP_MODE` (unchanged).
  - Layer 4 side controls shifted onto `←`/`↓`/`→` (`SIDE_MOD`/`SIDE_VAD`/`SIDE_HUI`) and `,`/`.`/`↑` (`SIDE_SPD`/`SIDE_SPI`/`SIDE_VAI`); `MO(4)` corrected onto LSFT/M/RSFT and right-Fn.
  - `RGB_HUI` step 16 → 8 and `RGB_MOD` reverted to plain next-effect-on-press, matching QMK defaults.

## v4.1.4 (2026-06-02)

RGB animation now defaults to maximum speed, plus a global +50% speed boost.

### Changed

- **Default RGB speed → max** — `rgb_speed` default raised from 127 to 255 (max) in `UserConfig::default()`, `RgbMatrix::new()`, and the `DEV_RESET` handler.
- **Global +50% animation speed** — `tick_animation()` advances `anim_tick` by 30 instead of 20 per 20ms tick (×1.5). Every effect's `time` is linear in `anim_tick`, so all 50 modes and reactive decay speed up uniformly — on top of the max speed setting.
- **EEPROM magic V3 → V4 (0xA7 → 0xA8)** — DFU flashing doesn't erase the config page (flash page 63, past the firmware), so a persisted V3 config would keep the old speed=127. Bumping the magic invalidates the saved config on first boot so the new max-speed default actually applies.

## v4.1.3 (2026-06-02)

Fixed the SysTick clock source — the actual root cause of the "static" RGB animation and stretched debounce/sleep timings.

### Fixed

- **SysTick running 8x too slow (static RGB animation)** — `main()` configured SysTick with `set_reload(47999)` for a 1ms tick but never called `set_clock_source(SystClkSource::Core)`. The cortex-m reset default is the *external reference* clock, which on STM32F0 is HCLK/8 (6MHz), so every "1ms" tick was actually **8ms**. The RGB animation gate (`rgb_anim_timer >= 20`) therefore fired every 160ms instead of 20ms, so a `CYCLE_LEFT_RIGHT` sweep took ~16s (and ~5.5min on the prior `anim_tick += 1` build) — visually frozen. Added `cp.SYST.set_clock_source(SystClkSource::Core)` so the tick is a true 1ms, matching the C reference's `g_rgb_timer`/`sync_timer` and `stm32f0xx-hal`'s own `Delay`. This also un-stretches every other ms-based timing (the "5ms" debounce lockout was really 40ms — the same-key latency chased in v4.1.2; "3s" DFU hold was 24s; init/sleep/RF-sync delays were all 8x long).

## v4.1.2 (2026-06-02)

Restored QMK `sym_eager_pk` debouncing and fixed RGB matrix defaults to match the C reference firmware.

### Fixed

- **Same-Key Latency** — Reverted `matrix::scan()` from `sym_defer_pk` (10ms stability timer per edge) back to QMK's `sym_eager_pk` (0ms first-event latency, 5ms per-key lockout). With `sym_defer_pk`, a release→press cycle faster than 10ms silently dropped the second press because the stability counter was reset by the re-press before it could expire. `sym_eager_pk` matches `quantum/debounce/sym_eager_pk.c` from the C reference (`keyboards/air96_v2/ansi/keyboard.json` `DEBOUNCE=5`) — the first scan that detects a change emits the event immediately, and a 5ms lockout absorbs bounce. Rapid press→release→press cycles now register each edge as it happens (bounce-filtered only within the 5ms lockout window).
- **RGB Matrix Defaults** — Freshly flashed RGB now matches the C reference (`quantum/rgb_matrix/rgb_matrix.h:52-82`). Default mode changed from solid_color (0) to `CYCLE_LEFT_RIGHT` (4). Default HSV changed from (255, 255, 223) to (0, 255, 255). Default speed changed from 223 to 127. EEPROM `UserConfig::default()` and the `DEV_RESET` handler also updated to match. EEPROM magic bumped from V2 (0xA6) to V3 (0xA7) to force a config reset on first boot after flash, ensuring old EEPROM values (mode=0, hue=255) don't override the new defaults.
- **RGB Animation Not Running** — The I2C PWM flush was gated on `idle` (no keys pressed for 20ms), so the RGB animation was invisible during typing and had a 20ms startup delay. Removed the idle gate so the flush runs unconditionally every 10ms, matching C `rgb_matrix_task()` which flushes every 16ms regardless of activity. Removed dead `idle_timer` variable.
- **RGB Animation Frozen** — `anim_tick` incremented by 1 every 20ms, but `compute_time()` and reactive effects expect millisecond units (matching C `g_rgb_timer = sync_timer_read32()`). The animation was 20x slower than intended — a full rainbow cycle took 41 seconds instead of 2 seconds, appearing completely static. Changed increment from 1 to 20 so `anim_tick` tracks milliseconds, matching the C reference.

### Changed

- **Debounce Core** — Extracted the per-key decision into a pure `eager_pk_step(pressed, debounced_bit, ctr) -> (new_bit, new_ctr, Option<bool>)` helper so the algorithm is unit-testable without hardware pins. `scan()` updates `raw` then calls the helper for each key; `tick_debounce()` is now a pure counter decrementor (no event generation, no debounced-bit mutation).

### Added

- **Debounce Unit Tests** — `matrix_sym_eager_pk_*` tests cover eager event on first change, lockout suppression of bounce, post-lockout release, the C reference's `OneKeyShort1` press-release-press sequence, and counter underflow clamping.

## v4.0.2 (2026-05-26)

Fully implemented deferred stability debouncing, fixed the wireless Num Lock indicator, and optimized sleep manager and EEPROM loaders.

### Fixed

- **Wireless Num Lock Indicator** — Mapped the side LED Num Lock status to also read NRF co-processor RF LED status, ensuring the indicator reflects Num Lock state in both wired and wireless modes.
- **EEPROM Volatile Loads** — Refactored the emulated EEPROM load block to use `core::ptr::read_volatile`, matching the flash write safety of `save()`.

### Changed

- **Debounce Algorithm Implementation** — Fully implemented the Symmetric Deferred Per-Key (`sym_defer_pk`) stability-based debouncing algorithm described in `v4.0.1` by storing raw matrix state and generating events from the 1ms SysTick handler.
- **Sleep Manager Cleanup** — Cleaned up `SleepManager::tick` return signature to return `()` instead of unused wrapper vectors, removing unused imports.

### Added

- **Debounce Unit Tests** — Added `matrix_sym_defer_pk_debouncing()` to assert stability counter countdown, noise/bounce cancellation, and press/release event generation.

## v4.0.1 (2026-05-26)

Rewrote debounce algorithm from eager lockout to deferred stability for reliable slow-key detection.

### Changed

- **Debounce Algorithm** — Changed from sym_eager_pk (instant trigger + lockout) to sym_defer_pk (stability timer). `scan()` now detects raw state changes and initializes a per-key stability counter. `tick_debounce()` decrements counters and generates events only when the key has been stable for the full debounce window. This eliminates phantom press/release cycles during slow key transitions.
- **Debounce Window** — Increased from 5ms to 10ms to accommodate slower mechanical switch settling times.

### Added

- **Debounce Unit Tests** — `matrix_sym_defer_pk_debouncing()` validates counter decrement, event generation on expiry, and noise filtering.

## v4.0.0 (2026-05-26)

Aligned keyboard Fn hotkeys with physical legends on the Air96 V2 keycaps, and implemented Tap-Hold timer logic for FN + backslash to cycle backlight styles / show battery.

### Fixed

- **Physical Icon Layout Alignment** — Mapped Sidelight Toggle (`MO | 4` layer shift) to the **M** key (which has the sidelight keycap legend) on both Mac and Windows Fn layers, replacing the misplaced `FN + N` shortcut. Moved the main Backlight Effects cycle keycode (`KC_RGB_MOD`) to the backslash (**\\**) key, matching its physical backlight legend.
- **Tap-Hold Backlight / Battery Shortcut** — Added `rgb_mod_press` hold-tracking timer logic in `src/main.rs`. Tapping `FN + \\` (<300ms) cycles main backlight styles and initializes the active color on solid mode immediately, while holding it ($\ge 300\text{ ms}$) lights up the side battery status LED color-coded level, fading back when released. This prevents battery indicators from flashing on rapid taps.
- **Enhanced Test Coverage** — Updated the `mac_fn_rgb_controls` and `win_fn_rgb_controls` unit tests to explicitly assert these legend-aligned physical mappings.

## v3.9.9 (2026-05-26)

Added support for loading, applying, and saving main RGB matrix settings (mode, hue, sat, val, speed, enabled) in the emulated EEPROM, resolving startup default behavior and lack of animations across power cycles.

### Fixed

- **Main RGB Settings Load & Restore** — Configured the startup loader to parse and assign `rgb_mode`, `rgb_hue`, `rgb_sat`, `rgb_val`, `rgb_speed`, and `rgb_enabled` from stored flash configuration block into active `dev.rgb` fields. Includes support for shutting down physical boost/LED drivers at startup if RGB is configured as disabled.
- **Main RGB Settings Save** — Configured the deferred EEPROM save block to serialize active RGB settings into the 16-byte `V2` layout.
- **RGB Control Keycode Triggers** — Added immediate `save_config()` triggers to all main RGB matrix control keycodes (`KC_RGB_SPD`, `KC_RGB_SPI`, `KC_RGB_VAI`, `KC_RGB_VAD`, `KC_RGB_MOD`, `KC_RGB_HUI`) to flag changes for the deferred EEPROM writer.
- **Enhanced Test Coverage** — Updated the `eeprom_defaults` unit test to explicitly check and assert the correct QMK-standard RGB fields on `UserConfig::default()`.

## v3.9.8 (2026-05-26)

Optimized wireless mode standby battery consumption by halting the CPU in standby sleep mode.

### Fixed

- **Standby CPU Sleep Optimization (WFI)** — Integrated a low-power assembly halt (`cortex_m::asm::wfi()`) at the end of the main loop when in deep sleep standby state (`f_wakeup_prepare` is true). Halting the CPU prevents continuous high-power active spinning, reducing standby CPU current draw by **over 99%** (to under 50µA) while preserving instant, sub-millisecond wakeup latency when any key is pressed.

## v3.9.7 (2026-05-26)

Updated the side LED battery indicator color scheme thresholds to map Orange to 21-50% and Green to >50%.

### Changed

- **Side LED Battery Indicator Color Thresholds** — Shifted the battery color scheme ranges: Red remains ≤ 20%, Orange now represents medium levels (21-50%), and Green signifies healthy levels (>50%, previously >95%).
- **Enhanced Test Coverage** — Updated the `side_battery_indicator_color_coding` unit test to explicitly assert green color mapping and full segment counts for above-50% charge states (e.g. 75%).

## v3.9.6 (2026-05-26)

Fixed the manual battery status hotkey shortcut and added rigorous unit test coverage for the color-coded battery indicator levels and animation overrides.

### Fixed

- **Manual Battery Indicator Shortcut** — Passed the custom battery show flag `f_bat_hold` from the keycode handler into `SideLeds::update` and `bat_led_show`. Pressing or toggling the battery shortcut (Fn + `\` / `KC_BAT_SHOW`) now successfully displays the color-coded battery level on the right side LED as intended, overriding normal side animations.
- **Rigorously Tested Indicator Logic** — Added comprehensive unit tests (`side_battery_indicator_color_coding` and `side_battery_indicator_hold_override`) verifying the correct color coding (Red ≤ 20%, Orange 21-95%, Green > 95%), LED segment counts, and non-destructive animation hold overrides.

## v3.9.5 (2026-05-26)

Resolved sleep-mode battery drain, enhanced flash write safety for emulated EEPROM, and restored host unit test compilation support.

### Fixed

- **Sleep Battery Drain** — Implemented sleep/wakeup GPIO power transitions for the Boost Converter (`dc_boost`) and LED drivers (`rgb_sdb1`, `rgb_sdb2`). They are now powered off completely during sleep, resolving excessive battery drain.
- **Zero-Latency Wakeup** — Added immediate keypress wakeup detection in the matrix scan loop that restores power rails and wakes up the hardware instantly (<1ms), bypassing the 50ms periodic timer.
- **USB Host Resume** — Updated the sleep wakeup state machine to detect when the USB host resumes from suspend and automatically wakes the keyboard.
- **Flash Write Safety** — Refactored the emulated EEPROM flash programming writes in `src/config/eeprom.rs` to use `core::ptr::write_volatile`, preventing compiler reordering or elimination of flash status checks and registers.
- **Host Test Harness** — Conditionally compiled the bare-metal `#[panic_handler]` in `src/main.rs` with `#[cfg(not(test))]` to resolve duplicate lang item conflicts when compiling host-based unit tests.

## v3.9.4 (2026-05-26)

Fixed Bluetooth pairing hold bug and resolved lost keystrokes/overwritten UART reports.

### Fixed

- **Bluetooth Pairing Hold** — Shifted `dev_sts_sync` status sync logic from 50ms block to a dedicated 200ms `t200` block. This reduces UART traffic by 4x and increases sync loss reset window from 250ms to 1000ms (1.0 second), preventing false NRF52 co-processor resets during long pairing/link establishment sequences.
- **Lost Keystrokes & Overwritten Reports** — Replaced delayed scan-end flushing with a closure-based immediate UART flushing architecture. This ensures standard, consumer, system, and hotkey reports are transmitted instantly when built. Mutually decoupled NKRO bit reports (`CMD_RPT_BIT_KB`) and standard boot reports (`CMD_RPT_BYTE_KB`) in `send_nkro` to prevent them from overwriting each other in the shared `tx_buf` array.
- **Stale Press Timer** — Added reset of `rf_sw_press_delay = 0` on switch key press to prevent stale counter values.

## v3.9.3 (2026-05-26)

Remapped fn+arrow keys for intuitive RGB control.

### Changed

- **fn+Arrow RGB Mapping** — fn+UP = brightness up (VAI), fn+DOWN = brightness down (VAD), fn+LEFT = speed down (SPD), fn+RIGHT = speed up (SPI). Mode change (MOD) moved to fn+8. Previously fn+DOWN was mode change and fn+RIGHT was brightness down.

## v3.9.2 (2026-05-26)

Fixed missed keystrokes — lockout debounce was double-decrementing causing premature expiry.

### Fixed

- **Double Decrement Debounce** — `scan()` decrements lockout counter per scan (~180µs), and `tick_debounce()` was ALSO decrementing per millisecond. With lockout=28, effective debounce was ~2.5ms — too short. Keys could press+release within one scan window, producing zero net events visible to USB. Fixed by making `tick_debounce()` a no-op; lockout is now purely scan-based at 28 scans ≈ 5ms.
- **Lockout Resolution** — Changed from tick-based (1ms) to scan-based (~180µs) to match the continuous matrix scan rate introduced in v3.8.2.

## v3.9.1 (2026-05-26)

Fixed Bluetooth/2.4GHz pairing — NRF module must receive CMD_SET_LINK before CMD_NEW_ADV.

### Fixed

- **Pairing Root Cause** — Hold handler was sending `CMD_NEW_ADV` directly without first sending `CMD_SET_LINK` to switch the NRF module to the target channel. The NRF rejects pairing commands on the wrong channel. Added `CMD_SET_LINK` + flush + 20ms wait before the pairing retry loop, matching the dev_reset handler pattern.
- **Pairing Visual Feedback** — Added `side.blink_rf(3)` to hold handler (pairing starts) and tap handler (channel switch), matching the USB mode switch feedback.

## v3.9.0 (2026-05-26)

Fixed Bluetooth/2.4GHz pairing long-press timer and Caps/Num Lock hardware PWM override.

### Fixed

- **Pairing Long-Press Timer** — `rf_sw_press_delay` changed from 60 to 20. Counter runs in 50ms block, so 60×50ms=3s was required to enter pairing. Now 20×50ms=1s for all fn+1/2/3/4 modes.
- **Caps/Num Lock PWM Override** — LED 55 (Caps) and LED 33 (Num) forced to white at hardware PWM level. Survives brightness=0 and animations.
- **Host LED Report Parsing** — `pull_raw_output()` added for reliable LED state capture.

## v3.8.8 (2026-05-26)

Fixed Caps Lock / Num Lock LED indicators — hardware PWM override ensures key LEDs glow white regardless of RGB brightness setting.

### Fixed

- **Caps Lock Key LED** — LED index 55 (Caps Lock key) forced to white (0xFF,0xFF,0xFF) in `build_pwm_buffers()` when Caps Lock is active. Overrides any RGB profile, animation, or brightness=0 setting.
- **Num Lock Key LED** — LED index 33 (Num Lock key) forced to white when Num Lock is active. Same hardware-level override.
- **Host LED Report Parsing** — `pull_raw_report()` replaced with `pull_raw_report()` + `pull_raw_output()` to reliably capture SET_REPORT and OUT endpoint LED state from the host.
- **Side LED Ordering** — Side LED output copied to RGB buffer AFTER animation tick so animations don't overwrite indices 100-109.
- **Wireless LED Sync** — RF LED bits (`rf_led & 0x01`/`0x02`) merged with USB host LED state for Caps/Num Lock detection in both wired and wireless modes.

## v3.8.7 (2026-05-26)

Fixed hotkey behavior to match the C firmware 1:1.

### Fixed

- **MAC_SEARCH / MAC_PRT / MAC_PRTA Tap Behavior** — Changed from hold-release to tap-only (press+immediate release) to match the C firmware's `register_code`/`unregister_code` pattern.
- **Win Fn Layer Keymap** — Fixed row 0 col 14 from `KC_MAC_PRT` (full screenshot) to `KC_MAC_PRTA` (area screenshot) to match the C firmware.
- **DEV_RESET Key Break** — Added `break_all_keys()` call on DEV_RESET press to match C firmware behavior.
- **DEV_RESET Long Press 2.4GHz Preservation** — Added inner guard `if link_mode != Rf24` so device reset preserves the 2.4GHz RF mode instead of always resetting to BT1.

## v3.8.6 (2026-05-26)

Fixed wireless mode issues (Bluetooth pairing, channel switching, wireless switch button, 2.4GHz).

### Fixed

- **USART1 Parity Configuration Lockout** — Modified the USART1 initialization to disable the peripheral (`UE = 0`) before configuring parity control (`PCE = 1`) and word length (`M0 = 1`), preventing the configuration from being locked out and ignored by the hardware, and enabling communication with the NRF52 module.
- **Robust Blocking UART RX Loops** — Implemented a safe `uart_wait_rx!(dev, ms)` macro with 2ms transmission gap detection to replace broken inline blocking loops, ensuring incomplete frames are not prematurely discarded during blocking resets, pairing, or channel transitions.

## v3.8.5 (2026-05-26)

Fixed the DFU bootloader jump logic.

### Fixed

- **DFU Bootloader Interrupts & Pointers** — Added a `cpsie i` instruction to re-enable interrupts globally right before the jump to the bootloader, ensuring its USB interrupts can execute and enumerate. Also replaced UB-prone null/dangling pointer reads with explicit raw memory reads for the bootloader's initial SP and Reset Vector.

## v3.8.4 (2026-05-26)

Fixed the 64ms minimum keystroke latency limit by increasing the USB polling rate.

### Changed

- **1000Hz USB Polling Rate** — Changed the USB HID keyboard endpoint polling interval from `64ms` to `1ms`. This allows the host to request reports at a 1000Hz frequency, reducing the minimum registered keypress duration from 64ms down to under 1ms.

## v3.8.3 (2026-05-26)

Fixed microsecond keystroke stuttering/stalls when rapid key spamming, and resolved wireless mode UART packet receiving issues.

### Fixed

- **USB Report Collisions** — Changed report dispatching to only send either NKRO or standard boot keyboard reports (instead of both concurrently) when a key event occurs, avoiding endpoint congestion, `WouldBlock` drop-outs, and host-side input device de-synchronization.
- **I2C Typing Deferral** — Added a 20ms continuous idle check before performing RGB matrix I2C flushes (`pwm_flush!`), preventing the 3.5ms blocking I2C operations from stalling the CPU during active typing/key-spamming gaps.
- **Side LED Update Speed** — Moved the side LED update tick check inside the 1ms timing block to prevent it from spinning out of control under high-speed continuous matrix scanning.
- **UART RX Framing** — Implemented an idle-line gap detection timeout (2ms) for the UART receiver instead of resetting the frame parser immediately on USART read blockages, restoring full packet parsing capabilities in wireless mode.
- **Clippy Cleanups** — Resolved warnings related to saturating arithmetic for RGB speed/value increments and use of `.is_empty()` for length comparisons.

## v3.8.2 (2026-05-26)

Optimized keypress scanning latency and resolved host-side test compilation issues.

### Added

- **Continuous Matrix Scanning** — Decoupled the key matrix scan and USB HID poll from the 1ms SysTick timer, allowing them to spin as fast as possible to minimize latency.
- **`tick_debounce()`** — Added a dedicated tick-based debounce counter decrement method to preserve the 5ms key debounce lockout window under continuous matrix scanning.

### Fixed

- **Host Test Compilation** — Target-gated ARM inline assembly instructions (`msr`, `bx`) in `enter_bootloader` and `jump_to_bootloader` to compile only on ARM architecture target, allowing `cargo test` to build and run warning-free on the host.

## v3.8.1 (2026-05-26)

Fixed stuck keys and auto-repeat in wired USB NKRO mode.

### Fixed

- **USB Report Block Queue** — Pushing standard (Report ID 1) and NKRO (Report ID 2) reports in quick succession on a single shared endpoint caused the second push to return `WouldBlock` and be silently discarded, causing stuck keys on simultaneous press/release events. Implemented non-blocking report retry queues in `UsbHid`. Retries occur automatically on subsequent SysTick poll loops until reports are successfully sent.
- **NKRO Descriptor Realignment** — Corrected NKRO payload format to use a 31-byte key bitmap with direct `k / 8` byte offset mapping and modifiers completely decoupled into a separate byte. This resolves a +8 keycode shift and duplicate modifier bits.

## v3.8.0 (2026-05-26)

Merged USB NKRO keyboard interface into the standard boot keyboard interface using Report IDs to resolve STM32F072 endpoint/interface enumeration limit issues.

### Added

- **USB NKRO Support** — Full NKRO (up to 248 simultaneous keycodes) over wired USB connection. Uses Report ID 2 in a combined descriptor, routing standard keyboard reports (Report ID 1) and NKRO reports through the same endpoint.
- **Combined Keyboard Descriptor** — New 107-byte static descriptor `COMBINED_KEYBOARD_DESC` in `usb_hid.rs` implementing Report ID 1 and Report ID 2 application collections.
- **USB HID / NKRO Unit Tests** — Host unit tests validating combined descriptor bytes, Report ID formatting, and NKRO bitmap generation logic.

### Fixed

- **USB Interface Count** — Reduced interface count back to 3 interfaces, preventing the composite USB peripheral from failing to enumerate on host startup.
- **Compiler Warnings** — Cleaned up unused import warnings in `usb_hid.rs`.

## v3.7.4 (2026-05-26)

Warning-free Clippy lint pass and parser robustness fixes.

### Fixed

- **UART RX Parser Lockup** — Reset `self.rx_len` to `0` upon length (`FormatErr`) or checksum (`SumErr`) validation errors, preventing buffer overruns and infinite main loop polling.
- **RGB Dirty Flag Lifetime** — Moved dirty flag clearing from `build_pwm_buffers()` to the `pwm_flush!` macro to ensure flags are only cleared after successful I2C writes, not prematurely during buffer construction.
- **Clippy Cleanups** — Zero warnings across both `thumbv6m-none-eabi` and `x86_64-unknown-linux-gnu` targets. Applied `Default` impls for Matrix, RgbMatrix, SideLeds, SleepManager, and UartProtocol. Replaced manual abs/range checks with `abs_diff()`/`contains()`, manual modulo with `is_multiple_of()`, and redundant casts/borrow patterns throughout.

### Added

- **`uart_pending` tracking** — New `Cell<bool>` field on `UartProtocol` to track pending UART transmissions.
- **Parser Robustness Unit Tests** — Assertions verifying `rx_len` is cleared on format and checksum error transitions.

## v3.7.3 (2026-05-26)

Ultra-low latency symmetric eager per-key lockout debouncing.

### Changed

- **0.5-1ms Keystroke Eager Debouncing** — Implemented Symmetric Eager Per-Key Lockout Debouncing (QMK `sym_eager_pk` algorithm). Press and release events are registered instantly (0ms debounce delay) on the first scan, with a 5ms noise lockout timer. Replaces the previous 2ms counter-based debounce.

## v3.7.2 (2026-05-26)

USB suspend/resume detection.

### Added

- **Suspend Detection** — Tracking of USB suspend state via `UsbDeviceState::Suspend` in the poll loop.
- **`configured()` / `is_suspended()`** — Public accessors for USB device state.

## v3.7.1 (2026-05-26)

Host keyboard LED state (Caps Lock indicator).

### Added

- **Host LED State** — `host_led_state()` reads the boot keyboard SET_REPORT to track Caps Lock, Num Lock, and Scroll Lock state from the host. Used by side LEDs for Caps Lock indicator.

## v3.7.0 (2026-05-26)

Full QMK RGB matrix animation port and USB descriptor refinements.

### Added

- **50 RGB Matrix Animation Modes** — Full 1:1 port of QMK's `rgb_matrix/animations/` including breathing, rainbow cycles, gradients, reactive (simple/wide/cross/nexus/splash variants), raindrops, heatmap, digital rain, starlight, band (sat/val/pinwheel/spiral), and more. 1,518 lines in `src/led/animation.rs`.
- **SystemControlReport16** — Custom u16 HID report descriptor for system control (sleep, power, DND), matching QMK's `report_extra_t`.

### Changed

- **USB VID:PID** — Changed from generic `0xFEED:0x6060` to `0x19F5:0x3266` (GosuDRM Air96 V2).
- **USB Descriptor** — Three-interface composite (keyboard + consumer + system control). NKRO (4th interface) was found to break USB enumeration on this hardware and is deferred.

## v3.2.0 (2026-05-26)

Critical fixes for typing latency, RGB matrix driver, and DFU bootloader jump.

### Fixed

- **Typing Latency** — Resolved the HAL timing calculation bug that throttled I2C to ~15.8 kHz. Enabled Fast-mode Plus (Fm+) high-drive strength on PB8/PB9/I2C1 and manually configured `TIMINGR` to 1 MHz. Optimized RGB matrix updates to track and write only dirty drivers (side LEDs on driver 2 do not trigger updates for driver 1).
- **RGB Driver Corruption** — Fixed `pwm_flush!` macro and initialization loop to use single 193-byte I2C writes instead of fragmented single-byte/chunked writes, preventing register address corruption on the IS31FL3733.
- **DFU Bootloader Jump** — Replaced SCB.vtor write (unsupported on Cortex-M0) with SYSCFG system memory remap to address `0x0000_0000` to correctly relocate the vector table.

## v3.0.5 (2026-05-26)

DFU entry finally working — the VTOR register was never being pointed at the bootloader's vector table.

### Fixed

- **DFU entry (VTOR)** — Every previous attempt (MEM_MODE+reset, bootstrap jump, flash page erase) failed because VTOR was still pointing to firmware's vector table. The ROM bootloader uses USB interrupts — without VTOR set to `0x1FFF_C800`, its ISRs never fire and USB enumeration silently fails. Fixed: disable interrupts, reset GPIOA, write VTOR, load bootloader SP/entry, `bx` jump. This is the canonical pattern used by embassy-rs, keyberon, and all working STM32F0 Rust projects.
- **3-second Escape hold DFU** — Hold Escape alone (no other keys) for 3 seconds to enter DFU at any time. No power-cycling, no BOOT0 pads, no boot-only check.

## v3.0.3 (2026-05-26)

### Fixed

- **Keymap alignment** — Completely rewrote all 5 keymap layers against the physical matrix from `keyboard.json`. Column `[0,1]` is empty on the PCB — every top-row key was shifted left by one. DEL/HOME/END/PGUP/PGDN were all on wrong rows. Enter was at empty position `[3,12]` instead of `[3,13]`.

## v3.0.2 (2026-05-26)

Critical firmware fixes — custom keycodes, DFU, RGB, and side LEDs were all non-functional.

### Fixed

- **`mo_layer()` mask too broad** — Checked `kc & 0xFF00 == 0x5C00`, which matched ALL custom keycodes (0x5C00–0x5CFF) as layer toggles. Every media key, link switch, side/RGB control, and Mac function key was silently swallowed. Fixed mask to `kc & 0xFFF0 == 0x5C20` (only MO keycodes 0x5C20–0x5C2F).
- **DFU entry never worked** — `enter_bootloader()` set SYSCFG MEM_MODE then called `sys_reset()`, which clears SYSCFG registers before the remap takes effect. Also SYSCFG clock was never enabled. Fixed: enable SYSCFG clock, remap system memory, direct jump via `cortex_m::asm::bootstrap()`.
- **IS31FL3733 init incomplete** — Global Current Control (reg 0x01) was never set (defaults to 0 = zero LED current) and LED on/off registers (page 0) were never enabled. Also wrote to reserved register 0x0A. Fixed: full init with GCC=0xFF, SW pull-up, CS pull-down, and all channels enabled.
- **Side LED output disconnected** — `SideLeds::update()` computed animations into its own buffer but never copied to the RGB matrix (indices 100–109). Side LEDs were always dark.

## v3.0.1 (2026-05-25)

Cortex-M0+ hardware fixes after on-device flash test.

### Fixed

- **GPIOD clock not enabled** — Matrix column D2 uses GPIOD pin 2, but `dp.GPIOD.split()` was never called. On STM32F0, accessing an unclocked GPIO peripheral causes hard faults or reads all pins as junk. Only the Windows key happened to register because its column wasn't on GPIOD.
- **`cortex_m::asm::delay()` no-op on Cortex-M0+** — The DWT cycle counter isn't available on armv6m without manual `DEMCR.TRCENA` bit set. `delay()` returned immediately, silently breaking:
  - NRF wakeup timing in `uart_flush!` (50µs + len×32µs hold time)
  - Matrix scan signal-settling delay after driving row low
  Replaced both with `core::ptr::read_volatile(0xE000_E010 as *const u32)` — reads SYST_CSR as compiler-proof busy-wait.
- **DFU entry hook** — Hold Escape while plugging in USB to enter ROM bootloader (`0483:DF11`). Uses SYSCFG MEM_MODE + system reset. Prevents soft-brick when firmware has issues.

## v3.0.0 (2026-05-25)

Initial Rust port of the Air96 V2 keyboard firmware from QMK C (NuPhy v1.0.4).

### Ported Modules

- **UART protocol** — full NRF module communication (460800 baud, 8E1)
- **Keymap engine** — 5 layers, 35 custom keycodes, transparency/fallthrough
- **Matrix scanner** — raw GPIO register scan, 6×21 COL2ROW, 5 ms debounce
- **HID reports** — keyboard (6KRO), NKRO (bit/byte hybrid), consumer, system, mouse
- **Sleep manager** — inactivity timeout, USB suspend, wakeup sequence
- **Side LEDs** — 5 animation modes (wave/mix/static/breath/off), battery/system/sleep/Caps-Lock indicators
- **RGB matrix** — dual IS31FL3733 drivers, 110-LED PWM map, HSV color, test sequence
- **USB HID** — composite device: boot keyboard + consumer control + system control
- **EEPROM** — config persistence via last flash page (page 63)
- **SysTick loop** — non-blocking 1 ms tick with state machines for reset/RGB-test

### Applied Fixes (from C firmware audit)

- **B1** — blink guard with disconnect state tracking
- **B2** — checksum validation on all received frames
- **B5** — battery not forced to 100% when charging
- **L1** — startup delay reduced (500 ms → 100 ms)
- **L2** — reports no longer dropped during pairing/linking
- **L6** — single UART transmission (no 3× retransmit)
- **K1** — inline wakeup in report sender
- **K2** — removed trailing 200 µs wait after UART TX
- **L10** — long press poll interval reduced (100 ms → 50 ms)
- Plus 19 additional latency/optimisation fixes

### Added Features

- Wired USB mode (keyboard + consumer + system control HID)
- 14 consumer/media keycodes (brightness, media controls, volume)
- 6 RGB matrix controls (speed, brightness, hue, mode)
- Full device factory reset (3-white-blink sequence)
- 82 host-side unit tests (UART, keymap, reports, sleep, LEDs, RGB, EEPROM, USB)
- Non-blocking SysTick main loop
- GitHub-ready README and LICENSE

### Known Limitations

- VIA configurator not supported (raw HID protocol not ported)
- macOS media keys (BRID/BRIU) use consumer HID reports — may differ slightly from Apple keyboard scan codes

### Technical

- Target: STM32F072CBTx (Cortex-M0+, 128 KB flash, 16 KB RAM)
- Binary size: ~24 KB (~19% flash usage)
- Dependencies: `cortex-m` 0.7, `stm32f0xx-hal` 0.18, `usb-device` 0.2, `usbd-hid` 0.6

---

## Original C Firmware (v1.0.4 — NuPhy QMK)

Baseline for the Rust port. Source at `nuphy-src/qmk_firmware` branch `nuphy-keyboards`.

---

[← Back to README](README.md)
