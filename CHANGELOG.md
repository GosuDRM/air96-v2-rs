# Changelog

## v4.1.2 (2026-06-02)

Restored QMK `sym_eager_pk` debouncing and fixed RGB matrix defaults to match the C reference firmware.

### Fixed

- **Same-Key Latency** — Reverted `matrix::scan()` from `sym_defer_pk` (10ms stability timer per edge) back to QMK's `sym_eager_pk` (0ms first-event latency, 5ms per-key lockout). With `sym_defer_pk`, a release→press cycle faster than 10ms silently dropped the second press because the stability counter was reset by the re-press before it could expire. `sym_eager_pk` matches `quantum/debounce/sym_eager_pk.c` from the C reference (`keyboards/air96_v2/ansi/keyboard.json` `DEBOUNCE=5`) — the first scan that detects a change emits the event immediately, and a 5ms lockout absorbs bounce. Rapid press→release→press cycles now register each edge as it happens (bounce-filtered only within the 5ms lockout window).
- **RGB Matrix Defaults** — Freshly flashed RGB now matches the C reference (`quantum/rgb_matrix/rgb_matrix.h:52-82`). Default mode changed from solid_color (0) to `CYCLE_LEFT_RIGHT` (4). Default HSV changed from (255, 255, 223) to (0, 255, 255). Default speed changed from 223 to 127. EEPROM `UserConfig::default()` and the `DEV_RESET` handler also updated to match. EEPROM magic bumped from V2 (0xA6) to V3 (0xA7) to force a config reset on first boot after flash, ensuring old EEPROM values (mode=0, hue=255) don't override the new defaults.

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
