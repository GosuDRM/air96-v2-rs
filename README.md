# Air96 V2 — Rust Firmware

Full Rust rewrite of the Air96 V2 wireless mechanical keyboard firmware.
Complete port from the original QMK C firmware, with all bug/latency/optimization
fixes applied and new features added. **Stable daily-driver.**

**Target:** STM32F072CBTx — Cortex-M0+, 128 KB flash, 16 KB RAM  
**Wireless:** NRF52832 module (Bluetooth LE + 2.4 GHz proprietary) via UART  
**LED:** Dual IS31FL3733 drivers (110 RGB LEDs), 10 side LEDs, 50 animation modes  
**USB:** Built-in USB FS peripheral — composite HID (keyboard + consumer + system control)

---

## What's New (v4.2.0)

- ⚡ **Chunked I2C flush** — RGB PWM writes split into 64-byte chunks (~0.5ms each) so matrix scan runs between transfers. Fixes keystroke hiccups during rapid typing.
- 🎨 **Smooth RGB animation** — Real wall-clock millisecond counter (u32, matching C `g_rgb_timer`), 62.5 FPS frame rate. No more stepping artifacts in breathing/cycle effects.
- 📡 **BLE pairing from USB mode** — Fn+1/2/3/4 works regardless of current mode. Short tap switches channel, long hold (3s) enters pairing.
- 🔄 **Incremental LED flush (C firmware)** — Same chunked I2C optimization ported to the QMK C firmware (v3.2.3).

[Full changelog →](CHANGELOG.md)

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
```

Binary: `target/thumbv6m-none-eabi/release/air96-v2` (~42 KB, fits easily in 128 KB flash)

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
probe-rs download --chip STM32F072CBTx target/thumbv6m-none-eabi/release/air96-v2
```

---

## ⌨️ Keymap

### Layer 0 — Mac base

|   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |   |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ESC | 🔅 | 🔆 | Task | Search | Siri | DND | ⏮ | ⏯ | ⏭ | 🔇 | 🔉 | 🔊 | Screenshot | DEL | HOME | END | PGUP | PGDN |
| `` ` `` | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 0 | - | = | ⌫ | | NUM | / | * | - |
| TAB | Q | W | E | R | T | Y | U | I | O | P | [ | ] | \\ | | 7 | 8 | 9 | + |
| CAPS | A | S | D | F | G | H | J | K | L | ; | ' | ENTER | | | 4 | 5 | 6 | |
| SHIFT | | Z | X | C | V | B | N | M | , | . | / | SHIFT | | | ↑ | 1 | 2 | 3 | ENTER |
| CTRL | OPT | CMD | | | | SPACE | | | | CMD | Fn | CTRL | | | ← | ↓ | → | 0 | . |

Modifiers: `Ctrl` `Option` `Command` (Mac layout)

### Layer 1 — Mac Fn

Hold **Fn** (right Command position) to access:

| Key | Function |
|-----|----------|
| 1 | Bluetooth 1 (tap = switch, hold 3s = pair) |
| 2 | Bluetooth 2 |
| 3 | Bluetooth 3 |
| 4 | 2.4 GHz |
| F13 (PrtSc) | Reset device (hold 3s for factory reset) |
| F14 (ScrLk) | Sleep mode toggle |
| F15 (Pause) | Battery indicator toggle |
| Fn+Screenshot | Area screenshot |
| Fn+Z/X/C | RGB speed down/up |
| Fn+↑ | RGB brightness up |
| Fn+←/↓/→ | RGB mode / brightness down / hue |
| Fn+Space | RGB test (7-colour cycle) |

### Layer 2 — Win base

Same layout as Mac base but with **F1–F12** on the top row and `Ctrl` `Win` `Alt` modifiers.

### Layer 3 — Win Fn

Same as Mac Fn, plus media keys accessible on the top row (brightness + media controls).

### Layer 4 — Function (Side LED + RGB controls)

Accessed via **Fn+Fn** (double Fn):

| Key | Function |
|-----|----------|
| Fn+Z | Side LED speed down |
| Fn+X | Side LED speed up |
| Fn+↑ | Side LED brightness up |
| Fn+← | Side LED mode cycle |
| Fn+↓ | Side LED brightness down |
| Fn+→ | Side LED hue cycle |

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
