//! USB HID keyboard driver — wired mode support.
//!
//! Uses the STM32F072's built-in USB FS peripheral (PA11=DM, PA12=DP)
//! to present a composite HID device with three interfaces:
//!   1. Boot keyboard (8-byte reports) — standard keys + LED state
//!   2. Consumer control (2-byte reports) — media keys, volume, brightness
//!   3. System control (2-byte reports) — sleep, power, DND
//!
//! Works on Windows, Linux, and macOS without drivers.

use stm32f0xx_hal::usb::UsbBusType;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::UsbDeviceState;
use usb_device::prelude::*;
use usbd_hid::descriptor::generator_prelude::*;
use usbd_hid::descriptor::MediaKeyboardReport;
use usbd_hid::descriptor::SerializedDescriptor;
use usbd_hid::hid_class::HIDClass;

const USB_VID: u16 = 0x19F5;
const USB_PID: u16 = 0x3266;

/// System control report with 16-bit usage_id (matches QMK's report_extra_t).
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

/// Combined Keyboard Descriptor (107 bytes)
///
/// DO NOT MODIFY — USB NKRO combined descriptor (Report ID 1 + 2).
/// Adding interfaces, changing report IDs, or altering the descriptor
/// layout breaks USB enumeration on STM32F072. Verified working at v3.8.x.
///
/// Merges Report ID 1 (standard boot keyboard) and Report ID 2 (NKRO keyboard bitmap)
/// into a single HID interface to prevent USB endpoint/interface limitations.
pub const COMBINED_KEYBOARD_DESC: &[u8] = &[
    0x05, 0x01,       // USAGE_PAGE (Generic Desktop)
    0x09, 0x06,       // USAGE (Keyboard)
    0xA1, 0x01,       // COLLECTION (Application)
    0x85, 0x01,       //   REPORT_ID (1)
    // Modifiers (8 bits)
    0x05, 0x07,       //   USAGE_PAGE (Keyboard)
    0x19, 0xE0,       //   USAGE_MINIMUM (Keyboard Left Control)
    0x29, 0xE7,       //   USAGE_MAXIMUM (Keyboard Right GUI)
    0x15, 0x00,       //   LOGICAL_MINIMUM (0)
    0x25, 0x01,       //   LOGICAL_MAXIMUM (1)
    0x75, 0x01,       //   REPORT_SIZE (1)
    0x95, 0x08,       //   REPORT_COUNT (8)
    0x81, 0x02,       //   INPUT (Data,Var,Abs)
    // Reserved byte
    0x95, 0x01,       //   REPORT_COUNT (1)
    0x75, 0x08,       //   REPORT_SIZE (8)
    0x81, 0x03,       //   INPUT (Cnst,Var,Abs)
    // LEDs (5 bits + 3 bits padding)
    0x05, 0x08,       //   USAGE_PAGE (LEDs)
    0x19, 0x01,       //   USAGE_MINIMUM (Num Lock)
    0x29, 0x05,       //   USAGE_MAXIMUM (Kana)
    0x25, 0x01,       //   LOGICAL_MAXIMUM (1)
    0x75, 0x01,       //   REPORT_SIZE (1)
    0x95, 0x05,       //   REPORT_COUNT (5)
    0x91, 0x02,       //   OUTPUT (Data,Var,Abs)
    0x95, 0x01,       //   REPORT_COUNT (1)
    0x75, 0x03,       //   REPORT_SIZE (3)
    0x91, 0x03,       //   OUTPUT (Cnst,Var,Abs)
    // Keycodes (6 bytes)
    0x05, 0x07,       //   USAGE_PAGE (Keyboard)
    0x19, 0x00,       //   USAGE_MINIMUM (Reserved (no event indicated))
    0x29, 0xDD,       //   USAGE_MAXIMUM (221)
    0x26, 0xFF, 0x00, //   LOGICAL_MAXIMUM (255)
    0x95, 0x06,       //   REPORT_COUNT (6)
    0x75, 0x08,       //   REPORT_SIZE (8)
    0x81, 0x00,       //   INPUT (Data,Arr,Abs)
    0xC0,             // END_COLLECTION

    0x05, 0x01,       // USAGE_PAGE (Generic Desktop)
    0x09, 0x06,       // USAGE (Keyboard)
    0xA1, 0x01,       // COLLECTION (Application)
    0x85, 0x02,       //   REPORT_ID (2)
    // Modifiers (8 bits)
    0x05, 0x07,       //   USAGE_PAGE (Keyboard)
    0x19, 0xE0,       //   USAGE_MINIMUM (Keyboard Left Control)
    0x29, 0xE7,       //   USAGE_MAXIMUM (Keyboard Right GUI)
    0x15, 0x00,       //   LOGICAL_MINIMUM (0)
    0x25, 0x01,       //   LOGICAL_MAXIMUM (1)
    0x75, 0x01,       //   REPORT_SIZE (1)
    0x95, 0x08,       //   REPORT_COUNT (8)
    0x81, 0x02,       //   INPUT (Data,Var,Abs)
    // Key bitmap (248 bits = 31 bytes)
    0x05, 0x07,       //   USAGE_PAGE (Keyboard)
    0x19, 0x00,       //   USAGE_MINIMUM (0)
    0x29, 0xF7,       //   USAGE_MAXIMUM (247)
    0x15, 0x00,       //   LOGICAL_MINIMUM (0)
    0x25, 0x01,       //   LOGICAL_MAXIMUM (1)
    0x75, 0x01,       //   REPORT_SIZE (1)
    0x95, 0xF8,       //   REPORT_COUNT (248)
    0x81, 0x02,       //   INPUT (Data,Var,Abs)
    0xC0,             // END_COLLECTION
];

pub struct UsbHid<'a> {
    device: UsbDevice<'a, UsbBusType>,
    keyboard: HIDClass<'a, UsbBusType>,
    consumer: HIDClass<'a, UsbBusType>,
    system: HIDClass<'a, UsbBusType>,
    /// Host-controlled keyboard LED state (bits: 0=NumLock, 1=CapsLock, 2=ScrollLock, 3=Compose, 4=Kana)
    led_state: u8,
    /// Tracks suspend state for edge detection
    suspended: bool,
    pending_keyboard_report: Option<[u8; 9]>,
    pending_nkro_report: Option<[u8; 33]>,
}

impl<'a> UsbHid<'a> {
    pub fn new(bus: &'a UsbBusAllocator<UsbBusType>) -> Self {
        let keyboard = HIDClass::new(bus, COMBINED_KEYBOARD_DESC, 1);
        let consumer = HIDClass::new(bus, MediaKeyboardReport::desc(), 8);
        let system  = HIDClass::new(bus, SystemControlReport16::desc(), 8);
        let device = UsbDeviceBuilder::new(bus, UsbVidPid(USB_VID, USB_PID))
            .manufacturer("GosuDRM")
            .product("Air96 V2 Keyboard")
            .serial_number("v3.9.4")
            .device_class(0x00)
            .device_sub_class(0x00)
            .device_protocol(0x00)
            .build();

        Self {
            device,
            keyboard,
            consumer,
            system,
            led_state: 0,
            suspended: false,
            pending_keyboard_report: None,
            pending_nkro_report: None,
        }
    }

    /// Poll the USB device and HID classes. Returns true if data was exchanged.
    /// Also detects suspend/resume transitions and processes host LED reports.
    pub fn poll(&mut self) -> bool {
        let _was_suspended = self.suspended;

        let result = self.device.poll(&mut [&mut self.keyboard, &mut self.consumer, &mut self.system]);

        // Suspend / resume detection
        let state = self.device.state();
        let is_suspended = state == UsbDeviceState::Suspend;
        self.suspended = is_suspended;

        // Read host LED state via SET_REPORT or OUT endpoint (boot keyboard)
        let mut led_buf = [0u8; 8];
        if let Ok(info) = self.keyboard.pull_raw_report(&mut led_buf) {
            if info.report_id == 1 {
                self.led_state = led_buf[0];
            }
        }
        if let Ok(len) = self.keyboard.pull_raw_output(&mut led_buf) {
            if len >= 2 && led_buf[0] == 1 {
                self.led_state = led_buf[1];
            }
        }

        // Flush any pending keyboard reports
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
        if self.pending_keyboard_report.is_none() {
            if let Some(report) = self.pending_nkro_report {
                match self.keyboard.push_raw_input(&report) {
                    Ok(_) => self.pending_nkro_report = None,
                    Err(UsbError::WouldBlock) => {}
                    Err(_) => self.pending_nkro_report = None,
                }
            }
        }
    }

    pub fn send_keyboard(&mut self, modifiers: u8, keys: &[u8; 6]) {
        let mut report = [0u8; 9];
        report[0] = 1; // Report ID 1
        report[1] = modifiers;
        report[2] = 0; // reserved
        report[3..9].copy_from_slice(keys);
        self.pending_keyboard_report = Some(report);
        self.flush_reports();
    }

    pub fn send_nkro(&mut self, modifiers: u8, bitmap: &[u8; 31]) {
        let mut report = [0u8; 33];
        report[0] = 2; // Report ID 2
        report[1] = modifiers;
        report[2..33].copy_from_slice(bitmap);
        self.pending_nkro_report = Some(report);
        self.flush_reports();
    }

    pub fn send_consumer(&mut self, usage: u16) {
        let report = MediaKeyboardReport { usage_id: usage };
        let _ = self.consumer.push_input(&report);
    }

    pub fn send_system(&mut self, usage: u16) {
        let report = SystemControlReport16 { usage_id: usage };
        let _ = self.system.push_input(&report);
    }

    pub fn release_all(&mut self) {
        self.send_keyboard(0, &[0; 6]);
        self.send_nkro(0, &[0; 31]);
        self.send_consumer(0);
        self.send_system(0);
    }
}
