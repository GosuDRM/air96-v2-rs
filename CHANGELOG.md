# Changelog

## v3.0.1 (2026-05-25)

Cortex-M0+ hardware fixes after on-device flash test.

### Fixed

- **GPIOD clock not enabled** — Matrix column D2 uses GPIOD pin 2, but `dp.GPIOD.split()` was never called. On STM32F0, accessing an unclocked GPIO peripheral causes hard faults or reads all pins as junk. Only the Windows key happened to register because its column wasn't on GPIOD.
- **`cortex_m::asm::delay()` no-op on Cortex-M0+** — The DWT cycle counter isn't available on armv6m without manual `DEMCR.TRCENA` bit set. `delay()` returned immediately, silently breaking:
  - NRF wakeup timing in `uart_flush!` (50µs + len×32µs hold time)
  - Matrix scan signal-settling delay after driving row low
  Replaced both with `core::ptr::read_volatile(0xE000_E010 as *const u32)` — reads SYST_CSR register as compiler-proof busy-wait.

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
