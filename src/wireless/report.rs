//! HID report sender — port of rf_driver.c
//!
//! Translates QMK-style HID reports (keyboard, NKRO, consumer, system, mouse)
//! into UART frames for the NRF module. Implements the host_driver_t interface
//! in Rust via function calls instead of function pointers.

use crate::wireless::uart::{
    UartProtocol, CMD_RPT_BYTE_KB, CMD_RPT_BIT_KB,
    CMD_RPT_CONSUME, CMD_RPT_SYS, CMD_RPT_MS,
    LinkMode,
};

/// Keyboard report — 6KRO (port of rf_send_keyboard, rf_driver.c:45)
/// Returns the UART frame length that should be transmitted.
pub fn send_keyboard(proto: &mut UartProtocol, modifiers: u8, keys: &[u8; 6]) -> usize {
    if proto.link_mode == LinkMode::Usb { return 0; }

    let report: [u8; 8] = [
        modifiers,
        0, // reserved
        keys[0], keys[1], keys[2], keys[3], keys[4], keys[5],
    ];
    proto.bytekb_report_buf.copy_from_slice(&report);
    proto.build_report(CMD_RPT_BYTE_KB, &report, 8)
}

/// NKRO report (port of rf_send_nkro, rf_driver.c:50)
pub fn send_nkro(proto: &mut UartProtocol, bits: &[u8; 32]) {
    if proto.link_mode == LinkMode::Usb { return; }

    let changed = proto.auto_nkey_send(bits);
    let bit_active = proto.f_bit_kb_act;
    // Copy out slices before mutable borrow for build_report
    let bit_report: [u8; 16] = proto.uart_bit_report_buf[..16].try_into().unwrap();
    let byte_report = proto.bytekb_report_buf;

    if changed {
        if bit_active {
            proto.build_report(CMD_RPT_BIT_KB, &bit_report, 16);
        }
        proto.build_report(CMD_RPT_BYTE_KB, &byte_report, 8);
    }
    proto.bitkb_report_buf.copy_from_slice(bits);
}

/// Consumer report — media keys (port of uart_send_consumer_report, rf.c:143)
pub fn send_consumer(proto: &mut UartProtocol, usage: u16) {
    if proto.link_mode == LinkMode::Usb { return; }
    let buf = usage.to_le_bytes();
    proto.build_report(CMD_RPT_CONSUME, &buf, 2);
}

/// System report — power/sleep/wake (port of uart_send_system_report, rf.c:161)
pub fn send_system(proto: &mut UartProtocol, usage: u16) {
    if proto.link_mode == LinkMode::Usb { return; }
    let buf = usage.to_le_bytes();
    proto.build_report(CMD_RPT_SYS, &buf, 2);
}

/// Mouse report (port of uart_send_mouse_report, rf.c:152)
pub fn send_mouse(proto: &mut UartProtocol, buttons: u8, x: i8, y: i8, wheel: i8, pan: i8) {
    if proto.link_mode == LinkMode::Usb { return; }
    let buf = [buttons, x as u8, y as u8, wheel as u8, pan as u8];
    proto.build_report(CMD_RPT_MS, &buf, 5);
}
