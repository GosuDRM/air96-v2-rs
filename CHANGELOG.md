# Changelog

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
