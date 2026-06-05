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
| **LED** | Dual IS31FL3733 drivers — 110 RGB keys + 10 side LEDs, 50 animation modes |
| **USB** | Built-in USB FS — composite HID (keyboard + NKRO + consumer + system) |

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

### v4.7.3

- ⌨️ **Wired keyboard now works in BIOS / UEFI / bootloaders** — the USB keyboard is now a proper HID **boot keyboard** (`bInterfaceSubClass=1`, `bInterfaceProtocol=1`) sending the fixed 8-byte report with **no Report ID**, so it's recognised in the BIOS setup screen, boot menu, and disk-encryption prompts. Previously the interface advertised subclass/protocol `0/0` and prefixed every report with a Report ID byte (and defaulted to NKRO over USB), all of which pre-OS firmware can't read. VIA moved to its own RAW HID interface. **Wired is now 6KRO**; NKRO is unchanged over wireless.

### v4.7.2

- 🪟 **Windows 11 USB fix** — the keyboard now enumerates reliably on Windows 11. The polled USB stack asserted the D+ pull-up ~200 ms before the main loop began polling (during the EEPROM load, dial scan and RF handshake), so the device couldn't answer the host's enumeration requests in time and Windows showed "USB device not recognized". USB is now brought up as the **last step before the polling loop**, so the pull-up asserts only once the bus can be serviced.
- 🌑 **LEDs off at shutdown — hardware kill** — the USB-suspend edge now cuts the LED boost + driver-shutdown rails (`DC_BOOST`/`SDB1`/`SDB2`) directly, independent of the 1 s sleep debounce. *(Completes the v4.7.0 fix.)*

[Full changelog →](CHANGELOG.md)

---

## 🌟 Features

- **Pure Rust, `no_std`** — full rewrite of the QMK C firmware for the STM32F072, no RTOS.
- **Composite USB HID** — BIOS-compatible **boot keyboard** (6KRO over USB), consumer/media keys, and system control on the built-in USB FS peripheral. Driverless on Windows, macOS, and Linux.
- **VIA support** — remap the dynamic keymap and drive the RGB matrix live from the VIA app (dedicated RAW HID interface, protocol v12).
- **Wireless** — Bluetooth LE (3 channels) + 2.4 GHz proprietary link over an NRF52832 module, with full **NKRO**, hold-to-pair and battery reporting.
- **50 RGB matrix effects** + 5 side-LED modes — HSV control, reactive and typing-heatmap animations, per-key Caps/Num indicators.
- **Power management** — inactivity sleep with GPIO rail power-down, plus instant LED-off on USB suspend / PC shutdown.
- **5-layer keymap** with 35 custom keycodes (Mac/Win bases + Fn + Function layers).
- **Config persistence** — RGB/side/sleep settings and the VIA keymap stored in on-chip flash.
- **DFU flashing** — built-in STM32 ROM bootloader, entered by holding **Escape** on plug-in. No SWD probe required.

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

### DFU (via built-in ROM bootloader)

Hold **Escape** while plugging in the USB cable. The keyboard enters STM32 DFU mode (`0483:DF11`).

```bash
dfu-util -d 0483:DF11 -a 0 -s 0x08000000:leave -D air96-v2-<version>.bin
```

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
| `1` `2` `3` | Switch to Bluetooth channel 1/2/3 (tap) / pair (hold 3s) |
| `4` | 2.4 GHz wireless (tap) / pair (hold 3s) |
| `F13` (PrtSc) | Factory reset (hold 3s) |
| `F14` (ScrLk) | Sleep mode toggle |
| `F15` (Pause) | Battery indicator toggle |
| `Print` (PrtSc) | Area screenshot |
| `Z` / `X` | RGB speed down / up |
| `C` | RGB speed reset |
| `↑` | RGB brightness up |
| `←` / `→` | RGB effect previous / next |
| `↓` | RGB brightness down |
| `Space` | RGB test (7-colour cycle) |

### Win base (layer 2)

Same physical layout with `F1`–`F12` on the top row and `Ctrl` `Win` `Alt` modifiers
in the bottom-left cluster.

### Win Fn (layer 3)

Same shortcuts as Mac Fn, with media transport keys mapped to the top row
(brightness, prev/next/play, mute/volume) when the Win layer is active.

### Function (layer 4) — hold Fn, then tap Shift (or Fn + M)

Side LED and alternate RGB controls (matches C firmware `MO(4)`):

| Shortcut | Function |
|----------|----------|
| `B` | RGB test (7-colour cycle) |
| `,` / `.` | Side LED speed down / up |
| `↑` | Side LED brightness up |
| `←` | Side LED mode cycle |
| `↓` | Side LED brightness down |
| `→` | Side LED hue cycle |

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
├── usb_hid.rs           # USB composite HID (keyboard + consumer)
├── config/
│   ├── mod.rs
│   ├── hardware.rs      # Matrix dimensions, timing constants
│   └── eeprom.rs        # Flash-based config persistence (page 63)
├── keyboard/
│   ├── mod.rs
│   ├── matrix.rs        # Raw GPIO register scan (6×21 COL2ROW, 5ms debounce)
│   └── keymap.rs        # 5-layer keymap, 35 custom keycodes, consumer mapping
├── wireless/
│   ├── mod.rs
│   ├── uart.rs          # NRF UART protocol (440 lines, port of rf.c)
│   ├── report.rs         # HID report sender (keyboard/NKRO/consumer/system/mouse)
│   └── sleep.rs          # Power management (sleep/wakeup state machine)
└── led/
    ├── mod.rs
    ├── side.rs           # Side LED 5-mode animations + battery/system/sleep indicators
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
| `usb-device` | 0.2 | USB device framework |
| `usbd-hid` | 0.6 | USB HID class (keyboard + consumer descriptors) |
| `heapless` | 0.8 | Stack-allocated Vec for I2C write batching |
| `nb` | 1.1 | Non-blocking I/O (UART) |

---

## 📄 License

[GPL-2.0-or-later](LICENSE) — derived from NuPhy QMK firmware.
