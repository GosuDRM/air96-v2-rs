//! Enumerates the Rust USB stack through a mock bus, the way a host would,
//! and compares the bytes with the C VIA firmware's descriptors (extracted
//! from its ELF: `DeviceDescriptor`, `ConfigurationDescriptor`, report
//! descriptors) and with QMK's HID request handling.

use crate::usb_hid::{self, Interfaces};
use std::collections::VecDeque;
use std::sync::Mutex;
use usb_device::bus::{PollResult, UsbBus, UsbBusAllocator};
use usb_device::device::UsbDevice;
use usb_device::endpoint::{EndpointAddress, EndpointType};
use usb_device::{UsbDirection, UsbError};

const C_DEVICE: &str = "1201000200000040f5196632290301020001";
const C_CONFIG: &str = "09027400040100a0fa09040000010301010009211101000122440007058103080001090401000203000000092111010001222200070582032000010705030320000109040200010300000009211101000122b6000705840320000109040300010300000009211101000122150007058503200001";

#[derive(Default)]
struct State {
    setup: Option<[u8; 8]>,
    ep0_out: VecDeque<Vec<u8>>,
    ep_in: Vec<(u8, Vec<u8>)>,
    in_complete: u16,
    stalled: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap().get_or_insert_with(State::default))
}

struct MockBus;

impl UsbBus for MockBus {
    fn alloc_ep(&mut self, dir: UsbDirection, addr: Option<EndpointAddress>, _t: EndpointType, _size: u16, _i: u8) -> usb_device::Result<EndpointAddress> {
        Ok(addr.unwrap_or(EndpointAddress::from_parts(0, dir)))
    }
    fn enable(&mut self) {}
    fn reset(&self) {}
    fn set_device_address(&self, _a: u8) {}
    fn write(&self, ep: EndpointAddress, buf: &[u8]) -> usb_device::Result<usize> {
        st(|s| {
            s.ep_in.push((ep.into(), buf.to_vec()));
            s.in_complete |= 1 << ep.index();
        });
        Ok(buf.len())
    }
    fn read(&self, ep: EndpointAddress, buf: &mut [u8]) -> usb_device::Result<usize> {
        st(|s| {
            if ep.index() == 0 {
                if let Some(setup) = s.setup.take() {
                    buf[..8].copy_from_slice(&setup);
                    return Ok(8);
                }
                if let Some(d) = s.ep0_out.pop_front() {
                    buf[..d.len()].copy_from_slice(&d);
                    return Ok(d.len());
                }
            }
            Err(UsbError::WouldBlock)
        })
    }
    fn set_stalled(&self, ep: EndpointAddress, stalled: bool) {
        if ep.index() == 0 && stalled {
            st(|s| s.stalled = true);
        }
    }
    fn is_stalled(&self, _ep: EndpointAddress) -> bool {
        false
    }
    fn suspend(&self) {}
    fn resume(&self) {}
    fn poll(&self) -> PollResult {
        st(|s| {
            let ep_setup = s.setup.is_some() as u16;
            let ep_out = (!s.ep0_out.is_empty()) as u16;
            let ep_in_complete = core::mem::take(&mut s.in_complete);
            if ep_setup | ep_out | ep_in_complete == 0 {
                PollResult::None
            } else {
                PollResult::Data { ep_out, ep_in_complete, ep_setup }
            }
        })
    }
}

fn now() -> u32 {
    0
}

/// Run one control transfer; returns (EP0 IN data, stalled).
fn control(dev: &mut UsbDevice<'_, MockBus>, ifs: &mut Interfaces<'_, MockBus>, setup: [u8; 8], data_out: &[u8]) -> (Vec<u8>, bool) {
    st(|s| {
        s.setup = Some(setup);
        s.ep_in.clear();
        s.stalled = false;
        if !data_out.is_empty() {
            s.ep0_out.push_back(data_out.to_vec());
        }
    });
    let host_to_device = setup[0] & 0x80 == 0;
    for i in 0..20 {
        dev.poll(&mut ifs.as_classes());
        // Status stage of an IN transfer: host sends a zero-length OUT.
        if !host_to_device && i == 6 {
            st(|s| s.ep0_out.push_back(vec![]));
        }
    }
    st(|s| {
        let data: Vec<u8> = s.ep_in.iter().filter(|(a, _)| *a == 0x80).flat_map(|(_, d)| d.clone()).collect();
        (data, s.stalled)
    })
}

fn get_descriptor(dev: &mut UsbDevice<'_, MockBus>, ifs: &mut Interfaces<'_, MockBus>, recipient: u8, kind: u8, index: u16, len: u16) -> Vec<u8> {
    let setup = [0x80 | recipient, 0x06, 0, kind, index as u8, (index >> 8) as u8, len as u8, (len >> 8) as u8];
    control(dev, ifs, setup, &[]).0
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn usb_descriptors_match_c_via_build() {
    let alloc = UsbBusAllocator::new(MockBus);
    let (mut dev, mut ifs) = usb_hid::build(&alloc, now);

    // Device descriptor: identical to C except bcdDevice (= firmware version).
    let d = get_descriptor(&mut dev, &mut ifs, 0, 1, 0, 18);
    let mut expect: Vec<u8> = (0..18).map(|i| u8::from_str_radix(&C_DEVICE[2 * i..2 * i + 2], 16).unwrap()).collect();
    let v = usb_hid::device_version();
    expect[12] = v as u8;
    expect[13] = (v >> 8) as u8;
    assert_eq!(hex(&d), hex(&expect));

    let c = get_descriptor(&mut dev, &mut ifs, 0, 2, 0, 255);
    assert_eq!(hex(&c), C_CONFIG);

    // HID report descriptors per interface.
    for (iface, desc) in [
        (0u16, &usb_hid::KEYBOARD_REPORT_DESC[..]),
        (1, &usb_hid::RAW_REPORT_DESC[..]),
        (2, &usb_hid::SHARED_REPORT_DESC[..]),
        (3, &usb_hid::CONSOLE_REPORT_DESC[..]),
    ] {
        let r = get_descriptor(&mut dev, &mut ifs, 1, 0x22, iface, 255);
        assert_eq!(r, desc, "interface {iface}");
    }

    // String descriptors: manufacturer and product as keyboard.json.
    let utf16 = |s: &str| -> Vec<u8> {
        let mut v = vec![(2 + 2 * s.len()) as u8, 3];
        v.extend(s.encode_utf16().flat_map(|c| c.to_le_bytes()));
        v
    };
    let setup = |idx: u8| [0x80, 0x06, idx, 3, 0x09, 0x04, 255, 0];
    assert_eq!(control(&mut dev, &mut ifs, setup(1), &[]).0, utf16("NuPhy"));
    assert_eq!(control(&mut dev, &mut ifs, setup(2), &[]).0, utf16("Air96 V2"));
}

/// QMK HID class requests: GET_PROTOCOL only on the keyboard interface,
/// GET_REPORT/IDLE stall on interfaces without report storage, SET_REPORT
/// LED parsing on the keyboard and shared (report ID 6) interfaces.
#[test]
fn usb_hid_requests_follow_qmk() {
    use crate::usb_hid::{set_report_led, Kind, ReportStore};
    // SET_REPORT LED parsing (set_led_transfer_cb)
    assert_eq!(set_report_led(1, &[0x02]), Some(0x02));
    assert_eq!(set_report_led(2, &[6, 0x01]), Some(0x01));
    assert_eq!(set_report_led(2, &[1, 0x03]), Some(0x03));
    assert_eq!(set_report_led(2, &[2, 0x03]), None, "mouse report ID carries no LEDs");

    // Report storage: keyboard 8 bytes, shared per ID; raw/console none.
    let mut kb = ReportStore::new(Kind::Keyboard);
    assert_eq!(kb.get_report(0).unwrap().length, 8);
    assert!(ReportStore::new(Kind::Raw).get_report(0).is_none());
    assert!(ReportStore::new(Kind::Console).get_idle(0).is_none());
    let mut sh = ReportStore::new(Kind::Shared);
    assert_eq!(sh.get_report(6).unwrap().length, 32);
    assert_eq!(sh.get_report(4).unwrap().length, 3);
    assert!(sh.get_report(9).is_none(), "report ID beyond REPORT_ID_COUNT stalls");

    // Last sent report is returned and resent after the idle period.
    sh.set_report(&[4, 0xE9, 0x00], 100);
    assert_eq!(&sh.get_report(4).unwrap().data[..3], &[4, 0xE9, 0]);
    assert!(sh.set_idle(4, 25)); // 100ms
    assert_eq!(sh.get_idle(4), Some(25));
    assert!(sh.idle_due(150, true).is_empty());
    let due = sh.idle_due(200, true);
    assert_eq!(&due[0].0[..3], &[4, 0xE9, 0]);
    // Shared endpoint ignores SET_IDLE with report ID 0.
    assert!(sh.set_idle(0, 10));
    assert_eq!(sh.get_idle(0), Some(0));
    kb.set_idle(0, 10);
    assert_eq!(kb.get_idle(0), Some(10));
}

#[test]
fn device_version_is_bcd() {
    let v = usb_hid::device_version();
    let s = env!("CARGO_PKG_VERSION");
    let p: Vec<u16> = s.split('.').map(|x| x.parse().unwrap()).collect();
    assert_eq!(v, (p[0] << 8) | (p[1] << 4) | p[2]);
}
