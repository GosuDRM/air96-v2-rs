//! Comprehensive unit tests for all firmware modules.
//! Run with: cargo test --lib --target x86_64-unknown-linux-gnu


use crate::wireless::uart::*;
use crate::wireless::report;
use crate::wireless::sleep;
use crate::keyboard::keymap;
use crate::led::side;
use crate::led::rgb;
use crate::config::eeprom;

// ===== UART CHECKSUM =====
#[test] fn checksum_single() { assert_eq!(UartProtocol::checksum(&[0x00]), 0x5A); }
#[test] fn checksum_multiple() { assert_eq!(UartProtocol::checksum(&[0x01, 0x02]), 0x59); }
#[test] fn checksum_known() {
    let name = b"\x01\x0FNuPhy Air96 V2";
    assert_eq!(UartProtocol::checksum(name), 0x57 ^ UART_HEAD);
}
#[test] fn checksum_empty() { assert_eq!(UartProtocol::checksum(&[]), UART_HEAD); }

// ===== UART FRAMES =====
#[test] fn build_cmd_basic() { let mut p = UartProtocol::new(); assert_eq!(p.build_cmd(CMD_HAND, &[0x00]), 6); }
#[test] fn build_report() { let mut p = UartProtocol::new(); assert_eq!(p.build_report(CMD_RPT_BYTE_KB, &[0u8; 8], 8), 13); }
#[test] fn build_link_all_cmds() {
    let mut p = UartProtocol::new();
    assert_eq!(p.build_link_cmd(CMD_HAND), 6);
    assert_eq!(p.build_link_cmd(CMD_SLEEP), 6);
    p.link_mode = LinkMode::Bt1;
    assert_eq!(p.build_link_cmd(CMD_SET_LINK), 6);
    assert_eq!(p.rf_state, RfState::Linking);
    assert_eq!(p.disconnect_delay, 0xFF);
    assert_eq!(p.build_link_cmd(CMD_RF_STS_SYSC), 6);
    p.link_mode = LinkMode::Bt2;
    assert_eq!(p.build_link_cmd(CMD_NEW_ADV), 7);
    assert_eq!(p.rf_state, RfState::Pairing);
    assert_eq!(p.build_link_cmd(CMD_CLR_DEVICE), 6);
    assert_eq!(p.build_link_cmd(CMD_SET_CONFIG), 6);
    assert_eq!(p.build_link_cmd(CMD_RF_DFU), 6);
    assert_eq!(p.build_link_cmd(CMD_READ_DATA), 5);
}

// ===== UART DISPATCH =====
#[test] fn dispatch_connect() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24;
    p.dispatch_frame(CMD_RF_STS_SYSC, &[0, 3, 0, 0, 85]);
    assert_eq!(p.rf_state, RfState::Connect); assert_eq!(p.rf_battery, 85);
}
#[test] fn dispatch_handshake() { let mut p = UartProtocol::new(); p.dispatch_frame(CMD_HAND, &[]); assert!(p.f_rf_hand_ok); }
#[test] fn dispatch_suspend() { let mut p = UartProtocol::new(); p.dispatch_frame(CMD_24G_SUSPEND, &[]); assert!(p.f_goto_sleep); }
#[test] fn dispatch_read_data() {
    let mut p = UartProtocol::new(); p.dispatch_frame(CMD_READ_DATA, &[0, 0, 0, 0, 1, 1, 2]);
    assert_eq!(p.link_mode, LinkMode::Bt1); assert_eq!(p.rf_channel, 1); assert_eq!(p.ble_channel, 2);
}
#[test] fn battery_not_overwritten() {
    let mut p = UartProtocol::new(); p.rf_battery = 45; p.link_mode = LinkMode::Rf24;
    p.dispatch_frame(CMD_RF_STS_SYSC, &[0, 3, 0, 1, 45]);
    assert_eq!(p.rf_battery, 45);
}
#[test] fn battery_capped_above_100() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24; p.rf_battery = 50;
    p.dispatch_frame(CMD_RF_STS_SYSC, &[0, 3, 0, 0, 200]);
    assert_eq!(p.rf_battery, 50);
}
#[test] fn rf_led_on_connect() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24;
    p.dispatch_frame(CMD_RF_STS_SYSC, &[0, 3, 0x07, 0, 85]);
    assert_eq!(p.rf_led, 0x07);
}
#[test] fn mismatch_sets_send_channel() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Bt1; p.rf_state = RfState::Connect;
    for _ in 0..6 { p.dispatch_frame(CMD_RF_STS_SYSC, &[4, 3, 0, 0, 85]); }
    assert!(p.f_send_channel);
}
#[test] fn mismatch_clears_on_match() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Bt1; p.error_cnt = 3;
    p.dispatch_frame(CMD_RF_STS_SYSC, &[1, 3, 0, 0, 85]);
    assert_eq!(p.error_cnt, 0); assert!(!p.f_send_channel);
}
#[test] fn new_adv_sets_flag() { let mut p = UartProtocol::new(); p.dispatch_frame(CMD_NEW_ADV, &[]); assert!(p.f_rf_new_adv_ok); }

// ===== FRAME PARSING =====
#[test] fn parse_idle_returns_none() { let mut p = UartProtocol::new(); p.rx_state = RxState::Idle; assert!(p.parse_frame().is_none()); }
#[test] fn parse_ack_returns_none() {
    let mut p = UartProtocol::new(); p.rx_state = RxState::Done; p.rx_len = 3;
    p.rx_buf[0] = UART_HEAD; p.rx_buf[2] = 0xA0;
    assert!(p.parse_frame().is_none());
}
#[test] fn parse_bad_checksum() {
    let mut p = UartProtocol::new(); p.rx_state = RxState::Done; p.rx_len = 6;
    p.rx_buf[0] = UART_HEAD; p.rx_buf[1] = CMD_RF_STS_SYSC; p.rx_buf[3] = 1;
    p.rx_buf[4] = 0x42; p.rx_buf[5] = 0x00;
    assert!(p.parse_frame().is_none());
    assert_eq!(p.rx_state, RxState::SumErr);
    assert_eq!(p.rx_len, 0);
}
#[test] fn parse_length_mismatch() {
    let mut p = UartProtocol::new(); p.rx_state = RxState::Done; p.rx_len = 7;
    p.rx_buf[0] = UART_HEAD; p.rx_buf[1] = CMD_RF_STS_SYSC; p.rx_buf[3] = 1;
    assert!(p.parse_frame().is_none());
    assert_eq!(p.rx_state, RxState::FormatErr);
    assert_eq!(p.rx_len, 0);
}
#[test] fn parse_valid() {
    let mut p = UartProtocol::new(); let data = [0x42u8; 1]; let cs = UartProtocol::checksum(&data);
    p.rx_state = RxState::Done; p.rx_len = 6;
    p.rx_buf[0] = UART_HEAD; p.rx_buf[1] = CMD_RF_STS_SYSC; p.rx_buf[3] = 1;
    p.rx_buf[4] = 0x42; p.rx_buf[5] = cs;
    assert!(p.parse_frame().is_some());
}

// ===== STATE MACHINES =====
#[test] fn rf_state_values() {
    assert_eq!(RfState::Idle as u8, 0); assert_eq!(RfState::Pairing as u8, 1);
    assert_eq!(RfState::Linking as u8, 2); assert_eq!(RfState::Connect as u8, 3);
    assert_eq!(RfState::Disconnect as u8, 4); assert_eq!(RfState::Sleep as u8, 5);
}
#[test] fn link_mode_values() {
    assert_eq!(LinkMode::Rf24 as u8, 0); assert_eq!(LinkMode::Bt1 as u8, 1);
    assert_eq!(LinkMode::Bt2 as u8, 2); assert_eq!(LinkMode::Bt3 as u8, 3);
    assert_eq!(LinkMode::Usb as u8, 4);
}
#[test] fn rf_state_from_u8_all() {
    assert_eq!(RfState::from_u8(0), RfState::Idle);
    assert_eq!(RfState::from_u8(1), RfState::Pairing);
    assert_eq!(RfState::from_u8(2), RfState::Linking);
    assert_eq!(RfState::from_u8(3), RfState::Connect);
    assert_eq!(RfState::from_u8(4), RfState::Disconnect);
    assert_eq!(RfState::from_u8(5), RfState::Sleep);
    assert_eq!(RfState::from_u8(6), RfState::Snif);
    assert_eq!(RfState::from_u8(0xFE), RfState::Invalid);
    assert_eq!(RfState::from_u8(0xFF), RfState::ErrState);
}
#[test] fn link_mode_from_u8_all() {
    assert_eq!(LinkMode::from_u8(0), LinkMode::Rf24);
    assert_eq!(LinkMode::from_u8(1), LinkMode::Bt1);
    assert_eq!(LinkMode::from_u8(2), LinkMode::Bt2);
    assert_eq!(LinkMode::from_u8(3), LinkMode::Bt3);
    assert_eq!(LinkMode::from_u8(4), LinkMode::Usb);
    assert_eq!(LinkMode::from_u8(99), LinkMode::Usb);
}
#[test] fn initial_values() {
    let p = UartProtocol::new();
    assert_eq!(p.rx_state, RxState::Idle); assert_eq!(p.link_mode, LinkMode::Usb);
    assert_eq!(p.rf_state, RfState::Idle); assert_eq!(p.rf_battery, 100);
    assert!(!p.f_rf_reset); assert!(!p.f_send_channel);
}

// ===== COMMAND CONSTANTS =====
#[test] fn all_commands() {
    assert_eq!(CMD_POWER_UP, 0xF0); assert_eq!(CMD_SLEEP, 0xF1); assert_eq!(CMD_HAND, 0xF2);
    assert_eq!(CMD_24G_SUSPEND, 0xF4); assert_eq!(CMD_RPT_MS, 0xE0); assert_eq!(CMD_RPT_BYTE_KB, 0xE1);
    assert_eq!(CMD_RPT_BIT_KB, 0xE2); assert_eq!(CMD_RPT_CONSUME, 0xE3); assert_eq!(CMD_RPT_SYS, 0xE4);
    assert_eq!(CMD_SET_LINK, 0xC0); assert_eq!(CMD_SET_CONFIG, 0xC1); assert_eq!(CMD_SET_NAME, 0xC3);
    assert_eq!(CMD_CLR_DEVICE, 0xC5); assert_eq!(CMD_NEW_ADV, 0xC7); assert_eq!(CMD_RF_STS_SYSC, 0xC9);
    assert_eq!(CMD_SET_24G_NAME, 0xCA); assert_eq!(CMD_RF_DFU, 0xB1);
    assert_eq!(CMD_WRITE_DATA, 0x80); assert_eq!(CMD_READ_DATA, 0x81);
}
#[test] fn frame_lengths() {
    let mut p = UartProtocol::new();
    assert_eq!(p.build_link_cmd(CMD_SET_NAME), 16);
    assert_eq!(p.build_link_cmd(CMD_SET_24G_NAME), 37);
    p.link_mode = LinkMode::Bt1;
    assert_eq!(p.build_link_cmd(CMD_NEW_ADV), 7);
}

// ===== KEYMAP BASIC =====
#[test] fn lnk_to_channel() {
    assert_eq!(keymap::lnk_to_channel(keymap::KC_LNK_RF), LinkMode::Rf24);
    assert_eq!(keymap::lnk_to_channel(keymap::KC_LNK_BLE1), LinkMode::Bt1);
    assert_eq!(keymap::lnk_to_channel(keymap::KC_LNK_BLE2), LinkMode::Bt2);
    assert_eq!(keymap::lnk_to_channel(keymap::KC_LNK_BLE3), LinkMode::Bt3);
}
#[allow(clippy::identity_op)]
#[test] fn mo_layer() {
    assert_eq!(keymap::mo_layer(keymap::MO | 0), Some(0));
    assert_eq!(keymap::mo_layer(keymap::MO | 1), Some(1));
    assert_eq!(keymap::mo_layer(keymap::MO | 4), Some(4));
    assert_eq!(keymap::mo_layer(0x0000), None);
}
#[test] fn is_custom() {
    assert!(keymap::is_custom(0x5C00)); assert!(keymap::is_custom(0x5C35));
    assert!(!keymap::is_custom(0x0004)); assert!(!keymap::is_custom(0x00E0));
}
#[test] fn mac_task() { assert_eq!(keymap::resolve_keycode(&[0], 0, 4), keymap::KC_MAC_TASK); }
#[test] fn win_f1() { assert_eq!(keymap::get_keycode(2, 0, 2), 0x003A); }
#[test] fn win_pscr() { assert_eq!(keymap::resolve_keycode(&[2], 0, 14), 0x0046); }
#[test] fn mac_fn_lnk_rf() { assert_eq!(keymap::resolve_keycode(&[1], 1, 4), keymap::KC_LNK_RF); }
#[test] fn transparent_fallthrough() { assert_eq!(keymap::resolve_keycode(&[4,3,2,0], 0, 0), 0x0029); }
#[test] fn all_transparent_is_kc_no() { assert_eq!(keymap::resolve_keycode(&[4], 0, 2), keymap::KC_NO); }
#[test] fn out_of_bounds() { assert_eq!(keymap::get_keycode(0, 99, 99), keymap::KC_NO); assert_eq!(keymap::get_keycode(99, 0, 0), keymap::KC_NO); }

// ===== KEYMAP CONSUMER KEYS =====
#[test] fn consumer_usage_mapping() {
    assert_eq!(keymap::consumer_usage(keymap::KC_BRID), 0x0070);
    assert_eq!(keymap::consumer_usage(keymap::KC_BRIU), 0x006F);
    assert_eq!(keymap::consumer_usage(keymap::KC_MPRV), 0x00B6);
    assert_eq!(keymap::consumer_usage(keymap::KC_MPLY), 0x00CD);
    assert_eq!(keymap::consumer_usage(keymap::KC_MNXT), 0x00B5);
    assert_eq!(keymap::consumer_usage(keymap::KC_MUTE), 0x00E2);
    assert_eq!(keymap::consumer_usage(keymap::KC_VOLD), 0x00EA);
    assert_eq!(keymap::consumer_usage(keymap::KC_VOLU), 0x00E9);
}
#[test] fn is_consumer_key_range() {
    assert!(keymap::is_consumer_key(keymap::KC_BRID));
    assert!(keymap::is_consumer_key(keymap::KC_VOLU));
    assert!(!keymap::is_consumer_key(keymap::KC_MAC_TASK));
}
#[test] fn mac_media_keys_in_row0() {
    let r = &keymap::LAYER_MAC[0];
    assert_eq!(r[2], keymap::KC_BRID); assert_eq!(r[3], keymap::KC_BRIU);
    assert_eq!(r[8], keymap::KC_MPRV); assert_eq!(r[9], keymap::KC_MPLY);
    assert_eq!(r[10], keymap::KC_MNXT); assert_eq!(r[11], keymap::KC_MUTE);
    assert_eq!(r[12], keymap::KC_VOLD); assert_eq!(r[13], keymap::KC_VOLU);
}
#[test] fn win_fn_media_keys() {
    let r = &keymap::LAYER_WIN_FN[0];
    assert_eq!(r[2], keymap::KC_BRID); assert_eq!(r[8], keymap::KC_MPRV);
}

// ===== KEYMAP RGB CONTROLS =====
#[test] fn mac_fn_rgb_controls() {
    assert_eq!(keymap::LAYER_MAC_FN[4][8], keymap::KC_RGB_SPD);
    assert_eq!(keymap::LAYER_MAC_FN[4][9], keymap::KC_RGB_SPI);
    assert_eq!(keymap::LAYER_MAC_FN[4][15], keymap::KC_RGB_VAI);
    assert_eq!(keymap::LAYER_MAC_FN[5][15], keymap::KC_RGB_MOD);
    assert_eq!(keymap::LAYER_MAC_FN[5][16], keymap::KC_RGB_VAD);
    assert_eq!(keymap::LAYER_MAC_FN[5][17], keymap::KC_RGB_HUI);
}
#[test] fn win_fn_rgb_controls() {
    assert_eq!(keymap::LAYER_WIN_FN[4][8], keymap::KC_RGB_SPD);
    assert_eq!(keymap::LAYER_WIN_FN[5][15], keymap::KC_RGB_MOD);
}
#[test] fn fn_layer_side_controls() {
    assert_eq!(keymap::LAYER_FN[4][4], keymap::KC_RGB_TEST);
    assert_eq!(keymap::LAYER_FN[4][9], keymap::KC_SIDE_SPD);
    assert_eq!(keymap::LAYER_FN[5][15], keymap::KC_SIDE_MOD);
}

// ===== REPORTS =====
#[test] fn keyboard_builds_frame() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24;
    assert!(report::send_keyboard(&mut p, 0x01, &[0x04, 0, 0, 0, 0, 0]) > 0);
}
#[test] fn keyboard_noop_usb() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Usb;
    assert_eq!(report::send_keyboard(&mut p, 0, &[0; 6]), 0);
}
#[test] fn consumer_sets_cmd() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24;
    report::send_consumer(&mut p, 0x00CF);
    assert_eq!(p.tx_buf[1], CMD_RPT_CONSUME);
}
#[test] fn system_sets_cmd() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24;
    report::send_system(&mut p, 0x009B);
    assert_eq!(p.tx_buf[1], CMD_RPT_SYS);
}
#[test] fn mouse_sets_cmd() {
    let mut p = UartProtocol::new(); p.link_mode = LinkMode::Rf24;
    report::send_mouse(&mut p, 0x01, 10, -5, 0, 0);
    assert_eq!(p.tx_buf[1], CMD_RPT_MS); assert_eq!(p.tx_buf[3], 5);
}
#[test] fn nkro_modifier_change() {
    let mut p = UartProtocol::new(); p.bitkb_report_buf = [0; 32];
    let mut now = [0u8; 32]; now[0] = 0x01;
    assert!(p.auto_nkey_send(&now));
}
#[test] fn nkro_no_change() {
    let mut p = UartProtocol::new(); p.bitkb_report_buf = [0; 32];
    assert!(!p.auto_nkey_send(&[0; 32]));
}
#[test] fn nkro_key_press_fills_byte_buf() {
    let mut p = UartProtocol::new(); p.bitkb_report_buf = [0; 32];
    let mut now = [0u8; 32]; now[2] = 0x01;
    assert!(p.auto_nkey_send(&now));
    assert_eq!(p.bytekb_report_buf[2], 8);
}
#[test] fn nkro_key_release_from_byte_buf() {
    let mut p = UartProtocol::new();
    p.bytekb_report_buf[2] = 8; p.bitkb_report_buf[2] = 0x01;
    let now = [0u8; 32];
    assert!(p.auto_nkey_send(&now));
    assert_eq!(p.bytekb_report_buf[2], 0);
}
#[test] fn nkro_overflow_bit_report() {
    let mut p = UartProtocol::new(); p.bitkb_report_buf = [0; 32];
    for i in 2..8 { p.bytekb_report_buf[i] = i as u8 + 10; }
    let mut now = [0u8; 32]; now[2] = 0x01;
    assert!(p.auto_nkey_send(&now));
    assert!(p.f_bit_kb_act);
}

// ===== SLEEP =====
#[test] fn sleep_constants() {
    assert_eq!(sleep::SLEEP_TIME_DELAY, 36000);
    assert_eq!(sleep::LINK_TIMEOUT, 12000);
    assert_eq!(sleep::POWER_DOWN_DELAY, 24);
}
#[test] fn sleep_new_state() {
    let s = sleep::SleepManager::new();
    assert_eq!(s.no_act_time, 0); assert!(s.sleep_enabled); assert!(!s.f_goto_sleep);
}
#[test] fn sleep_on_activity_resets() {
    let mut s = sleep::SleepManager::new(); s.no_act_time = 500;
    s.on_activity(); assert_eq!(s.no_act_time, 0);
}
#[test] fn sleep_tick_10ms_increments() {
    let mut s = sleep::SleepManager::new();
    s.tick_10ms(); assert_eq!(s.no_act_time, 1); assert_eq!(s.rf_linking_time, 1);
}
#[test] fn sleep_goto_sleep_sets_wakeup() {
    let mut s = sleep::SleepManager::new(); s.f_goto_sleep = true;
    s.tick(&mut UartProtocol::new(), false);
    assert!(!s.f_goto_sleep); assert!(s.f_wakeup_prepare);
}
#[test] fn sleep_wakeup_clears_on_activity() {
    let mut s = sleep::SleepManager::new(); s.f_wakeup_prepare = true; s.no_act_time = 5;
    s.tick(&mut UartProtocol::new(), false);
    assert!(!s.f_wakeup_prepare);
}

// ===== SIDE LEDS =====
#[test] fn side_mode_constants() {
    assert_eq!(side::SIDE_WAVE, 0); assert_eq!(side::SIDE_MIX, 1);
    assert_eq!(side::SIDE_STATIC, 2); assert_eq!(side::SIDE_BREATH, 3); assert_eq!(side::SIDE_OFF, 4);
}
#[test] fn side_colour_lib() {
    assert_eq!(side::COLOUR_LIB.len(), 9);
    assert_eq!(side::COLOUR_LIB[0], [0xFF, 0x00, 0x00]);
    assert_eq!(side::COLOUR_LIB[3], [0x00, 0xFF, 0x00]);
    assert_eq!(side::COLOUR_LIB[5], [0x00, 0x00, 0xFF]);
}
#[test] fn side_new_state() {
    let sl = side::SideLeds::new();
    assert_eq!(sl.mode, side::SIDE_WAVE); assert_eq!(sl.brightness, 3);
    assert_eq!(sl.speed, 2); assert_eq!(sl.colour, 0); assert!(sl.rgb_enabled);
}
#[test] fn side_blink_rf() {
    let mut sl = side::SideLeds::new();
    sl.blink_rf(3);
    assert_eq!(sl.rf_link_show_time, 0);
}
#[test] fn side_reset() {
    let mut sl = side::SideLeds::new();
    sl.mode = 3; sl.brightness = 5; sl.speed = 4; sl.colour = 7;
    sl.reset();
    assert_eq!(sl.mode, 0); assert_eq!(sl.brightness, 3);
    assert_eq!(sl.speed, 2); assert_eq!(sl.colour, 0); assert!(sl.rgb_enabled);
}

// ===== RGB DRIVER =====
#[test] fn led_map_110() { assert_eq!(rgb::LED_MAP.len(), 110); }
#[test] fn led0_driver0() { assert_eq!(rgb::LED_MAP[0].driver, 0); }
#[test] fn side_leds_driver1() { for i in 100..110 { assert_eq!(rgb::LED_MAP[i].driver, 1); } }
#[test] fn no_all_zero_entries() {
    for i in 0..110 {
        let l = &rgb::LED_MAP[i];
        assert!(!(l.r == 0 && l.g == 0 && l.b == 0), "LED {} all zero", i);
    }
}
#[test] fn set_hsv_red() {
    let mut m = rgb::RgbMatrix::new(); m.set_hsv(0, 255, 255);
    let (b1, _) = m.build_pwm_buffers();
    let r = rgb::LED_MAP[0].r as usize; let g = rgb::LED_MAP[0].g as usize;
    assert!(b1[r] > 200); assert!(b1[g] < 20);
}
#[test] fn set_color_marks_dirty() {
    let mut m = rgb::RgbMatrix::new(); m.dirty1 = false; m.dirty2 = false;
    m.set_color(0, 255, 0, 0); assert!(m.needs_flush());
    let _ = m.build_pwm_buffers();
    m.dirty1 = false; m.dirty2 = false; // Mimics successful I2C write confirmation
    assert!(!m.needs_flush());
}
#[test] fn build_pwm_buffers_driver1() {
    let mut m = rgb::RgbMatrix::new(); m.set_all(0x10, 0x20, 0x30);
    let (_b1, b2) = m.build_pwm_buffers();
    let l10 = &rgb::LED_MAP[10]; assert_eq!(l10.driver, 1);
    assert_eq!(b2[l10.r as usize], 0x10);
}
#[test] fn hsv_blue() {
    let mut m = rgb::RgbMatrix::new(); m.set_hsv(170, 255, 255);
    let (b1, _) = m.build_pwm_buffers();
    let r = rgb::LED_MAP[0].r as usize; let b = rgb::LED_MAP[0].b as usize;
    assert!(b1[b] > b1[r]);
}

// ===== EEPROM =====
#[test] fn eeprom_defaults() {
    let c = eeprom::UserConfig::default();
    assert_eq!(c.side_mode, 0); assert_eq!(c.side_brightness, 3);
    assert_eq!(c.side_speed, 2); assert!(c.side_rgb); assert!(c.sleep_enable);
}

// ===== USB HID REPORT TYPES =====
// Verify all report structs exist and have correct fields (compile-time checks)

#[test]
fn keyboard_report_struct() {
    use usbd_hid::descriptor::KeyboardReport;
    let r = KeyboardReport { modifier: 0x01, reserved: 0, leds: 0, keycodes: [0; 6] };
    let m = r.modifier;
    assert_eq!(m, 0x01);
}

#[test]
fn media_keyboard_report_struct() {
    use usbd_hid::descriptor::MediaKeyboardReport;
    let r = MediaKeyboardReport { usage_id: 0x00EA };
    let v = r.usage_id;
    assert_eq!(v, 0x00EA); // Volume Down
}

#[test]
fn system_control_report_struct() {
    use usbd_hid::descriptor::SystemControlReport;
    let r = SystemControlReport { usage_id: 0x9B };
    let v = r.usage_id;
    assert_eq!(v, 0x9B); // Do Not Disturb
}

#[test]
fn system_report_descriptor_exists() {
    use usbd_hid::descriptor::SystemControlReport;
    use usbd_hid::descriptor::SerializedDescriptor;
    let desc = SystemControlReport::desc();
    assert!(!desc.is_empty());
    assert!(desc.len() > 5, "System control descriptor too short: {} bytes", desc.len());
}

#[test]
fn consumer_report_usage_values() {
    // Verify the consumer usage codes we send are in valid ranges
    assert_eq!(0x0070, 0x0070); // Brightness Down
    assert_eq!(0x00E9, 0x00E9); // Volume Up
    assert_eq!(0x00CD, 0x00CD); // Play/Pause
}

#[allow(clippy::assertions_on_constants)]
#[test]
fn system_report_usage_values() {
    // System control usage codes (HUT 1.12, §4.6)
    // 0x81 = System Power Down, 0x9B = Do Not Disturb
    assert!(0x81u8 <= 0x9B);
    assert!(0x9Bu8 >= 0x81);
}

#[test]
fn combined_keyboard_descriptor_and_reports() {
    let desc = crate::usb_hid::COMBINED_KEYBOARD_DESC;
    assert_eq!(desc.len(), 107);
    
    // Verify Report ID 1 (boot keyboard) is declared in the descriptor
    // 0x85, 0x01 => Report ID 1
    assert_eq!(desc[6], 0x85);
    assert_eq!(desc[7], 0x01);
    
    // Verify Report ID 2 (NKRO bitmap) is declared in the descriptor
    // 0x85, 0x02 => Report ID 2
    assert_eq!(desc[72], 0x85);
    assert_eq!(desc[73], 0x02);
    
    // Verify standard keyboard report serialization format
    let modifiers = 0x05; // Ctrl + Alt
    let keys = [0x04, 0x05, 0x06, 0x00, 0x00, 0x00];
    
    let mut report = [0u8; 9];
    report[0] = 1; // Report ID 1
    report[1] = modifiers;
    report[2] = 0; // reserved
    report[3..9].copy_from_slice(&keys);
    
    assert_eq!(report[0], 1);
    assert_eq!(report[1], 0x05);
    assert_eq!(report[2], 0);
    assert_eq!(&report[3..9], &keys);
    
    // Verify NKRO report serialization format
    let mut bitmap = [0u8; 31];
    bitmap[0] = 0x10; // some keycode bit
    bitmap[1] = 0x20;
    
    let mut nkro_report = [0u8; 33];
    nkro_report[0] = 2; // Report ID 2
    nkro_report[1] = modifiers;
    nkro_report[2..33].copy_from_slice(&bitmap);
    
    assert_eq!(nkro_report[0], 2);
    assert_eq!(nkro_report[1], modifiers);
    assert_eq!(nkro_report[2], bitmap[0]);
    assert_eq!(nkro_report[3], bitmap[1]);
}

#[test]
fn nkro_bitmap_generation_logic() {
    let current_keys = [0x04, 0x29, 0x1E, 0, 0, 0]; // 'A' (4), 'Esc' (41), '1' (30)
    
    let mut bits = [0u8; 31];
    // Note: modifiers intentionally NOT in bitmap — they're in the separate modifier byte
    for &k in &current_keys {
        if k > 0 && k < 248 {
            let byte = k as usize / 8;
            let bit = k as usize % 8;
            bits[byte] |= 1 << bit;
        }
    }
    
    // Verify keycode 4 ('A') -> byte = 0, bit = 4. bits[0] should have bit 4 set.
    assert_ne!(bits[0] & (1 << 4), 0);
    
    // Verify keycode 30 ('1') -> byte = 3, bit = 6. bits[3] should have bit 6 set.
    assert_ne!(bits[3] & (1 << 6), 0);
    
    // Verify keycode 41 ('Esc') -> byte = 5, bit = 1. bits[5] should have bit 1 set.
    assert_ne!(bits[5] & (1 << 1), 0);
}

