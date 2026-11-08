# ⌨️ Air96 V2 — Rust Firmware

> A complete, from-scratch **Rust** rewrite of the NuPhy Air96 V2 wireless mechanical keyboard firmware — every QMK C feature ported, with bug/latency/optimization fixes and new capabilities layered on top. **Stable daily-driver.**

[![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](LICENSE)
[![MCU](https://img.shields.io/badge/MCU-STM32F072CBTx-brightgreen.svg)](#)
[![Rust](https://img.shields.io/badge/rust-no__std-orange.svg)](#)
[![Wireless](https://img.shields.io/badge/wireless-BLE%20%2B%202.4GHz-blueviolet.svg)](#)
[![VIA](https://img.shields.io/badge/VIA-supported-success.svg)](#)

| | |
|---|---|
| **Target** | STM32F072CBTx — Cortex-M0+, 128 KB flash, 16 KB RAM |
| **Wireless** | NRF52832 module (Bluetooth LE + 2.4 GHz proprietary) over UART |
| **LED** | Dual IS31FL3733 drivers — 100 RGB keys + 10 side LEDs, the C build's 40 RGB matrix effects |
| **USB** | Built-in USB FS — same 4 HID interfaces as the C VIA firmware (boot keyboard, VIA raw HID, shared mouse/system/consumer/NKRO, console) |

---

## 📑 Contents

- [✨ What's New](#-whats-new)
- [🌟 Features](#-features)
- [🔨 Build](#-build)
- [⚡ Flash](#-flash)
- [⌨️ Shortcuts](#-shortcuts)
- [📡 Wireless Modes](#-wireless-modes)
- [🏗️ Architecture](#-architecture)
- [📦 Dependencies](#-dependencies)
- [📄 License](#-license)

---

## ✨ What's New

### v4.9.0

- 🔁 **1:1 port of the C firmware (v3.2.9)**: keyboard logic (`ansi.c`, `rf.c`, `rf_driver.c`, `sleep.c`, `side.c`), the QMK action engine (tap/hold, one-shot, layers, mouse keys, key lock, macros, magic, grave escape), QMK's RGB matrix effects (frame-identical, checked against QMK's own code), VIA, and USB descriptors byte-identical to the C VIA build. See [CHANGELOG](CHANGELOG.md#v490-2026-11-08).
- ⚠️ First boot after flashing resets saved settings and VIA keymaps (new storage layout, as QMK does on a new build).

---

## 🌟 Features

- **Pure Rust, `no_std`** — full rewrite of the QMK C firmware for the STM32F072, no RTOS.
- **USB identical to the C VIA build** — boot keyboard, NKRO, mouse, system/consumer and console on the same interfaces, endpoints and report descriptors (byte-for-byte), with QMK's GET_REPORT/idle handling.
- **VIA support** — VIA protocol 12: 8 dynamic layers, 16 macros, layout options, live RGB matrix control, at the C build's EEPROM layout.
- **QMK action engine** — every keycode VIA can assign works as in QMK: mod-tap, layer-tap, one-shot, TT/TG/TO/DF, mouse keys, key lock, space cadet, magic, grave escape, tri-layer, QK_BOOT.
- **Wireless** — Bluetooth LE (3 channels) + 2.4 GHz proprietary link over an NRF52832 module, with full **NKRO**, hold-to-pair and battery reporting.
- **40 RGB matrix effects** (the C build's set, frame-identical) + 5 side-LED modes with battery, link, Caps Lock, OS and sleep indicators.
- **Power management** — port of the C firmware's `Sleep_Handle()`: wireless idle sleep (6 min), and LED/radio power-down after 1 s of USB suspend.
- **5-layer keymap** generated from the C `keymap.c` (Mac/Win bases + Fn + Function layers).
- **Config persistence** — RGB/side/sleep settings and the VIA keymap stored in on-chip flash.
- **DFU flashing** — built-in STM32 ROM bootloader, entered by holding **Escape** on plug-in (like QMK bootmagic, this also resets saved settings). No SWD probe required.

---

## 🔨 Build

### Prerequisites

```bash
# Arch/CachyOS
sudo pacman -S arm-none-eabi-gcc rustup
rustup target add thumbv6m-none-eabi
```

### Compile

```bash
cd air96-v2-rs
cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv6m-none-eabi/release/air96-v2 air96-v2-<version>.bin
```

The ELF (`target/thumbv6m-none-eabi/release/air96-v2`) is for `probe-rs`; the
flat binary (`air96-v2-<version>.bin`, ~45 KB) is for DFU. 128 KB flash has
plenty of headroom.

### Run tests

```bash
cargo test --lib --target x86_64-unknown-linux-gnu
```

---

## ⚡ Flash

### DFU (via built-in ROM bootloader) — recommended

Hold **Escape** while plugging in the USB cable. The keyboard enters STM32 DFU mode (`0483:DF11`).

```bash
dfu-util -d 0483:DF11 -a 0 -s 0x08000000:leave -D air96-v2-<version>.bin
```

> **QMK Toolbox is not recommended** for this board — it does not reliably write
> the flash.  Use `dfu-util` (or STM32CubeProgrammer) instead.

Pre-compiled binaries are available on the [Releases](../../releases) page.

### probe-rs (via SWD debugger)

```bash
probe-rs download --chip STM32F072CBTx --format elf target/thumbv6m-none-eabi/release/air96-v2
```

---

## ⌨️ Shortcuts

Two function layers, toggled by the **Fn** key (right Command on Mac / right Alt on Win):

### Mac base (layer 0)

Standard ANSI 96-key layout with `Ctrl` `Option` `Command` modifiers, dedicated
Delete cluster, and a 17-key numpad on the right edge. Top row carries
brightness, Mac media (`Task` `Search` `Siri` `DND`), media transport, and volume.

### Mac Fn (layer 1) — hold Fn

| Shortcut | Function |
|----------|----------|
| `F1`–`F12` | F1–F12 |
| `PrtSc` | Screenshot (Cmd+Shift+3) |
| `Del` | Insert |
| `1` `2` `3` | Bluetooth channel 1/2/3 (tap) / pair (hold 3s) |
| `4` | 2.4 GHz (tap) / pair (hold 3s) |
| `[` | Factory reset (hold 3s) |
| `]` | Sleep mode toggle |
| `\` | Battery indicator toggle |
| `,` / `.` | RGB speed down / up (on release; Shift reverses) |
| `↑` / `↓` | RGB brightness up / down |
| `←` | RGB effect next |
| `→` | RGB hue up |
| `Left Shift` / `M` / `Right Shift` | Function layer (layer 4) |

### Win base (layer 2)

Same physical layout with `F1`–`F12` on the top row and `Ctrl` `Win` `Alt` modifiers
in the bottom-left cluster.

### Win Fn (layer 3)

Same as Mac Fn, except the top row is brightness, media transport and volume,
and `PrtSc` is area screenshot (Win+Shift+S).

### Function (layer 4) — hold Fn, then Shift (or M)

| Shortcut | Function |
|----------|----------|
| `C` | RGB test (7-colour cycle, hold 3s) |
| `,` / `.` | Side LED speed down / up |
| `↑` / `↓` | Side LED brightness up / down |
| `←` | Side LED mode |
| `→` | Side LED colour |

---

## 📡 Wireless Modes

The physical switch on the back of the keyboard selects USB vs wireless.
In wireless mode, press the link keys (Fn+1/2/3/4) to switch between Bluetooth
channels and 2.4 GHz. Hold a link key for 3 seconds to enter pairing mode.

| Mode | Indicator (left side LED) |
|------|--------------------------|
| USB | Yellow |
| Bluetooth | Blue |
| 2.4 GHz | Green |
| Pairing | Fast blink |
| Connecting | Slow blink |

---

## 🏗️ Architecture

```
src/
├── main.rs              # Entry point, SysTick ISR, main loop, device state
├── lib.rs               # Crate root, cfg(not(test)) no_std
├── hal.rs               # Board seam: one method per QMK/ChibiOS call the C firmware makes
├── kb.rs                # 1:1 port of ansi.c + QMK action/report/host glue
├── usb_hid.rs           # USB composite HID (keyboard + consumer)
├── config/
│   ├── mod.rs
│   ├── hardware.rs      # Matrix dimensions, timing constants
│   └── eeprom.rs        # Flash-based config persistence (page 63)
├── keyboard/
│   ├── mod.rs
│   ├── matrix.rs        # Raw GPIO register scan (6×21 COL2ROW), QMK asym_eager_defer_pk debounce (10ms)
│   └── keymap.rs        # 5-layer keymap, 35 custom keycodes, consumer mapping
├── wireless/
│   ├── mod.rs
│   ├── uart.rs          # 1:1 port of rf.c (NRF UART protocol, report queue/replay)
│   ├── report.rs        # 1:1 port of rf_driver.c
│   └── sleep.rs         # 1:1 port of sleep.c
└── led/
    ├── mod.rs
    ├── side.rs          # 1:1 port of side.c (side bars, battery/system/sleep/link indicators)
    ├── side_tables.rs   # Tables generated from side.h
    └── rgb.rs            # IS31FL3733 driver, 110-LED PWM map, HSV conversion
```

### Data Flow

```
Matrix Scan → Keymap Lookup (5 layers) → Process Key Event
    │                                            │
    ├─ Custom keycode? ──→ Link switch / Side LED / RGB control / Consumer key
    │                                            │
    └─ Standard HID? ────→ Update modifier + 6KRO buffer
                                                     │
                            ┌────────────────────────┘
                            ▼
              ┌─ link_mode == USB? ──→ USB HID (keyboard + consumer)
              │
              └─ link_mode != USB? ──→ UART frame → NRF module → wireless
```

---

## 📦 Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `cortex-m` | 0.7 | Cortex-M0+ runtime, critical sections |
| `cortex-m-rt` | 0.7 | Startup, vector table, exception handlers |
| `stm32f0xx-hal` | 0.18 | GPIO, UART, I2C, USB peripheral drivers |
| `usb-device` | 0.2.9 | USB device framework (vendored: bcdUSB 2.00 like QMK) |
| `heapless` | 0.8 | Stack-allocated Vec for I2C write batching |
| `nb` | 1.1 | Non-blocking I/O (UART) |

---

## 📄 License

[GPL-2.0-or-later](LICENSE) — derived from NuPhy QMK firmware.
