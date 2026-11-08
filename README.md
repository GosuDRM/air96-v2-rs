# Air96 V2 Rust Firmware

Rust (`no_std`) firmware for the NuPhy Air96 V2 keyboard, ported 1:1 from the
[C/QMK firmware](https://github.com/GosuDRM/air96-v2) v3.2.9.

[![Release](https://img.shields.io/github/v/release/GosuDRM/air96-v2-rs)](https://github.com/GosuDRM/air96-v2-rs/releases/latest)
[![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](LICENSE)

| Hardware | |
|---|---|
| MCU | STM32F072CB: Cortex-M0, 128 KB flash, 16 KB RAM |
| Wireless | NRF52832 module over UART: Bluetooth LE (3 channels) + 2.4 GHz |
| LEDs | 2× IS31FL3733: 100 RGB keys + 10 side LEDs |
| USB | Full speed, 4 HID interfaces: boot keyboard, VIA, mouse/system/consumer/NKRO, console |

## Status

- **Verified against the C build:** USB descriptors are byte-identical to the
  C VIA build; all 40 RGB effects match QMK's own code frame for frame
  (`tools/rgb_harness`).
- **Hardware-tested:** wired USB on Linux.
- **Not yet verified on hardware:** wireless, pairing, sleep/wake, USB on
  Windows and macOS.

Release notes: [CHANGELOG.md](CHANGELOG.md).

## Features

- Keyboard logic ported function by function from the C sources (`ansi.c`,
  `rf.c`, `rf_driver.c`, `sleep.c`, `side.c`), and QMK's action engine:
  mod-tap, layer-tap, one-shot, layer keys, mouse keys, key lock, space cadet,
  magic, grave escape, tri-layer.
- VIA (protocol 12): 8 layers, 16 macros, layout options, live RGB control.
  If VIA doesn't detect the board, load [`via/Air96 V2 via3.json`](via/).
- Full NKRO over USB and wireless, NumLock on by default (Windows mode).
- 40 RGB matrix effects, 5 side-LED modes, battery/link/Caps Lock/OS/sleep
  indicators.
- Settings and VIA keymaps saved in on-chip flash.

## Flash

Download `air96-v2-rs-vX.Y.Z.bin` from
[Releases](https://github.com/GosuDRM/air96-v2-rs/releases), or build it
(below).

1. Unplug the keyboard, hold **Esc**, plug it in. It enters STM32 DFU mode
   (`0483:df11`). Like QMK bootmagic, this also resets saved settings.
2. Flash:

   ```bash
   dfu-util -d 0483:df11 -a 0 -s 0x08000000:leave -D air96-v2-rs-vX.Y.Z.bin
   ```

The first boot of a new build resets settings and VIA keymaps (QMK behaviour).
Use `dfu-util` or STM32CubeProgrammer; QMK Toolbox doesn't write this board's
flash reliably.

With an SWD probe:

```bash
probe-rs download --chip STM32F072CBTx --format elf target/thumbv6m-none-eabi/release/air96-v2
```

## Build

Requires Rust with the `thumbv6m-none-eabi` target and `arm-none-eabi-objcopy`
(Arch: `arm-none-eabi-binutils`; Debian/Ubuntu: `binutils-arm-none-eabi`).

```bash
rustup target add thumbv6m-none-eabi
cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv6m-none-eabi/release/air96-v2 air96-v2-rs.bin

# host tests
cargo test --lib --target x86_64-unknown-linux-gnu
```

Pushing a `v*` tag builds the binary and publishes a GitHub release with that
version's CHANGELOG section.

## Shortcuts

**Fn** is right Command (Mac) / right Alt (Win). Layers: 0 Mac base, 1 Mac Fn,
2 Win base, 3 Win Fn, 4 Function.

### Fn (layers 1 and 3)

| Key | Action |
|-----|--------|
| `F1`–`F12` | F1–F12 (Mac); the Win Fn top row is brightness, media and volume |
| `PrtSc` | Screenshot: Cmd+Shift+3 (Mac) / Win+Shift+S (Win) |
| `Del` | Insert |
| `1` `2` `3` | Bluetooth channel 1/2/3; hold 3 s to pair |
| `4` | 2.4 GHz; hold 3 s to pair |
| `[` | Factory reset (hold 3 s) |
| `]` | Sleep mode on/off |
| `\` | Battery indicator on/off |
| `,` / `.` | RGB speed down / up (on release; Shift reverses) |
| `↑` / `↓` | RGB brightness up / down |
| `←` / `→` | Next RGB effect / RGB hue up |
| `Left Shift` / `M` / `Right Shift` | Function layer (hold) |

### Function (layer 4): hold Fn, then Shift or M

| Key | Action |
|-----|--------|
| `C` | RGB test, 7-colour cycle (hold 3 s) |
| `,` / `.` | Side LED speed down / up |
| `↑` / `↓` | Side LED brightness up / down |
| `←` / `→` | Side LED mode / colour |

### Link indicator (left side LEDs)

The switch on the back selects USB or wireless; Fn+1–4 picks the wireless link.

| Link | Colour |
|------|--------|
| USB | Yellow |
| Bluetooth | Blue |
| 2.4 GHz | Green |

Fast blink: pairing. Slow blink: connecting.

## Source layout

```
src/
├── main.rs            board bring-up, interrupts (SysTick, USART1, USB), main loop
├── hal.rs             board seam: one method per QMK/ChibiOS call the C code makes
├── kb.rs              ansi.c, host.c
├── action.rs          QMK action engine, quantum keycodes, mouse keys
├── via.rs             via.c, dynamic_keymap.c
├── usb_hid.rs         USB descriptors and QMK HID request handling
├── config/eeprom.rs   flash storage (last two 2 KB pages)
├── keyboard/          matrix scan + debounce, keymap (generated)
├── wireless/          rf.c (uart.rs), rf_driver.c (report.rs), sleep.c
├── led/               rgb_matrix.c effects, IS31FL3733 map, side.c (+ generated tables)
└── *test.rs           host tests (mock Hal, mock USB bus)
tools/                 generators from the C sources, RGB diff harness, stack analysis
vendor/usb-device-0.2.9  patched to report bcdUSB 2.00, as QMK does
```

## License

GPL-2.0-or-later, see [LICENSE](LICENSE).

- Copyright (C) 2023 NuPhy / Persama (original QMK C firmware)
- Copyright (C) 2026 GosuDRM (Rust port)
