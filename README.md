# Air96 V2 — Rust Firmware

Full Rust rewrite of the Air96 V2 wireless mechanical keyboard firmware.
Complete port from the original QMK C firmware, with all bug/latency/optimization
fixes applied and new features added. **Stable daily-driver.**

**Target:** STM32F072CBTx — Cortex-M0+, 128 KB flash, 16 KB RAM  
**Wireless:** NRF52832 module (Bluetooth LE + 2.4 GHz proprietary) via UART  
**LED:** Dual IS31FL3733 drivers (110 RGB LEDs), 10 side LEDs, 50 animation modes  
**USB:** Built-in USB FS peripheral — composite HID (keyboard + consumer + system control)

---

## What's New (v4.6.0)

- 🎛️ **VIA support** — Remap the dynamic keymap and control the RGB matrix live from the VIA app. RAW HID collection embedded as Report ID 3 inside the existing boot-kbd HIDClass (no 4th interface, no EP-memory overflow). Protocol v12, channel 3 reads/writes `RgbMatrix.{val,mode,speed,hue,sat}` in place. `via/Air96 V2 via3.json` carries the 24 custom keycodes.
- 💡 **Immediate USB suspend LED-off** — RGB matrix is zeroed the same cycle the host signals suspend (mirrors C `suspend_power_down_kb`), not after the 1s sleep-handler debounce. No more ~1s of frozen animation on PC shutdown.

Previous: v4.3.0 — bug audit fixes, chunked I2C flush, smooth RGB animation, BLE pairing from USB mode.

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
