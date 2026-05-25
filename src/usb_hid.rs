//! USB HID keyboard driver — wired mode support.
//!
//! Uses the STM32F072's built-in USB FS peripheral (PA11=DM, PA12=DP)
//! to present a composite HID device with four interfaces:
//!   1. Boot keyboard (8-byte reports) — standard keys + LED state
//!   2. NKRO keyboard (33-byte reports) — N-key rollover bitmap
//!   3. Consumer control (2-byte reports) — media keys, volume, brightness
//!   4. System control (2-byte reports) — sleep, power, DND
//! Works on Windows, Linux, and macOS without drivers.

use stm32f0xx_hal::usb::UsbBusType;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::UsbDeviceState;
use usb_device::prelude::*;
use usbd_hid::descriptor::generator_prelude::*;
use usbd_hid::descriptor::KeyboardReport;
use usbd_hid::descriptor::MediaKeyboardReport;
use usbd_hid::hid_class::HIDClass;

// ── Vendor / Product IDs ─────────────────────────────────────────

const USB_VID: u16 = 0x19F5;
const USB_PID: u16 = 0x3266;

// ── Custom HID report types ──────────────────────────────────────

/// System control report with 16-bit usage_id (matches QMK's report_extra_t).
///
/// Uses a 16-bit value so both consumer page (0x0001–0x029C) and
/// system control page (0x0081–0x00B7) can share the same report type.
#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = SYSTEM_CONTROL) = {
        (usage_min = 0x81, usage_max = 0xB7, logical_min = 1) = {
            #[item_settings data,array,absolute,not_null] usage_id=input;
        };
    }
)]
#[allow(dead_code)]
pub struct SystemControlReport16 {
    pub usage_id: u16,
}

/// NKRO (N-Key Rollover) report — 32-byte bitmap.
///
/// Each bit corresponds to one keycode (bit n = keycode n).
/// Modifiers are packed as 8 independent bits in the modifier field.
#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = KEYBOARD) = {
        (usage_page = KEYBOARD, usage_min = 0xE0, usage_max = 0xE7) = {
            #[packed_bits 8] #[item_settings data,variable,absolute] modifier=input;
        };
        (usage_min = 0x00, usage_max = 0xFF) = {
            #[item_settings data,array,absolute] bitmap=input;
        };
    }
)]
#[allow(dead_code)]
pub struct NkroReport {
    pub modifier: u8,
    pub bitmap: [u8; 32],
}

// ── USB HID device ───────────────────────────────────────────────

pub struct UsbHid<'a> {
    device: UsbDevice<'a, UsbBusType>,
    keyboard: HIDClass<'a, UsbBusType>,
    nkro: HIDClass<'a, UsbBusType>,
    consumer: HIDClass<'a, UsbBusType>,
    system: HIDClass<'a, UsbBusType>,
    /// Host-controlled keyboard LED state (bits: 0=NumLock, 1=CapsLock, 2=ScrollLock, 3=Compose, 4=Kana)
    led_state: u8,
    /// Tracks suspend state for edge detection
    suspended: bool,
}

impl<'a> UsbHid<'a> {
    pub fn new(bus: &'a UsbBusAllocator<UsbBusType>) -> Self {
        let keyboard = HIDClass::new(bus, KeyboardReport::desc(), 16);
        let nkro = HIDClass::new(bus, NkroReport::desc(), 16);
        let consumer = HIDClass::new(bus, MediaKeyboardReport::desc(), 8);
        let system = HIDClass::new(bus, SystemControlReport16::desc(), 8);
        let device = UsbDeviceBuilder::new(bus, UsbVidPid(USB_VID, USB_PID))
            .manufacturer("GosuDRM")
            .product("Air96 V2 Keyboard")
            .serial_number("v3.1.0")
            .device_class(0x00)
            .device_sub_class(0x00)
            .device_protocol(0x00)
            .build();

        Self {
            device,
            keyboard,
            nkro,
            consumer,
            system,
            led_state: 0,
            suspended: false,
        }
    }

    /// Poll the USB device and HID classes. Returns true if data was exchanged.
    /// Must be called at least every 10 ms while connected.
    ///
    /// Also detects suspend/resume transitions and processes host LED reports.
    pub fn poll(&mut self) -> bool {
        let was_suspended = self.suspended;

        let result = self.device.poll(&mut [
            &mut self.keyboard,
            &mut self.nkro,
            &mut self.consumer,
            &mut self.system,
        ]);

        // ── Suspend / resume detection ───────────────────────
        let state = self.device.state();
        let is_suspended = state == UsbDeviceState::Suspend;

        if is_suspended && !was_suspended {
            // Device suspended — stop sending reports
        } else if !is_suspended && was_suspended {
            // Device resumed — host may have reconnected
        }
        self.suspended = is_suspended;

        // ── Host LED state via SET_REPORT (boot keyboard) ────
        let mut led_buf = [0u8; 4];
        if self.keyboard.pull_raw_report(&mut led_buf).is_ok() {
            self.led_state = led_buf[0];
        }

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

    /// Return the current keyboard LED state as reported by the host.
    /// Bits: 0=NumLock, 1=CapsLock, 2=ScrollLock, 3=Compose, 4=Kana.
    pub fn host_led_state(&self) -> u8 {
        self.led_state
    }

    // ── Report senders ──────────────────────────────────────

    /// Send a standard 6KRO keyboard report (8 bytes).
    pub fn send_keyboard(&mut self, modifiers: u8, keys: &[u8; 6]) {
        let report = KeyboardReport {
            modifier: modifiers,
            reserved: 0,
            leds: 0,
            keycodes: *keys,
        };
        let _ = self.keyboard.push_input(&report);
    }

    /// Send an NKRO keyboard report (33 bytes: 1 modifier + 32 bitmap).
    pub fn send_nkro(&mut self, modifiers: u8, bitmap: &[u8; 32]) {
        let report = NkroReport {
            modifier: modifiers,
            bitmap: *bitmap,
        };
        let _ = self.nkro.push_input(&report);
    }

    /// Send a consumer control report (media keys, volume, etc.).
    pub fn send_consumer(&mut self, usage: u16) {
        let report = MediaKeyboardReport { usage_id: usage };
        let _ = self.consumer.push_input(&report);
    }

    /// Send a system control report (sleep, power, DND, etc.).
    /// Accepts 16-bit usage for compatibility with QMK's report_extra_t.
    pub fn send_system(&mut self, usage: u16) {
        let report = SystemControlReport16 { usage_id: usage };
        let _ = self.system.push_input(&report);
    }

    /// Release all keys across all interfaces (empty reports).
    pub fn release_all(&mut self) {
        self.send_keyboard(0, &[0; 6]);
        self.send_nkro(0, &[0; 32]);
        self.send_consumer(0);
        self.send_system(0);
    }
}
