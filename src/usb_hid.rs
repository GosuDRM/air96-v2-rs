//! USB HID keyboard driver — wired mode support.
//!
//! Uses the STM32F072's built-in USB FS peripheral (PA11=DM, PA12=DP)
//! to present a composite HID device with four interfaces:
//!   1. Boot keyboard (8-byte reports, NO report ID) — standard keys + LED state
//!   2. RAW HID / VIA (32-byte IN/OUT, vendor page 0xFF60)
//!   3. Consumer control (2-byte reports) — media keys, volume, brightness
//!   4. System control (2-byte reports) — sleep, power, DND
//!
//! ## Why interface #1 is a *boot* keyboard (BIOS / pre-OS support)
//!
//! PC firmware (BIOS/UEFI legacy USB support) and bootloaders do **not** parse
//! the HID report descriptor. They only drive HID keyboards that present the
//! USB-spec "Boot Interface": `bInterfaceSubClass = 1` (Boot) and
//! `bInterfaceProtocol = 1` (Keyboard), and they read a *fixed* 8-byte report
//! with **no Report ID prefix** — `[modifiers, reserved, key0..key5]` (HID 1.11
//! Appendix B.1). A keyboard that omits the Boot subclass/protocol, or that
//! prefixes its report with a Report ID byte, is invisible or garbled in the
//! BIOS setup screen, boot menu, and disk-encryption prompts.
//!
//! The previous merged descriptor declared the keyboard with Report IDs 1/2/3
//! on a single non-boot interface (subclass 0, protocol 0). That works under a
//! full OS HID stack (which reads the descriptor) but is dead in BIOS — the
//! firmware never recognises it as a keyboard, and even if it did, the Report
//! ID 1 byte is misread as the modifier byte. That is the wired-mode "keyboard
//! doesn't work in BIOS" bug.
//!
//! ## Why USB is 6KRO-only (no NKRO over the wire)
//!
//! `usbd-hid` gates `push_*` on a single per-interface protocol mode: a Boot
//! interface can only transmit in boot protocol, a report interface only in
//! report protocol (see `push_raw_input` in usbd-hid's `hid_class.rs`). There
//! is no public API to read the host's SET_PROTOCOL choice on a `ForceBoot`
//! interface, so we cannot do QMK-style dynamic 6KRO(boot)/NKRO(report)
//! switching on one endpoint. We therefore pin the keyboard to boot protocol
//! (`ForceBoot`) and always emit the 8-byte 6KRO boot report — universally
//! compatible with BIOS and every OS. NKRO is unchanged on the wireless path.
//!
//! Works on Windows, Linux, and macOS without drivers.

use stm32f0xx_hal::pac;
use stm32f0xx_hal::usb::UsbBusType;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::UsbDeviceState;
use usb_device::prelude::*;
use usbd_hid::descriptor::MediaKeyboardReport;
use usbd_hid::descriptor::SerializedDescriptor;
use usbd_hid::hid_class::{
    HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
};

// ───────────────────────────────────────────────────────────────────────────
// usbd-hid 0.6.2 SET_REPORT panic — DO NOT remove the keyboard OUT endpoint.
// ───────────────────────────────────────────────────────────────────────────
//
// `usbd-hid` 0.6.2's `control_out` SET_REPORT handler does
//
//     let mut buf: [u8; CONTROL_BUF_LEN] = [0; CONTROL_BUF_LEN];   // 128 B
//     buf.copy_from_slice(&xfer.data()[..len]);                    // len = 1 for LED
//
// (`hid_class.rs:677-678`). `copy_from_slice` requires equal-length slices and
// PANICS on every SET_REPORT shorter than 128 B — i.e. on every keyboard LED
// state update the host sends. With `panic = "abort"` and a `wfe` panic
// handler (`src/main.rs:1581`) that means the firmware silently stops
// scanning the matrix and the keyboard becomes unresponsive to ALL keys.
//
// Fixed upstream in 0.7+ (`buf[..len].copy_from_slice(&xfer.data()[..len])`),
// but 0.7 requires usb-device 0.3 while stm32-usbd 0.6 pins usb-device 0.2 —
// so we can't simply bump the crate.
//
// Workaround: keep an interrupt OUT endpoint on the keyboard interface
// (`HIDClass::new_with_settings`, not `new_ep_in_with_settings`). With an
// interrupt OUT advertised, Windows / Linux / macOS deliver the 1-byte LED
// state via that endpoint (read with `pull_raw_output`), and never hit the
// buggy SET_REPORT control path. This mirrors what v4.7.2 did before the
// boot-keyboard refactor.
//
// BIOS doesn't care that the boot interface has an OUT endpoint — the boot
// keyboard spec permits it (HID 1.11 §B.1) and it doesn't affect the IN
// report format. EP budget is fine: 5 IN + 2 OUT + EP0 + BTABLE = 576 B of
// the STM32F072's 1024-byte PMA, 5 of 8 EP indices used.

const USB_VID: u16 = 0x19F5;
const USB_PID: u16 = 0x3266;

/// System control report descriptor (25 bytes) — Generic Desktop / System Control.
///
/// Hand-written (NOT `gen_hid_descriptor`) so the array bounds are valid for
/// Windows. This is an **array** input item: the 2-byte field carries a usage
/// *selector*, so `LOGICAL_MAXIMUM` must equal the top of the usage range
/// (0xB7), exactly as QMK's C descriptor (`usb_descriptor.c:300-310`) declares.
/// Value sent = the raw usage code (e.g. 0x9B), since
/// `usage = usage_min + (value - logical_min) = value`.
///
/// ## Why this is hand-written: Windows "USB device not recognized"
///
/// The previous version used `#[gen_hid_descriptor]` on a `u16` field. The
/// macro derives `LOGICAL_MAXIMUM` from the field's *type* (u16 → 65535), so it
/// emitted an array whose logical range (0..65535) dwarfed its 0x81..0xB7 usage
/// range — and the macro's spec-level `logical_min` also injected a stray
/// reserved Main item (`0x11 0x01`, an undefined Main tag). Linux's lenient HID
/// parser accepts that; Windows' strict `hidparse.sys` rejects the System
/// Control interface, which surfaces as "USB device not recognized" on Windows
/// while the keyboard works fine on Linux. No Report ID (its own interface).
pub const SYSTEM_DESC: &[u8] = &[
    0x05, 0x01,       // USAGE_PAGE (Generic Desktop)
    0x09, 0x80,       // USAGE (System Control)
    0xA1, 0x01,       // COLLECTION (Application)
    0x19, 0x01,       //   USAGE_MINIMUM (0x01)
    0x2A, 0xB7, 0x00, //   USAGE_MAXIMUM (0x00B7)
    0x15, 0x01,       //   LOGICAL_MINIMUM (1)
    0x26, 0xB7, 0x00, //   LOGICAL_MAXIMUM (0x00B7)
    0x95, 0x01,       //   REPORT_COUNT (1)
    0x75, 0x10,       //   REPORT_SIZE (16)
    0x81, 0x00,       //   INPUT (Data,Array,Abs)
    0xC0,             // END_COLLECTION
];

/// Boot keyboard report descriptor (63 bytes) — HID 1.11 Appendix B.1.
///
/// Single top-level collection, **no Report ID**, fixed 8-byte input report:
///   byte 0    — modifier bitmap (LCtrl..RGui)
///   byte 1    — reserved (0)
///   bytes 2-7 — up to six pressed keycodes (6KRO)
/// plus a 1-byte LED output report (NumLock..Kana + 3 bits padding).
///
/// Paired with `bInterfaceSubClass=1` / `bInterfaceProtocol=1` so BIOS, UEFI,
/// and bootloaders recognise and read it. Because there is no Report ID, the
/// boot-protocol consumer reads it byte-for-byte; the OS reads the identical
/// format in report protocol.
pub const BOOT_KEYBOARD_DESC: &[u8] = &[
    0x05, 0x01, // USAGE_PAGE (Generic Desktop)
    0x09, 0x06, // USAGE (Keyboard)
    0xA1, 0x01, // COLLECTION (Application)
    // Modifiers (8 bits)
    0x05, 0x07, //   USAGE_PAGE (Keyboard)
    0x19, 0xE0, //   USAGE_MINIMUM (Keyboard Left Control)
    0x29, 0xE7, //   USAGE_MAXIMUM (Keyboard Right GUI)
    0x15, 0x00, //   LOGICAL_MINIMUM (0)
    0x25, 0x01, //   LOGICAL_MAXIMUM (1)
    0x75, 0x01, //   REPORT_SIZE (1)
    0x95, 0x08, //   REPORT_COUNT (8)
    0x81, 0x02, //   INPUT (Data,Var,Abs)
    // Reserved byte
    0x95, 0x01, //   REPORT_COUNT (1)
    0x75, 0x08, //   REPORT_SIZE (8)
    0x81, 0x03, //   INPUT (Cnst,Var,Abs)
    // LEDs (5 bits + 3 bits padding) — host-driven (NumLock, CapsLock, …)
    0x95, 0x05, //   REPORT_COUNT (5)
    0x75, 0x01, //   REPORT_SIZE (1)
    0x05, 0x08, //   USAGE_PAGE (LEDs)
    0x19, 0x01, //   USAGE_MINIMUM (Num Lock)
    0x29, 0x05, //   USAGE_MAXIMUM (Kana)
    0x91, 0x02, //   OUTPUT (Data,Var,Abs)
    0x95, 0x01, //   REPORT_COUNT (1)
    0x75, 0x03, //   REPORT_SIZE (3)
    0x91, 0x03, //   OUTPUT (Cnst,Var,Abs)
    // Keycodes (6 bytes, array)
    0x95, 0x06, //   REPORT_COUNT (6)
    0x75, 0x08, //   REPORT_SIZE (8)
    0x15, 0x00, //   LOGICAL_MINIMUM (0)
    0x26, 0xFF, 0x00, // LOGICAL_MAXIMUM (255)
    0x05, 0x07, //   USAGE_PAGE (Keyboard)
    0x19, 0x00, //   USAGE_MINIMUM (Reserved (no event indicated))
    0x29, 0xFF, //   USAGE_MAXIMUM (255)
    0x81, 0x00, //   INPUT (Data,Arr,Abs)
    0xC0, // END_COLLECTION
];

/// RAW HID (VIA) report descriptor (29 bytes) — vendor page 0xFF60.
///
/// Its own interface, **no Report ID**, 32-byte IN and 32-byte OUT reports.
/// VIA's host client identifies the device by usage page 0xFF60 / usage 0x61,
/// not by Report ID, so a report-ID-less single-report interface is what it
/// expects. Split out from the keyboard so the keyboard can be a clean boot
/// interface (see module docs).
pub const VIA_DESC: &[u8] = &[
    0x06, 0x60, 0xFF, // USAGE_PAGE (Vendor Defined 0xFF60)
    0x09, 0x61, // USAGE (0x61)
    0xA1, 0x01, // COLLECTION (Application)
    0x09, 0x62, //   USAGE (0x62)
    0x15, 0x00, //   LOGICAL_MINIMUM (0)
    0x26, 0xFF, 0x00, // LOGICAL_MAXIMUM (255)
    0x95, 0x20, //   REPORT_COUNT (32)
    0x75, 0x08, //   REPORT_SIZE (8)
    0x81, 0x02, //   INPUT (Data,Var,Abs)
    0x09, 0x63, //   USAGE (0x63)
    0x15, 0x00, //   LOGICAL_MINIMUM (0)
    0x26, 0xFF, 0x00, // LOGICAL_MAXIMUM (255)
    0x95, 0x20, //   REPORT_COUNT (32)
    0x75, 0x08, //   REPORT_SIZE (8)
    0x91, 0x02, //   OUTPUT (Data,Var,Abs)
    0xC0, // END_COLLECTION
];

pub struct UsbHid<'a> {
    device: UsbDevice<'a, UsbBusType>,
    /// Boot keyboard (subclass=Boot, protocol=Keyboard, ForceBoot).
    /// IN endpoint carries the 8-byte report; OUT endpoint carries the 1-byte
    /// LED state. The OUT endpoint is mandatory here as a workaround for the
    /// usbd-hid 0.6.2 SET_REPORT panic — see the comment at the top of this
    /// file. Do NOT drop it back to IN-only.
    keyboard: HIDClass<'a, UsbBusType>,
    /// RAW HID / VIA — IN + OUT endpoints.
    via: HIDClass<'a, UsbBusType>,
    consumer: HIDClass<'a, UsbBusType>,
    system: HIDClass<'a, UsbBusType>,
    /// Host-controlled keyboard LED state (bits: 0=NumLock, 1=CapsLock, 2=ScrollLock, 3=Compose, 4=Kana)
    led_state: u8,
    /// Tracks suspend state for edge detection
    suspended: bool,
    /// Latched: true for one poll cycle on the suspend edge (consumed by take_suspend_edge)
    just_suspended: bool,
    /// Latched: true for one poll cycle on the resume edge
    just_resumed: bool,
    /// Pending 8-byte boot keyboard report (no Report ID).
    pending_keyboard_report: Option<[u8; 8]>,
}

impl<'a> UsbHid<'a> {
    pub fn new(bus: &'a UsbBusAllocator<UsbBusType>) -> Self {
        // Boot keyboard: advertise Boot subclass + Keyboard protocol so BIOS/UEFI
        // recognise it, and ForceBoot so usbd-hid lets us push the 8-byte boot
        // report regardless of the host's SET_PROTOCOL (see module docs).
        // Uses `new_with_settings` (IN + OUT), NOT `new_ep_in_with_settings`,
        // to dodge the usbd-hid 0.6.2 SET_REPORT panic — host LED state goes
        // out the interrupt OUT endpoint instead of the buggy control pipe.
        // See the long comment near the top of this file.
        let keyboard = HIDClass::new_with_settings(
            bus,
            BOOT_KEYBOARD_DESC,
            1,
            HidClassSettings {
                subclass: HidSubClass::Boot,
                protocol: HidProtocol::Keyboard,
                config: ProtocolModeConfig::ForceBoot,
                locale: HidCountryCode::NotSupported,
            },
        );
        // VIA needs an OUT endpoint for host→device commands, so it keeps the
        // full IN+OUT pair. Consumer/system are IN-only — they have no host→device
        // traffic, so dropping their OUT endpoints saves PMA without affecting
        // any host that's tested against this firmware.
        let via = HIDClass::new(bus, VIA_DESC, 1);
        let consumer = HIDClass::new_ep_in(bus, MediaKeyboardReport::desc(), 1);
        let system = HIDClass::new_ep_in(bus, SYSTEM_DESC, 1);
        let device = UsbDeviceBuilder::new(bus, UsbVidPid(USB_VID, USB_PID))
            .manufacturer("NuPhy")
            .product("Air96 V2 Keyboard")
            .serial_number("v4.7.4")
            .device_class(0x00)
            .device_sub_class(0x00)
            .device_protocol(0x00)
            // Advertise remote-wakeup so the host enables DEVICE_REMOTE_WAKEUP
            // before suspending — required for keypress-to-wake (see remote_wakeup()).
            .supports_remote_wakeup(true)
            .build();

        Self {
            device,
            keyboard,
            via,
            consumer,
            system,
            led_state: 0,
            suspended: false,
            just_suspended: false,
            just_resumed: false,
            pending_keyboard_report: None,
        }
    }

    /// Poll the USB device and HID classes. Returns true if data was exchanged.
    /// Also detects suspend/resume transitions and processes host LED reports.
    /// `rgb` is the live RGB matrix state, passed through to the VIA dispatch
    /// on RAW HID OUT reports so channel-3 (RGB matrix) values read from and
    /// write to the live state owned by the caller. `save_pending` is set when
    /// VIA's `ID_CUSTOM_SAVE` arrives so the main loop can flush to flash during
    /// an idle tick (consistent with how firmware-side config changes are
    /// already deferred).
    pub fn poll(
        &mut self,
        rgb: &mut crate::led::rgb::RgbMatrix,
        save_pending: &mut bool,
    ) -> bool {
        let was_suspended = self.suspended;

        let result = self.device.poll(&mut [
            &mut self.keyboard,
            &mut self.via,
            &mut self.consumer,
            &mut self.system,
        ]);

        // Suspend / resume edge detection (latched; consumed by take_suspend_edge).
        // Mirrors QMK's suspend_power_down_kb / suspend_wakeup_init_kb hooks so
        // the main loop can react on the same cycle the host signal arrives,
        // not after the 1s sleep-handler debounce.
        let state = self.device.state();
        let is_suspended = state == UsbDeviceState::Suspend;
        self.suspended = is_suspended;
        if !was_suspended && is_suspended {
            self.just_suspended = true;
        } else if was_suspended && !is_suspended {
            self.just_resumed = true;
        }

        // Host keyboard LED state (NumLock/CapsLock/…) arrives on the keyboard
        // interface's interrupt OUT endpoint. The boot report has no Report ID,
        // so the 1-byte payload IS the LED bitmap. We must NOT call
        // `pull_raw_report` here: usbd-hid 0.6.2's SET_REPORT handler panics
        // (`copy_from_slice` with mismatched lengths), and `pull_raw_report`'s
        // own `data.copy_from_slice(&set_report_buf.buf)` panics the same way
        // even after the fact — see the comment at the top of this file.
        let mut led_buf = [0u8; 8];
        if let Ok(len) = self.keyboard.pull_raw_output(&mut led_buf) {
            if len >= 1 {
                self.led_state = led_buf[0];
            }
        }

        // RAW HID (VIA) command on the dedicated VIA interface's OUT endpoint.
        // No Report ID, so the 32-byte payload is the whole report. via_command()
        // mutates the buffer in place with the response, which we echo back on
        // the VIA IN endpoint.
        let mut cmd = [0u8; 32];
        if let Ok(len) = self.via.pull_raw_output(&mut cmd) {
            if len > 0 && crate::via::via_command(&mut cmd, rgb, save_pending) {
                let _ = self.via.push_raw_input(&cmd);
            }
        }

        // Flush any pending keyboard report
        self.flush_reports();

        result
    }

    /// Returns true if the device is in the configured state (ready for reports).
    pub fn configured(&self) -> bool {
        self.device.state() == UsbDeviceState::Configured
    }

    /// Returns true if the device is currently suspended.
    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Drive a device-initiated USB resume to wake a sleeping host (remote wakeup).
    ///
    /// No-op unless the bus is suspended AND the host enabled DEVICE_REMOTE_WAKEUP
    /// (which it only does because we advertise `supports_remote_wakeup`). Mirrors C
    /// `usb_lld_wakeup_host`: usb-device 0.2 / stm32-usbd expose no device-initiated
    /// resume, so we touch the STM32 USB peripheral directly — clear LP_MODE/FSUSP so
    /// the transceiver clock runs, drive RESUME (K-state) for ~5 ms (USB spec: 1–15 ms),
    /// then release it. The host then completes the resume and the next `poll()` exits
    /// Suspend. Returns true if a wakeup was driven.
    ///
    /// NOTE: the bit sequence is unvalidated on hardware — only the keypress-to-wake
    /// path exercises it; normal suspend/resume does not depend on it.
    pub fn remote_wakeup(&mut self) -> bool {
        if !self.suspended || !self.device.remote_wakeup_enabled() {
            return false;
        }
        let usb = unsafe { &*pac::USB::ptr() };
        usb.cntr
            .modify(|_, w| w.lpmode().clear_bit().fsusp().clear_bit().resume().set_bit());
        cortex_m::asm::delay(240_000); // ~5 ms at 48 MHz
        usb.cntr.modify(|_, w| w.resume().clear_bit());
        true
    }

    /// Returns (just_suspended, just_resumed) and clears the latches.
    /// Call once per main loop iteration after `poll()`.
    pub fn take_suspend_edge(&mut self) -> (bool, bool) {
        let edge = (self.just_suspended, self.just_resumed);
        self.just_suspended = false;
        self.just_resumed = false;
        edge
    }

    /// Return the current keyboard LED state as reported by the host.
    /// Bits: 0=NumLock, 1=CapsLock, 2=ScrollLock, 3=Compose, 4=Kana.
    pub fn host_led_state(&self) -> u8 {
        self.led_state
    }

    fn flush_reports(&mut self) {
        if let Some(report) = self.pending_keyboard_report {
            match self.keyboard.push_raw_input(&report) {
                Ok(_) => self.pending_keyboard_report = None,
                Err(UsbError::WouldBlock) => {}
                Err(_) => self.pending_keyboard_report = None,
            }
        }
    }

    /// Queue an 8-byte 6KRO boot keyboard report (no Report ID).
    pub fn send_keyboard(&mut self, modifiers: u8, keys: &[u8; 6]) {
        let mut report = [0u8; 8];
        report[0] = modifiers;
        report[1] = 0; // reserved
        report[2..8].copy_from_slice(keys);
        self.pending_keyboard_report = Some(report);
        self.flush_reports();
    }

    /// USB is a 6KRO boot keyboard (see module docs), so there is no NKRO report
    /// over the wire. Callers still hand us an NKRO bitmap for symmetry with the
    /// wireless path; collapse it back to the first six pressed keycodes and emit
    /// a standard boot report. `current_keys` never holds more than six entries,
    /// so this is lossless in practice.
    pub fn send_nkro(&mut self, modifiers: u8, bitmap: &[u8; 31]) {
        let mut keys = [0u8; 6];
        let mut n = 0;
        'scan: for (byte_idx, &b) in bitmap.iter().enumerate() {
            if b == 0 {
                continue;
            }
            for bit in 0..8 {
                if b & (1 << bit) != 0 {
                    keys[n] = (byte_idx * 8 + bit) as u8;
                    n += 1;
                    if n == 6 {
                        break 'scan;
                    }
                }
            }
        }
        self.send_keyboard(modifiers, &keys);
    }

    pub fn send_consumer(&mut self, usage: u16) {
        let report = MediaKeyboardReport { usage_id: usage };
        let _ = self.consumer.push_input(&report);
    }

    pub fn send_system(&mut self, usage: u16) {
        // 2-byte little-endian array selector, no Report ID (see SYSTEM_DESC).
        // `usage` is the raw System Control usage code; 0 releases.
        let _ = self.system.push_raw_input(&usage.to_le_bytes());
    }

    pub fn release_all(&mut self) {
        self.send_keyboard(0, &[0; 6]);
        self.send_consumer(0);
        self.send_system(0);
    }
}
