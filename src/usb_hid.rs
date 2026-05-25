//! USB HID keyboard driver — wired mode support.
//!
//! Uses the STM32F072's built-in USB FS peripheral (PA11=DM, PA12=DP)
//! to present a composite HID device with three interfaces:
//!   1. Boot keyboard (8-byte reports) — standard keys
//!   2. Consumer control (2-byte reports) — media keys, volume, brightness
//!   3. System control (1-byte reports) — sleep, power, DND
//! Works on Windows, Linux, and macOS without drivers.

use stm32f0xx_hal::usb::UsbBusType;
use usb_device::bus::UsbBusAllocator;
use usb_device::prelude::*;
use usbd_hid::descriptor::KeyboardReport;
use usbd_hid::descriptor::MediaKeyboardReport;
use usbd_hid::descriptor::SerializedDescriptor;
use usbd_hid::descriptor::SystemControlReport;
use usbd_hid::hid_class::HIDClass;

const USB_VID: u16 = 0xFEED;
const USB_PID: u16 = 0x6060;

pub struct UsbHid<'a> {
    device: UsbDevice<'a, UsbBusType>,
    keyboard: HIDClass<'a, UsbBusType>,
    consumer: HIDClass<'a, UsbBusType>,
    system: HIDClass<'a, UsbBusType>,
}

impl<'a> UsbHid<'a> {
    pub fn new(bus: &'a UsbBusAllocator<UsbBusType>) -> Self {
        let keyboard = HIDClass::new(bus, KeyboardReport::desc(), 16);
        let consumer = HIDClass::new(bus, MediaKeyboardReport::desc(), 8);
        let system  = HIDClass::new(bus, SystemControlReport::desc(), 8);
        let device = UsbDeviceBuilder::new(bus, UsbVidPid(USB_VID, USB_PID))
            .manufacturer("GosuDRM")
            .product("Air96 V2 Keyboard")
            .serial_number("v3.0.6")
            .device_class(0x00)
            .device_sub_class(0x00)
            .device_protocol(0x00)
            .build();

        Self { device, keyboard, consumer, system }
    }

    pub fn poll(&mut self) -> bool {
        self.device.poll(&mut [&mut self.keyboard, &mut self.consumer, &mut self.system])
    }

    pub fn send_keyboard(&mut self, modifiers: u8, keys: &[u8; 6]) {
        let report = KeyboardReport {
            modifier: modifiers,
            reserved: 0,
            leds: 0,
            keycodes: *keys,
        };
        let _ = self.keyboard.push_input(&report);
    }

    pub fn send_consumer(&mut self, usage: u16) {
        let report = MediaKeyboardReport { usage_id: usage };
        let _ = self.consumer.push_input(&report);
    }

    pub fn send_system(&mut self, usage: u8) {
        let report = SystemControlReport { usage_id: usage };
        let _ = self.system.push_input(&report);
    }

    pub fn release_all(&mut self) {
        self.send_keyboard(0, &[0; 6]);
        self.send_consumer(0);
        self.send_system(0);
    }
}
