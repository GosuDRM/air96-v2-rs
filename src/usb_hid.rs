//! Wired USB — 1:1 port of the QMK ChibiOS USB layer as built for the C VIA
//! firmware (`tmk_core/protocol/usb_descriptor.c`, `chibios/usb_main.c`,
//! `usb_driver.c`, `usb_report_handling.c`).
//!
//! Device / configuration / HID descriptors are byte-identical to the C VIA
//! build (extracted from its ELF):
//!
//!   if 0  boot keyboard   EP1 IN  8 B   6KRO, LED output
//!   if 1  RAW (VIA)       EP2 IN 32 B + EP3 OUT 32 B
//!   if 2  shared          EP4 IN 32 B   mouse(2) system(3) consumer(4) NKRO(6)
//!   if 3  console         EP5 IN 32 B
//!
//! IN endpoints keep a 4-deep report queue and per-report storage for
//! GET_REPORT / idle resends, as QMK's buffered endpoints do.

use stm32f0xx_hal::pac;
use stm32f0xx_hal::usb::UsbBusType;
use usb_device::bus::UsbBusAllocator;
use usb_device::class_prelude::*;
use usb_device::control::{Recipient, RequestType};
use usb_device::device::UsbDeviceState;
use usb_device::prelude::*;
use usb_device::UsbDirection;

pub const USB_VID: u16 = 0x19F5;
pub const USB_PID: u16 = 0x3266;

/// `KeyboardReport` (boot keyboard, no report ID).
pub const KEYBOARD_REPORT_DESC: [u8; 68] = [
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01,
    0x95, 0x08, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x05, 0x07, 0x19, 0x00,
    0x29, 0xFF, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x06, 0x75, 0x08, 0x81, 0x00, 0x05, 0x08, 0x19,
    0x01, 0x29, 0x05, 0x15, 0x00, 0x25, 0x01, 0x95, 0x05, 0x75, 0x01, 0x91, 0x02, 0x95, 0x01, 0x75,
    0x03, 0x91, 0x01, 0xC0,
];

/// `RawReport` (VIA, usage page 0xFF60).
pub const RAW_REPORT_DESC: [u8; 34] = [
    0x06, 0x60, 0xFF, 0x09, 0x61, 0xA1, 0x01, 0x09, 0x62, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20,
    0x75, 0x08, 0x81, 0x02, 0x09, 0x63, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20, 0x75, 0x08, 0x91,
    0x02, 0xC0,
];

/// `SharedReport`: mouse (ID 2), system control (ID 3), consumer (ID 4), NKRO (ID 6).
pub const SHARED_REPORT_DESC: [u8; 182] = [
    0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01,
    0x29, 0x08, 0x15, 0x00, 0x25, 0x01, 0x95, 0x08, 0x75, 0x01, 0x81, 0x02, 0x05, 0x01, 0x09, 0x30,
    0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x95, 0x02, 0x75, 0x08, 0x81, 0x06, 0x09, 0x38, 0x15, 0x81,
    0x25, 0x7F, 0x95, 0x01, 0x75, 0x08, 0x81, 0x06, 0x05, 0x0C, 0x0A, 0x38, 0x02, 0x15, 0x81, 0x25,
    0x7F, 0x95, 0x01, 0x75, 0x08, 0x81, 0x06, 0xC0, 0xC0, 0x05, 0x01, 0x09, 0x80, 0xA1, 0x01, 0x85,
    0x03, 0x19, 0x01, 0x2A, 0xB7, 0x00, 0x15, 0x01, 0x26, 0xB7, 0x00, 0x95, 0x01, 0x75, 0x10, 0x81,
    0x00, 0xC0, 0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x04, 0x19, 0x01, 0x2A, 0xA0, 0x02, 0x15,
    0x01, 0x26, 0xA0, 0x02, 0x95, 0x01, 0x75, 0x10, 0x81, 0x00, 0xC0, 0x05, 0x01, 0x09, 0x06, 0xA1,
    0x01, 0x85, 0x06, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x95, 0x08, 0x75,
    0x01, 0x81, 0x02, 0x05, 0x07, 0x19, 0x00, 0x29, 0xEF, 0x15, 0x00, 0x25, 0x01, 0x95, 0xF0, 0x75,
    0x01, 0x81, 0x02, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x95, 0x05, 0x75, 0x01, 0x91, 0x02, 0x95,
    0x01, 0x75, 0x03, 0x91, 0x01, 0xC0,
];

/// `ConsoleReport` (hid_listen, usage page 0xFF31).
pub const CONSOLE_REPORT_DESC: [u8; 21] = [
    0x06, 0x31, 0xFF, 0x09, 0x74, 0xA1, 0x01, 0x09, 0x75, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20,
    0x75, 0x08, 0x81, 0x02, 0xC0,
];

pub const REPORT_ID_KEYBOARD: u8 = 1;
pub const REPORT_ID_MOUSE: u8 = 2;
pub const REPORT_ID_SYSTEM: u8 = 3;
pub const REPORT_ID_CONSUMER: u8 = 4;
pub const REPORT_ID_NKRO: u8 = 6;
/// `REPORT_ID_COUNT` (= REPORT_ID_DIGITIZER).
pub const REPORT_ID_COUNT: u8 = 8;

const HID_DESC_TYPE_HID: u8 = 0x21;
const HID_DESC_TYPE_REPORT: u8 = 0x22;
const HID_REQ_GET_REPORT: u8 = 0x01;
const HID_REQ_GET_IDLE: u8 = 0x02;
const HID_REQ_GET_PROTOCOL: u8 = 0x03;
const HID_REQ_SET_REPORT: u8 = 0x09;
const HID_REQ_SET_IDLE: u8 = 0x0A;
const HID_REQ_SET_PROTOCOL: u8 = 0x0B;

/// `USB_DEFAULT_BUFFER_CAPACITY`.
const IN_CAPACITY: usize = 4;
/// `send_report()` timeout.
const SEND_TIMEOUT_MS: u32 = 100;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Keyboard,
    Raw,
    Shared,
    Console,
}

/// `usb_fs_report_t`.
#[derive(Clone, Copy, Debug)]
pub struct StoredReport {
    pub data: [u8; 32],
    pub length: u8,
    pub idle_rate: u32,
    pub last_report: u32,
}

impl StoredReport {
    const fn new(length: u8) -> Self {
        Self { data: [0; 32], length, idle_rate: 0, last_report: 0 }
    }
    fn reset(&mut self) {
        self.data = [0; 32];
        self.idle_rate = 0;
        self.last_report = 0;
    }
}

/// Shared-endpoint storage size for each report ID (None = no storage).
pub const fn shared_storage_len(id: u8) -> Option<u8> {
    match id {
        REPORT_ID_MOUSE => Some(6),
        REPORT_ID_SYSTEM | REPORT_ID_CONSUMER => Some(3),
        REPORT_ID_NKRO => Some(32),
        _ => None,
    }
}

/// Per-interface report storage and the HID class-request rules of
/// `usb_report_handling.c` / `usb_requests_hook_cb()`, kept free of the USB
/// bus so they can be tested on the host.
#[derive(Clone, Copy, Debug)]
pub struct ReportStore {
    pub kind: Kind,
    pub storage: [Option<StoredReport>; REPORT_ID_COUNT as usize + 1],
    pub run_idle_task: bool,
    get_report_scratch: StoredReport,
}

impl ReportStore {
    pub fn new(kind: Kind) -> Self {
        let mut storage = [None; REPORT_ID_COUNT as usize + 1];
        match kind {
            Kind::Keyboard => storage[0] = Some(StoredReport::new(8)),
            Kind::Shared => {
                for (id, slot) in storage.iter_mut().enumerate() {
                    *slot = shared_storage_len(id as u8).map(StoredReport::new);
                }
            }
            _ => {}
        }
        Self { kind, storage, run_idle_task: false, get_report_scratch: StoredReport::new(0) }
    }

    pub fn has_storage(&self) -> bool {
        matches!(self.kind, Kind::Keyboard | Kind::Shared)
    }

    fn slot(&mut self, report_id: u8) -> Option<&mut StoredReport> {
        let idx = if self.kind == Kind::Shared { report_id as usize } else { 0 };
        self.storage.get_mut(idx).and_then(|s| s.as_mut())
    }

    pub fn reset(&mut self) {
        for s in self.storage.iter_mut().flatten() {
            s.reset();
        }
    }

    /// `usb_set_report()` / `usb_shared_set_report()` after a completed send.
    pub fn set_report(&mut self, data: &[u8], now: u32) {
        let id = data[0];
        if self.kind == Kind::Shared && id > REPORT_ID_COUNT {
            return;
        }
        if let Some(s) = self.slot(id) {
            s.last_report = now;
            s.length = data.len() as u8;
            s.data = [0; 32];
            s.data[..data.len()].copy_from_slice(data);
        }
    }

    /// `usb_get_report_cb()`: None = stall.
    pub fn get_report(&mut self, report_id: u8) -> Option<StoredReport> {
        if report_id > REPORT_ID_COUNT || !self.has_storage() {
            return None;
        }
        if let Some(s) = self.slot(report_id).copied() {
            self.get_report_scratch = s;
        }
        Some(self.get_report_scratch)
    }

    /// `usb_get_idle_cb()`: None = stall.
    pub fn get_idle(&mut self, report_id: u8) -> Option<u8> {
        if report_id > REPORT_ID_COUNT || !self.has_storage() {
            return None;
        }
        Some(self.slot(report_id).map_or(0, |s| (s.idle_rate / 4) as u8))
    }

    /// `usb_set_idle_cb()`: false = stall. The shared endpoint ignores report
    /// ID 0 (QMK deviates from the spec there).
    pub fn set_idle(&mut self, report_id: u8, idle_rate: u8) -> bool {
        if report_id > REPORT_ID_COUNT || !self.has_storage() {
            return false;
        }
        if !(self.kind == Kind::Shared && report_id == 0) {
            if let Some(s) = self.slot(report_id) {
                s.idle_rate = idle_rate as u32 * 4;
                self.run_idle_task |= idle_rate != 0;
            }
        }
        true
    }

    /// Reports due for an idle resend (`usb_idle_task()`), oldest ID first.
    pub fn idle_due(&mut self, now: u32, inactive: bool) -> heapless::Vec<([u8; 32], u8), 9> {
        let mut due = heapless::Vec::new();
        if !self.run_idle_task || !self.has_storage() {
            return due;
        }
        let ids: &[u8] = if self.kind == Kind::Shared { &[1, 2, 3, 4, 5, 6, 7, 8] } else { &[0] };
        let mut non_zero = false;
        for &id in ids {
            let Some(s) = self.slot(id).copied() else { continue };
            non_zero |= s.idle_rate != 0;
            if inactive && s.idle_rate != 0 && now.wrapping_sub(s.last_report) >= s.idle_rate {
                let _ = due.push((s.data, s.length));
            }
        }
        self.run_idle_task = non_zero;
        due
    }
}

/// `set_led_transfer_cb()`: LED byte from a SET_REPORT data stage, or None
/// when the report ID isn't the keyboard's or NKRO's.
pub fn set_report_led(w_length: u16, data: &[u8]) -> Option<u8> {
    let mut buf = [0u8; 2];
    let n = data.len().min(2);
    buf[..n].copy_from_slice(&data[..n]);
    if w_length == 2 {
        if buf[0] == REPORT_ID_KEYBOARD || buf[0] == REPORT_ID_NKRO {
            Some(buf[1])
        } else {
            None
        }
    } else {
        Some(buf[0])
    }
}

/// HID class descriptor body (bcdHID 1.11, country 0, one report descriptor).
pub fn hid_descriptor(report_len: usize) -> [u8; 7] {
    [0x11, 0x01, 0x00, 0x01, HID_DESC_TYPE_REPORT, report_len as u8, (report_len >> 8) as u8]
}

/// Requests QMK handles in its USB callbacks, collected for the main loop.
#[derive(Default)]
struct Events {
    led_state: Option<u8>,
    protocol: Option<u8>,
    keyboard_protocol: u8,
}

/// One QMK HID interface.
pub struct QmkHid<'a, B: UsbBus> {
    if_num: InterfaceNumber,
    in_ep: EndpointIn<'a, B>,
    out_ep: Option<EndpointOut<'a, B>>,
    descriptor: &'static [u8],
    queue: heapless::Deque<([u8; 32], u8), IN_CAPACITY>,
    in_flight: Option<([u8; 32], u8)>,
    timed_out: bool,
    store: ReportStore,
    rx: Option<[u8; 32]>,
    events: Events,
    now: fn() -> u32,
}

impl<'a, B: UsbBus> QmkHid<'a, B> {
    #[inline(never)]
    fn new(
        alloc: &'a UsbBusAllocator<B>,
        kind: Kind,
        descriptor: &'static [u8],
        in_ep: (usize, u16),
        out_ep: Option<(usize, u16)>,
        now: fn() -> u32,
    ) -> Self {
        let if_num = alloc.interface();
        let in_ep = alloc
            .alloc(Some(EndpointAddress::from_parts(in_ep.0, UsbDirection::In)), EndpointType::Interrupt, in_ep.1, 1)
            .expect("IN endpoint");
        let out_ep = out_ep.map(|(n, size)| {
            alloc
                .alloc(Some(EndpointAddress::from_parts(n, UsbDirection::Out)), EndpointType::Interrupt, size, 1)
                .expect("OUT endpoint")
        });
        Self {
            if_num,
            in_ep,
            out_ep,
            descriptor,
            queue: heapless::Deque::new(),
            in_flight: None,
            timed_out: false,
            store: ReportStore::new(kind),
            rx: None,
            events: Events { keyboard_protocol: 1, ..Default::default() },
            now,
        }
    }

    fn kind(&self) -> Kind {
        self.store.kind
    }

    /// `usb_endpoint_in_is_inactive()`.
    fn inactive(&self) -> bool {
        self.in_flight.is_none() && self.queue.is_empty()
    }

    fn start_next(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        if let Some((buf, len)) = self.queue.pop_front() {
            match self.in_ep.write(&buf[..len as usize]) {
                Ok(_) => self.in_flight = Some((buf, len)),
                Err(_) => {
                    let _ = self.queue.push_front((buf, len));
                }
            }
        }
    }

    fn try_push(&mut self, data: &[u8]) -> bool {
        let mut buf = [0u8; 32];
        buf[..data.len()].copy_from_slice(data);
        if self.queue.push_back((buf, data.len() as u8)).is_err() {
            return false;
        }
        self.start_next();
        true
    }

    /// `obqResetI()`: drop queued reports.
    fn reset_queue(&mut self) {
        self.queue.clear();
    }

    fn idle_task(&mut self) {
        let inactive = self.inactive();
        for (buf, len) in self.store.idle_due((self.now)(), inactive) {
            self.try_push(&buf[..len as usize]);
        }
    }

    fn is_mine(&self, index: u16) -> bool {
        index == u8::from(self.if_num) as u16
    }
}

impl<'a, B: UsbBus> UsbClass<B> for QmkHid<'a, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()> {
        let (sub, proto) = if self.kind() == Kind::Keyboard { (1, 1) } else { (0, 0) };
        writer.interface(self.if_num, 0x03, sub, proto)?;
        writer.write(HID_DESC_TYPE_HID, &hid_descriptor(self.descriptor.len()))?;
        writer.endpoint(&self.in_ep)?;
        if let Some(ep) = &self.out_ep {
            writer.endpoint(ep)?;
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.reset_queue();
        self.in_flight = None;
        self.store.reset();
        self.rx = None;
    }

    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        if addr != self.in_ep.address() {
            return;
        }
        self.timed_out = false;
        if let Some((buf, len)) = self.in_flight.take() {
            if len > 0 {
                self.store.set_report(&buf[..len as usize], (self.now)());
            }
        }
        self.start_next();
    }

    fn endpoint_out(&mut self, addr: EndpointAddress) {
        let Some(ep) = &self.out_ep else { return };
        if addr != ep.address() {
            return;
        }
        let mut buf = [0u8; 32];
        if let Ok(n) = ep.read(&mut buf) {
            if n > 0 && self.rx.is_none() {
                self.rx = Some(buf);
            }
        }
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = *xfer.request();
        if req.recipient != Recipient::Interface || !self.is_mine(req.index) {
            return;
        }
        match (req.request_type, req.request) {
            (RequestType::Standard, usb_device::control::Request::GET_DESCRIPTOR) => match (req.value >> 8) as u8 {
                HID_DESC_TYPE_REPORT => {
                    xfer.accept_with_static(self.descriptor).ok();
                }
                HID_DESC_TYPE_HID => {
                    let mut d = [0u8; 9];
                    d[0] = 9;
                    d[1] = HID_DESC_TYPE_HID;
                    d[2..].copy_from_slice(&hid_descriptor(self.descriptor.len()));
                    xfer.accept_with(&d).ok();
                }
                _ => {}
            },
            (RequestType::Class, HID_REQ_GET_REPORT) => match self.store.get_report(req.value as u8) {
                Some(s) => {
                    xfer.accept_with(&s.data[..s.length as usize]).ok();
                }
                None => {
                    xfer.reject().ok();
                }
            },
            (RequestType::Class, HID_REQ_GET_PROTOCOL) => {
                if self.kind() == Kind::Keyboard {
                    let p = [self.events.keyboard_protocol];
                    xfer.accept_with(&p).ok();
                } else {
                    xfer.reject().ok();
                }
            }
            (RequestType::Class, HID_REQ_GET_IDLE) => match self.store.get_idle(req.value as u8) {
                Some(rate) => {
                    xfer.accept_with(&[rate]).ok();
                }
                None => {
                    xfer.reject().ok();
                }
            },
            _ => {}
        }
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let req = *xfer.request();
        if req.recipient != Recipient::Interface || req.request_type != RequestType::Class || !self.is_mine(req.index) {
            return;
        }
        match req.request {
            HID_REQ_SET_REPORT => {
                if !matches!(self.kind(), Kind::Keyboard | Kind::Shared) {
                    xfer.reject().ok();
                    return;
                }
                if let Some(led) = set_report_led(req.length, xfer.data()) {
                    self.events.led_state = Some(led);
                }
                xfer.accept().ok();
            }
            HID_REQ_SET_PROTOCOL => {
                if self.kind() == Kind::Keyboard {
                    self.events.keyboard_protocol = req.value as u8;
                    self.events.protocol = Some(req.value as u8);
                }
                xfer.accept().ok();
            }
            HID_REQ_SET_IDLE => {
                if self.store.set_idle(req.value as u8, (req.value >> 8) as u8) {
                    xfer.accept().ok();
                } else {
                    xfer.reject().ok();
                }
            }
            _ => {}
        }
    }
}

/// The four QMK interfaces, in interface-number order.
pub struct Interfaces<'a, B: UsbBus> {
    pub keyboard: QmkHid<'a, B>,
    pub raw: QmkHid<'a, B>,
    pub shared: QmkHid<'a, B>,
    pub console: QmkHid<'a, B>,
}

impl<'a, B: UsbBus> Interfaces<'a, B> {
    pub fn as_classes(&mut self) -> [&mut dyn UsbClass<B>; 4] {
        [&mut self.keyboard, &mut self.raw, &mut self.shared, &mut self.console]
    }
}

/// keyboard.json: manufacturer "NuPhy", keyboard_name "Air96 V2", no serial;
/// bcdDevice = firmware version (DEVICE_VER).
fn build_device<B: UsbBus>(bus: &UsbBusAllocator<B>) -> UsbDevice<'_, B> {
    UsbDeviceBuilder::new(bus, UsbVidPid(USB_VID, USB_PID))
        .manufacturer("NuPhy")
        .product("Air96 V2")
        .device_class(0x00)
        .device_sub_class(0x00)
        .device_protocol(0x00)
        .max_packet_size_0(64)
        .device_release(device_version())
        .self_powered(false)
        .supports_remote_wakeup(true)
        .max_power(500)
        .build()
}

/// Build the device exactly as the C VIA firmware enumerates. Interfaces and
/// endpoints are allocated in QMK's order so their numbers match.
pub fn build<B: UsbBus>(bus: &UsbBusAllocator<B>, now: fn() -> u32) -> (UsbDevice<'_, B>, Interfaces<'_, B>) {
    let ifs = Interfaces {
        keyboard: QmkHid::new(bus, Kind::Keyboard, &KEYBOARD_REPORT_DESC, (1, 8), None, now),
        raw: QmkHid::new(bus, Kind::Raw, &RAW_REPORT_DESC, (2, 32), Some((3, 32)), now),
        shared: QmkHid::new(bus, Kind::Shared, &SHARED_REPORT_DESC, (4, 32), None, now),
        console: QmkHid::new(bus, Kind::Console, &CONSOLE_REPORT_DESC, (5, 32), None, now),
    };
    let device = build_device(bus);
    (device, ifs)
}

/// An IN report and the interface it goes out on (host.c send_* layouts).
pub struct Report {
    kind: Kind,
    buf: [u8; 32],
    len: u8,
}

impl Report {
    fn new(kind: Kind, data: &[u8]) -> Self {
        let mut buf = [0u8; 32];
        buf[..data.len()].copy_from_slice(data);
        Self { kind, buf, len: data.len() as u8 }
    }

    /// `send_keyboard()`: 8-byte boot-format report on the keyboard endpoint.
    pub fn keyboard(report: &[u8; 8]) -> Self {
        Self::new(Kind::Keyboard, report)
    }

    /// `send_nkro()`: `report_nkro_t` = {REPORT_ID_NKRO, mods, bits[30]}.
    pub fn nkro(mods: u8, bits: &[u8; 30]) -> Self {
        let mut r = Self::new(Kind::Shared, &[REPORT_ID_NKRO, mods]);
        r.buf[2..].copy_from_slice(bits);
        r.len = 32;
        r
    }

    /// `send_mouse()`: {REPORT_ID_MOUSE, buttons, x, y, v, h}.
    pub fn mouse(report: &[u8; 5]) -> Self {
        let mut r = Self::new(Kind::Shared, &[REPORT_ID_MOUSE]);
        r.buf[1..6].copy_from_slice(report);
        r.len = 6;
        r
    }

    /// `send_extra()` with REPORT_ID_CONSUMER.
    pub fn consumer(usage: u16) -> Self {
        let [lo, hi] = usage.to_le_bytes();
        Self::new(Kind::Shared, &[REPORT_ID_CONSUMER, lo, hi])
    }

    /// `send_extra()` with REPORT_ID_SYSTEM.
    pub fn system(usage: u16) -> Self {
        let [lo, hi] = usage.to_le_bytes();
        Self::new(Kind::Shared, &[REPORT_ID_SYSTEM, lo, hi])
    }

    /// `raw_hid_send()`.
    pub fn raw(data: &[u8; 32]) -> Self {
        Self::new(Kind::Raw, data)
    }
}

pub struct UsbHid<'a> {
    device: UsbDevice<'a, UsbBusType>,
    keyboard: QmkHid<'a, UsbBusType>,
    raw: QmkHid<'a, UsbBusType>,
    shared: QmkHid<'a, UsbBusType>,
    console: QmkHid<'a, UsbBusType>,
    now: fn() -> u32,
    led_state: u8,
    suspended: bool,
    just_suspended: bool,
    just_resumed: bool,
    last_state: crate::hal::UsbDeviceState,
    state_changes: heapless::Vec<crate::hal::UsbDeviceState, 8>,
    protocol_change: Option<u8>,
}

impl<'a> UsbHid<'a> {
    /// Build the device in `slot` (in place: the struct is 3KB, too big to
    /// assemble on the stack and move). Interfaces and endpoints are
    /// allocated in QMK's order so their numbers match the C build.
    pub fn init(slot: &'a mut core::mem::MaybeUninit<Self>, bus: &'a UsbBusAllocator<UsbBusType>, now: fn() -> u32) -> &'a mut Self {
        let p = slot.as_mut_ptr();
        unsafe {
            core::ptr::addr_of_mut!((*p).keyboard).write(QmkHid::new(bus, Kind::Keyboard, &KEYBOARD_REPORT_DESC, (1, 8), None, now));
            core::ptr::addr_of_mut!((*p).raw).write(QmkHid::new(bus, Kind::Raw, &RAW_REPORT_DESC, (2, 32), Some((3, 32)), now));
            core::ptr::addr_of_mut!((*p).shared).write(QmkHid::new(bus, Kind::Shared, &SHARED_REPORT_DESC, (4, 32), None, now));
            core::ptr::addr_of_mut!((*p).console).write(QmkHid::new(bus, Kind::Console, &CONSOLE_REPORT_DESC, (5, 32), None, now));
            core::ptr::addr_of_mut!((*p).device).write(build_device(bus));
            core::ptr::addr_of_mut!((*p).now).write(now);
            core::ptr::addr_of_mut!((*p).led_state).write(0);
            core::ptr::addr_of_mut!((*p).suspended).write(false);
            core::ptr::addr_of_mut!((*p).just_suspended).write(false);
            core::ptr::addr_of_mut!((*p).just_resumed).write(false);
            core::ptr::addr_of_mut!((*p).last_state).write(crate::hal::UsbDeviceState::NoInit);
            core::ptr::addr_of_mut!((*p).state_changes).write(heapless::Vec::new());
            core::ptr::addr_of_mut!((*p).protocol_change).write(None);
            &mut *p
        }
    }

    /// Poll the USB stack (ChibiOS services this from its interrupt).
    #[inline(never)]
    pub fn poll(&mut self) -> bool {
        let was_suspended = self.suspended;
        let result = self.device.poll(&mut [&mut self.keyboard, &mut self.raw, &mut self.shared, &mut self.console]);

        let state = self.device.state();
        let qmk_state = match state {
            UsbDeviceState::Configured => crate::hal::UsbDeviceState::Configured,
            UsbDeviceState::Suspend => crate::hal::UsbDeviceState::Suspend,
            _ => crate::hal::UsbDeviceState::Init,
        };
        if qmk_state != self.last_state {
            if qmk_state == crate::hal::UsbDeviceState::Suspend {
                // usb_endpoint_in_suspend_cb()
                for c in [&mut self.keyboard, &mut self.raw, &mut self.shared, &mut self.console] {
                    c.reset_queue();
                    c.store.reset();
                }
            }
            self.last_state = qmk_state;
            if self.state_changes.push(qmk_state).is_err() {
                self.state_changes.remove(0);
                let _ = self.state_changes.push(qmk_state);
            }
        }
        let is_suspended = state == UsbDeviceState::Suspend;
        self.suspended = is_suspended;
        if !was_suspended && is_suspended {
            self.just_suspended = true;
        } else if was_suspended && !is_suspended {
            self.just_resumed = true;
        }

        // keyboard_led_state / keyboard_protocol are single globals in QMK.
        for c in [&mut self.keyboard, &mut self.shared] {
            if let Some(led) = c.events.led_state.take() {
                self.led_state = led;
            }
            if let Some(p) = c.events.protocol.take() {
                self.protocol_change = Some(p);
            }
        }
        result
    }

    /// `usb_idle_task()` (protocol_post_task).
    #[inline(never)]
    pub fn idle_task(&mut self) {
        self.keyboard.idle_task();
        self.shared.idle_task();
    }

    fn active(&self) -> bool {
        self.device.state() == UsbDeviceState::Configured
    }

    /// One attempt of `send_report()` → `usb_endpoint_in_send()`, which
    /// queues the report and waits up to 100ms for space; after a timeout,
    /// later sends don't wait until a transfer completes again. Returns None
    /// while the caller should keep waiting (with interrupts enabled, so the
    /// USB interrupt drains the endpoint and the millisecond clock runs).
    pub fn send_step(&mut self, r: &Report, start: u32) -> Option<bool> {
        if !self.active() {
            return Some(false);
        }
        let c = match r.kind {
            Kind::Keyboard => &mut self.keyboard,
            Kind::Raw => &mut self.raw,
            Kind::Shared => &mut self.shared,
            Kind::Console => &mut self.console,
        };
        let data = &r.buf[..r.len as usize];
        if c.try_push(data) {
            return Some(true);
        }
        if c.timed_out || (self.now)().wrapping_sub(start) >= SEND_TIMEOUT_MS {
            c.timed_out = true;
            c.reset_queue();
            return Some(c.try_push(data));
        }
        None
    }

    /// RAW HID command received since the last call (`raw_hid_task()`).
    pub fn take_via_command(&mut self) -> Option<[u8; 32]> {
        self.raw.rx.take()
    }

    /// QMK-equivalent device state changes since the last call.
    pub fn take_state_changes(&mut self) -> heapless::Vec<crate::hal::UsbDeviceState, 8> {
        core::mem::take(&mut self.state_changes)
    }

    /// SET_PROTOCOL from the host since the last call (`keyboard_protocol`).
    pub fn take_protocol_change(&mut self) -> Option<u8> {
        self.protocol_change.take()
    }

    pub fn qmk_state(&self) -> crate::hal::UsbDeviceState {
        self.last_state
    }

    pub fn remote_wakeup_enabled(&self) -> bool {
        self.device.remote_wakeup_enabled()
    }

    pub fn configured(&self) -> bool {
        self.device.state() == UsbDeviceState::Configured
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// `usbWakeupHost()`, first half: start resume signalling (K-state). The
    /// caller waits ~5ms with interrupts enabled, then calls
    /// `remote_wakeup_end()`.
    pub fn remote_wakeup_begin(&mut self) -> bool {
        if !self.suspended || !self.device.remote_wakeup_enabled() {
            return false;
        }
        let usb = unsafe { &*pac::USB::ptr() };
        usb.cntr.modify(|_, w| w.lpmode().clear_bit().fsusp().clear_bit().resume().set_bit());
        true
    }

    /// `usbWakeupHost()`, second half: stop resume signalling.
    pub fn remote_wakeup_end(&mut self) {
        let usb = unsafe { &*pac::USB::ptr() };
        usb.cntr.modify(|_, w| w.resume().clear_bit());
    }

    /// Returns (just_suspended, just_resumed) and clears the latches.
    pub fn take_suspend_edge(&mut self) -> (bool, bool) {
        let edge = (self.just_suspended, self.just_resumed);
        self.just_suspended = false;
        self.just_resumed = false;
        edge
    }

    /// `keyboard_leds()`.
    pub fn host_led_state(&self) -> u8 {
        self.led_state
    }
}

/// `DEVICE_VER` as BCD from the crate version (e.g. 4.9.0 → 0x0490).
pub const fn device_version() -> u16 {
    const V: &[u8] = env!("CARGO_PKG_VERSION").as_bytes();
    let mut parts = [0u16; 3];
    let mut i = 0;
    let mut p = 0;
    while i < V.len() && p < 3 {
        if V[i] == b'.' {
            p += 1;
        } else if V[i] >= b'0' && V[i] <= b'9' {
            parts[p] = parts[p] * 10 + (V[i] - b'0') as u16;
        } else {
            break;
        }
        i += 1;
    }
    (parts[0] << 8) | ((parts[1] & 0xF) << 4) | (parts[2] & 0xF)
}
