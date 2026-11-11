//! Host tests. Run with: cargo test --lib --target x86_64-unknown-linux-gnu
//!
//! The behaviour tests drive the 1:1 C ports (`kb`, `wireless`, `led::side`)
//! through a mock board and an NRF52 model, and assert what the C firmware
//! (keyboards/air96_v2/ansi, v3.2.9) does on the wire and on the host.

use crate::config::eeprom;
use crate::hal::{Hal, UsbDeviceState};
use crate::kb::*;
use crate::keyboard::keymap;
use crate::keyboard::matrix::{Debouncer, Matrix, MATRIX_ROWS};
use crate::led::rgb;
use crate::wireless::uart::*;
use std::collections::VecDeque;

// ═════════════════════════════════════════════════════════════════════════
// Mock board + NRF52 model
// ═════════════════════════════════════════════════════════════════════════

struct Mock {
    t: u32,
    tx: Vec<(u32, Vec<u8>)>,
    rx: VecDeque<u8>,
    reset_log: Vec<(u32, bool)>,
    led_on: bool,
    dev_pin: bool,
    sys_pin: bool,
    usb_state: UsbDeviceState,
    usb_suspended: bool,
    usb_leds: u8,
    usb_nkro: Vec<(u8, [u8; 30])>,
    usb_kb: Vec<[u8; 8]>,
    usb_consumer: Vec<u16>,
    usb_system: Vec<u16>,
    usb_mouse: Vec<[u8; 5]>,
    wakeups: u32,
    remote_wakeup: bool,
    flushes: u32,
    cfgs: Vec<eeprom::UserConfig>,
    ee_inits: u32,
    keymap_raw: u16,
    resets: u32,
    dfu: u32,
    // NRF model
    nrf_auto: bool,
    nrf_saved: [u8; 3], // link_mode, rf_channel, ble_channel
    nrf_state: u8,
    nrf_led: u8,
    nrf_charge: u8,
    nrf_battery: u8,
    next_reply: Option<Vec<u8>>,
}

impl Mock {
    fn new() -> Self {
        Self {
            t: 1000,
            tx: vec![],
            rx: VecDeque::new(),
            reset_log: vec![],
            led_on: true,
            dev_pin: true,
            sys_pin: false,
            usb_state: UsbDeviceState::Configured,
            usb_suspended: false,
            usb_leds: 0,
            usb_nkro: vec![],
            usb_kb: vec![],
            usb_consumer: vec![],
            usb_system: vec![],
            usb_mouse: vec![],
            wakeups: 0,
            remote_wakeup: false,
            flushes: 0,
            cfgs: vec![],
            ee_inits: 0,
            keymap_raw: 0x1400,
            resets: 0,
            dfu: 0,
            nrf_auto: true,
            nrf_saved: [LINK_USB, LINK_BT_1, LINK_BT_1],
            nrf_state: RF_CONNECT,
            nrf_led: 0,
            nrf_charge: 0,
            nrf_battery: 80,
            next_reply: None,
        }
    }

    /// Frames sent with the given command / report type.
    fn sent(&self, cmd: u8) -> Vec<Vec<u8>> {
        self.tx.iter().filter(|(_, f)| f[1] == cmd).map(|(_, f)| f.clone()).collect()
    }

    fn reports(&self) -> Vec<Vec<u8>> {
        self.tx.iter().filter(|(_, f)| (CMD_RPT_MS..=CMD_RPT_SYS).contains(&f[1])).map(|(_, f)| f.clone()).collect()
    }
}

/// An NRF → keyboard frame: incoming checksum is the plain byte sum.
fn nrf_frame(cmd: u8, data: &[u8]) -> Vec<u8> {
    let mut f = vec![UART_HEAD, cmd, 0x00, data.len() as u8];
    f.extend_from_slice(data);
    f.push(data.iter().fold(0u8, |a, &b| a.wrapping_add(b)));
    f
}

impl Hal for Mock {
    fn now(&self) -> u32 {
        self.t
    }
    fn wait_ms(&mut self, ms: u32) {
        self.t += ms;
    }
    fn uart_send_bytes(&mut self, buf: &[u8]) {
        self.tx.push((self.t, buf.to_vec()));
        if let Some(r) = self.next_reply.take() {
            self.rx.extend(r);
            return;
        }
        if !self.nrf_auto {
            return;
        }
        let reply = match buf[1] {
            CMD_HAND | CMD_SET_NAME | CMD_SET_24G_NAME | CMD_SET_LINK | CMD_SLEEP | CMD_SET_CONFIG | CMD_CLR_DEVICE
            | CMD_NEW_ADV => vec![UART_HEAD, buf[1], 0xA0],
            CMD_READ_DATA => {
                let mut func = [0u8; 32];
                func[4..7].copy_from_slice(&self.nrf_saved);
                nrf_frame(CMD_READ_DATA, &func)
            }
            CMD_RF_STS_SYSC => {
                nrf_frame(CMD_RF_STS_SYSC, &[buf[4], self.nrf_state, self.nrf_led, self.nrf_charge, self.nrf_battery])
            }
            _ => vec![],
        };
        self.rx.extend(reply);
    }
    fn uart_read(&mut self) -> Option<u8> {
        self.rx.pop_front()
    }
    fn nrf_reset_pin(&mut self, high: bool) {
        self.reset_log.push((self.t, high));
    }
    fn led_power(&mut self, on: bool) {
        self.led_on = on;
    }
    fn read_dev_mode_pin(&mut self) -> bool {
        self.dev_pin
    }
    fn read_sys_mode_pin(&mut self) -> bool {
        self.sys_pin
    }
    fn usb_state(&self) -> UsbDeviceState {
        self.usb_state
    }
    fn usb_suspended(&self) -> bool {
        self.usb_suspended
    }
    fn usb_keyboard_leds(&self) -> u8 {
        self.usb_leds
    }
    fn usb_send_nkro(&mut self, mods: u8, bits: &[u8; 30]) {
        self.usb_nkro.push((mods, *bits));
        // Host model: a NumLock press toggles the host's NumLock LED.
        if bits[KC_NUM_LOCK as usize / 8] & (1 << (KC_NUM_LOCK % 8)) != 0 {
            self.usb_leds ^= 1;
        }
    }
    fn usb_send_keyboard(&mut self, report: &[u8; 8]) {
        self.usb_kb.push(*report);
    }
    fn usb_send_mouse(&mut self, report: &[u8; 5]) {
        self.usb_mouse.push(*report);
    }
    fn usb_send_consumer(&mut self, usage: u16) {
        self.usb_consumer.push(usage);
    }
    fn usb_send_system(&mut self, usage: u16) {
        self.usb_system.push(usage);
    }
    fn usb_remote_wakeup_enabled(&self) -> bool {
        self.remote_wakeup
    }
    fn usb_wakeup_host(&mut self) {
        self.wakeups += 1;
    }
    fn rgb_update_pwm_buffers(&mut self, _rgb: &mut rgb::RgbMatrix) {
        self.flushes += 1;
    }
    fn eeconfig_update(&mut self, cfg: &eeprom::UserConfig) {
        self.cfgs.push(*cfg);
    }
    fn eeconfig_init(&mut self) {
        self.ee_inits += 1;
    }
    fn eeconfig_read_keymap(&self) -> u16 {
        self.keymap_raw
    }
    fn eeconfig_update_keymap(&mut self, raw: u16) {
        self.keymap_raw = raw;
    }
    fn bootloader_jump(&mut self) {
        self.dfu += 1;
    }
    fn mcu_reset(&mut self) {
        self.resets += 1;
    }
}

/// Power on like the C firmware: keyboard_post_init_kb(), protocol_post_init().
fn boot(m: &mut Mock) -> Kb {
    crate::via::via_init(); // keyboard_init()
    let mut kb = Kb::new();
    kb.keymap_config_init(m);
    kb.keyboard_post_init_kb(m, None);
    kb.protocol_post_init();
    kb
}

/// Run the housekeeping loop for `ms` milliseconds (1 pass per ms).
fn run(kb: &mut Kb, m: &mut Mock, ms: u32) {
    for _ in 0..ms {
        kb.housekeeping_task_kb(m, false);
        m.t += 1;
    }
}

fn tap(kb: &mut Kb, m: &mut Mock, row: u8, col: u8) {
    kb.matrix_event(m, row, col, true);
    kb.matrix_event(m, row, col, false);
}

fn nkro_has(bits: &[u8; 30], key: u8) -> bool {
    bits[key as usize / 8] & (1 << (key % 8)) != 0
}

// Matrix positions (keyboard.json LAYOUT)
const ESC: (u8, u8) = (0, 0);
const KEY_A: (u8, u8) = (3, 1);
const KEY_LSFT: (u8, u8) = (4, 0);
const KEY_FN: (u8, u8) = (5, 10);
const KEY_M: (u8, u8) = (4, 8);
const KEY_1: (u8, u8) = (1, 1);
const KEY_4: (u8, u8) = (1, 4);
const KEY_F13_POS: (u8, u8) = (0, 14); // Fn+PrtSc = MAC_PRTA on both Fn layers
const KEY_NUM: (u8, u8) = (1, 17);

// ═════════════════════════════════════════════════════════════════════════
// rf.c — UART framing
// ═════════════════════════════════════════════════════════════════════════

/// The NRF checksums its frames with a plain byte sum. The previous Rust
/// parser used the outgoing (XOR 0x5A) checksum and dropped every status
/// frame, so link state, battery and lock LEDs never updated.
#[test]
fn rx_status_frame_uses_plain_sum_checksum() {
    let mut m = Mock::new();
    m.nrf_auto = false;
    let mut kb = Kb::new();
    kb.dev_info.link_mode = LINK_BT_1;

    m.rx.extend(nrf_frame(CMD_RF_STS_SYSC, &[LINK_BT_1, RF_CONNECT, 0x02, 0x01, 57]));
    kb.uart_receive_pro(&mut m);
    assert_eq!(kb.dev_info.rf_state, RF_CONNECT);
    assert_eq!((kb.dev_info.rf_led, kb.dev_info.rf_charge, kb.dev_info.rf_baterry), (0x02, 0x01, 57));

    let mut bad = nrf_frame(CMD_RF_STS_SYSC, &[LINK_BT_1, RF_DISCONNECT, 0, 0, 10]);
    *bad.last_mut().unwrap() ^= UART_HEAD;
    m.rx.extend(bad);
    kb.uart_receive_pro(&mut m);
    assert_eq!(kb.dev_info.rf_state, RF_CONNECT, "corrupt frame must be ignored");
}

/// Outgoing command frames are byte-identical to C's uart_send_cmd().
#[test]
fn command_frames_match_c() {
    let mut m = Mock::new();
    m.nrf_auto = false;
    let mut kb = Kb::new();
    kb.dev_info.link_mode = LINK_BT_2;
    for cmd in [CMD_HAND, CMD_SET_LINK, CMD_NEW_ADV, CMD_RF_STS_SYSC, CMD_READ_DATA, CMD_SET_CONFIG, CMD_SET_NAME, CMD_SET_24G_NAME] {
        kb.uart_send_cmd(&mut m, cmd, 0, 0);
    }
    let f: Vec<Vec<u8>> = m.tx.iter().map(|x| x.1.clone()).collect();
    assert_eq!(f[0], [0x5A, CMD_HAND, 0, 1, 0, 0x5A]);
    assert_eq!(f[1], [0x5A, CMD_SET_LINK, 0, 1, 2, 2 ^ 0x5A]);
    assert_eq!(f[2], [0x5A, CMD_NEW_ADV, 0, 2, 2, 1, 3 ^ 0x5A]);
    assert_eq!(f[3], [0x5A, CMD_RF_STS_SYSC, 0, 1, 2, 2 ^ 0x5A]);
    assert_eq!(f[4], [0x5A, CMD_READ_DATA, 0, 2, 0, 32, 32 ^ 0x5A]);
    assert_eq!(f[5], [0x5A, CMD_SET_CONFIG, 0, 1, 24, 24 ^ 0x5A]);
    let mut name = vec![0x5A, CMD_SET_NAME, 0, 11, 1, 9];
    name.extend_from_slice(b"Air96 V2\0");
    name.push(get_checksum(&name[4..]));
    assert_eq!(f[6], name);
    let dongle: Vec<u8> = "NuPhy Air96 V2 Dongle".bytes().flat_map(|c| [c, 0]).collect();
    assert_eq!(f[7][3], 44);
    assert_eq!(&f[7][4..6], &[44, 3]);
    assert_eq!(&f[7][6..48], &dongle[..]);
    assert_eq!(f[7][48], get_checksum(&f[7][4..48]));
}

/// A short ACK only satisfies the command that is waiting for it, and a
/// reply already queued before the send is drained, not counted.
#[test]
fn ack_matches_pending_command() {
    let mut m = Mock::new();
    m.nrf_auto = false;
    let mut kb = Kb::new();
    m.next_reply = Some(vec![UART_HEAD, CMD_HAND, 0xA0]);
    assert_eq!(kb.uart_send_cmd(&mut m, CMD_SLEEP, 5, 0), TX_TIMEOUT, "ACK for another command");
    m.rx.extend([UART_HEAD, CMD_SLEEP, 0xA0]);
    assert_eq!(kb.uart_send_cmd(&mut m, CMD_SLEEP, 5, 0), TX_TIMEOUT, "stale ACK from before the send");
    m.next_reply = Some(vec![UART_HEAD, CMD_SLEEP, 0xA0]);
    assert_eq!(kb.uart_send_cmd(&mut m, CMD_SLEEP, 5, 0), TX_OK);
}

/// CMD_24G_SUSPEND from the dongle only puts the 2.4G link to sleep.
#[test]
fn dongle_suspend_only_in_24g_mode() {
    let mut m = Mock::new();
    m.nrf_auto = false;
    let mut kb = Kb::new();
    kb.dev_info.link_mode = LINK_BT_1;
    m.rx.extend([UART_HEAD, CMD_24G_SUSPEND, 0xA0]);
    kb.uart_receive_pro(&mut m);
    assert!(!kb.f_goto_sleep);
    kb.dev_info.link_mode = LINK_RF_24;
    m.rx.extend([UART_HEAD, CMD_24G_SUSPEND, 0xA0]);
    kb.uart_receive_pro(&mut m);
    assert!(kb.f_goto_sleep);
}

/// A partial frame is dropped after 10ms of silence so noise can't wedge the parser.
#[test]
fn partial_frame_times_out() {
    let mut m = Mock::new();
    m.nrf_auto = false;
    let mut kb = Kb::new();
    kb.dev_info.link_mode = LINK_BT_1;
    m.rx.extend([UART_HEAD, CMD_RF_STS_SYSC, 0x00, 5, LINK_BT_1]);
    kb.uart_receive_pro(&mut m);
    m.t += 11;
    m.rx.extend(nrf_frame(CMD_RF_STS_SYSC, &[LINK_BT_1, RF_CONNECT, 0, 0, 90]));
    kb.uart_receive_pro(&mut m);
    assert_eq!(kb.dev_info.rf_baterry, 90);
}

// ═════════════════════════════════════════════════════════════════════════
// ansi.c — power-on
// ═════════════════════════════════════════════════════════════════════════

/// The radio's saved link must be read before the dial is scanned, so a
/// wireless power-on returns to the last channel instead of 2.4G.
#[test]
fn power_on_restores_saved_radio_channel() {
    let mut m = Mock::new();
    m.dev_pin = false; // wireless
    m.nrf_saved = [LINK_BT_2, LINK_BT_2, LINK_BT_2];
    let kb = boot(&mut m);
    assert_eq!(kb.dev_info.link_mode, LINK_BT_2);
    assert_eq!(kb.dev_info.rf_channel, LINK_BT_2);
    assert!(!m.sent(CMD_READ_DATA).is_empty() && !m.sent(CMD_SET_24G_NAME).is_empty());
}

/// Win position: default layer 2 and NKRO; Mac: layer 0 and still NKRO (v3.2.7).
#[test]
fn os_dial_selects_layer_and_keeps_nkro() {
    for (mac, layer) in [(false, 2u32), (true, 0)] {
        let mut m = Mock::new();
        m.sys_pin = mac;
        let kb = boot(&mut m);
        assert_eq!(kb.default_layer_state, 1 << layer);
        assert!(kb.nkro);
        assert_eq!(kb.dev_info.sys_sw_state, if mac { SYS_SW_MAC } else { SYS_SW_WIN });
    }
}

/// First boot (no saved config) applies the C defaults: HSV 255/255/151,
/// CYCLE_LEFT_RIGHT, QMK default speed, side bar wave/brightness 3, sleep on.
#[test]
fn first_boot_defaults() {
    let mut m = Mock::new();
    let kb = boot(&mut m);
    assert_eq!((kb.rgb.hue, kb.rgb.sat, kb.rgb.val), (255, 255, 151));
    assert_eq!((kb.rgb.mode, kb.rgb.speed), (RGB_MATRIX_DEFAULT_MODE, RGB_MATRIX_DEFAULT_SPD));
    assert_eq!((kb.side.side_mode, kb.side.side_light, kb.side.side_speed, kb.side.side_rgb), (0, 3, 2, 1));
    assert!(kb.user_config.sleep_enable);
    assert_eq!(m.cfgs.len(), 1, "first boot persists the defaults once");
}

/// Out-of-range saved side settings are clamped (no table overrun).
#[test]
fn saved_side_config_is_clamped() {
    let mut m = Mock::new();
    let mut kb = Kb::new();
    let cfg = eeprom::UserConfig { side_mode: 9, side_brightness: 9, side_speed: 9, side_colour: 9, ..Default::default() };
    kb.keyboard_post_init_kb(&mut m, Some(cfg));
    assert_eq!((kb.side.side_mode, kb.side.side_light, kb.side.side_speed, kb.side.side_colour), (0, 3, 2, 0));
}

// ═════════════════════════════════════════════════════════════════════════
// QMK actions / layers / shortcuts
// ═════════════════════════════════════════════════════════════════════════

/// MO() layers are a bitmask: releasing either of two held MO(4) keys turns
/// layer 4 off (QMK layer_off), and the highest layer wins.
#[test]
fn mo_layers_are_bitmask() {
    let mut m = Mock::new();
    m.sys_pin = true; // Mac: Fn = MO(1), LSFT/M on layer 1 = MO(4)
    let mut kb = boot(&mut m);
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
    kb.matrix_event(&mut m, KEY_LSFT.0, KEY_LSFT.1, true);
    kb.matrix_event(&mut m, KEY_M.0, KEY_M.1, true);
    assert_eq!(kb.layer_state, (1 << 1) | (1 << 4));
    kb.matrix_event(&mut m, KEY_M.0, KEY_M.1, false);
    assert_eq!(kb.layer_state, 1 << 1);
}

/// A key releases with the keycode it pressed with, even if Fn was released first.
#[test]
fn release_uses_press_time_keycode() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
    kb.matrix_event(&mut m, 0, 2, true); // Win Fn+F1 = BRID (consumer)
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, false);
    kb.matrix_event(&mut m, 0, 2, false);
    assert_eq!(m.usb_consumer, vec![0x0070, 0]);
    assert!(m.usb_nkro.iter().all(|(_, b)| !nkro_has(b, 0x3A)), "F1 never pressed");
}

/// More than six keys are reported (NKRO), matching FORCE_NKRO.
#[test]
fn nkro_reports_more_than_six_keys() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    for col in 1..=8 {
        kb.matrix_event(&mut m, 2, col, true); // Q W E R T Y U I
    }
    let (_, bits) = *m.usb_nkro.last().unwrap();
    assert_eq!(bits.iter().map(|b| b.count_ones()).sum::<u32>(), 8);
}

/// tap_shortcut() borrows modifiers without releasing a held one, holds the
/// key 5ms, and leaves the physically held modifier down afterwards.
#[test]
fn shortcut_keeps_held_modifier() {
    let mut m = Mock::new();
    m.sys_pin = true; // Mac base row 0 col 5 = MAC_SEARCH (GUI+Space)
    let mut kb = boot(&mut m);
    kb.matrix_event(&mut m, KEY_LSFT.0, KEY_LSFT.1, true);
    let n = m.usb_nkro.len();
    let t0 = m.t;
    kb.matrix_event(&mut m, 0, 5, true);
    let sent = &m.usb_nkro[n..];
    assert_eq!(sent[0].0, 0x02 | 0x08);
    assert!(nkro_has(&sent[0].1, 0x2C));
    assert!(m.t - t0 >= 5, "space held at least 5ms");
    let (mods, bits) = *m.usb_nkro.last().unwrap();
    assert_eq!(mods, 0x02, "Shift stays down, GUI released");
    assert!(!nkro_has(&bits, 0x2C));
}

/// MAC_PRTA is Cmd+Shift+4 in Mac mode (base layer) and Win+Shift+S in
/// Windows mode (Fn layer).
#[test]
fn screenshot_key_follows_os_mode() {
    for (mac, key) in [(true, 0x21u8), (false, 0x16)] {
        let mut m = Mock::new();
        m.sys_pin = mac;
        let mut kb = boot(&mut m);
        if !mac {
            kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
        }
        let n = m.usb_nkro.len();
        tap(&mut kb, &mut m, KEY_F13_POS.0, KEY_F13_POS.1);
        let (mods, bits) = m.usb_nkro[n];
        assert_eq!(mods, 0x0A);
        assert!(nkro_has(&bits, key), "mac={mac}");
    }
}

/// host_consumer_send() only sends changes.
#[test]
fn consumer_reports_are_deduplicated() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    m.usb_consumer.clear();
    kb.host_consumer_send(&mut m, 0xE9);
    kb.host_consumer_send(&mut m, 0xE9);
    kb.host_consumer_send(&mut m, 0);
    assert_eq!(m.usb_consumer, vec![0xE9, 0]);
}

/// Holding Shift reverses RGB steps (QMK process_rgb).
#[test]
fn shift_reverses_rgb_keys() {
    let mut m = Mock::new();
    m.sys_pin = true; // Mac Fn+DOWN = RGB_VAD
    let mut kb = boot(&mut m);
    let v = kb.rgb.val;
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
    tap(&mut kb, &mut m, 5, 15);
    assert_eq!(kb.rgb.val, v - RGB_MATRIX_VAL_STEP);
    kb.matrix_event(&mut m, 4, 13, true); // RShift → MO(4) on layer 1, use real shift instead
    kb.matrix_event(&mut m, 4, 13, false);
    kb.real_mods = 0x02;
    tap(&mut kb, &mut m, 5, 15);
    assert_eq!(kb.rgb.val, v);
}

// ═════════════════════════════════════════════════════════════════════════
// rf.c — wireless reports
// ═════════════════════════════════════════════════════════════════════════

fn wireless(m: &mut Mock) -> Kb {
    m.dev_pin = false;
    m.nrf_saved = [LINK_BT_1, LINK_BT_1, LINK_BT_1];
    let mut kb = boot(m);
    run(&mut kb, m, 400); // first dev_sts_sync installs the RF driver
    assert_eq!(kb.driver, HostDriver::Rf);
    assert_eq!(kb.dev_info.rf_state, RF_CONNECT);
    m.tx.clear();
    kb
}

/// Usages 1..111 live in the bitmap report, 112+ in the byte report slots;
/// adding a key never moves a held one between the dongle's interfaces.
#[test]
fn wireless_keys_have_fixed_report_ownership() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    kb.register_code(&mut m, 0x04);
    let bit = m.sent(CMD_RPT_BIT_KB);
    assert_eq!(bit.len(), 1);
    assert_eq!(bit[0][4 + 1] & 1 << 4, 1 << 4, "A in bitmap");
    kb.register_code(&mut m, 0x73); // F24 (usage 115)
    let byte = m.sent(CMD_RPT_BYTE_KB);
    assert_eq!(&byte.last().unwrap()[4..12], &[0, 0, 0x73, 0, 0, 0, 0, 0]);
    assert_eq!(m.sent(CMD_RPT_BIT_KB).len(), 1, "A did not move");
}

/// Reports captured while the link is down are replayed in order once the
/// selected channel reports connected (8ms apart on Bluetooth).
#[test]
fn reports_queue_while_linking_and_replay_in_order() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    m.nrf_state = RF_LINKING;
    kb.dev_info.rf_state = RF_LINKING;
    tap(&mut kb, &mut m, KEY_A.0, KEY_A.1);
    tap(&mut kb, &mut m, KEY_1.0, KEY_1.1);
    assert!(m.reports().is_empty(), "nothing sent while linking");
    m.nrf_state = RF_CONNECT;
    run(&mut kb, &mut m, 300);
    // The replayed frames come first; periodic resends of the idle state follow.
    let bit: Vec<(u32, Vec<u8>)> = m.tx.iter().filter(|(_, f)| f[1] == CMD_RPT_BIT_KB).cloned().collect();
    let keys: Vec<u8> = bit[..4].iter().map(|(_, f)| f[5] | f[8]).collect();
    assert_eq!(keys, vec![0x10, 0, 0x40, 0], "A down, A up, 1 down, 1 up");
    assert!(bit[..4].windows(2).all(|w| w[1].0 - w[0].0 >= 8), "8ms Bluetooth pacing");
}

/// Old taps are not replayed after a long outage; current key state is.
#[test]
fn stale_queue_expires_after_one_second() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    m.nrf_state = RF_LINKING;
    kb.dev_info.rf_state = RF_LINKING;
    tap(&mut kb, &mut m, KEY_A.0, KEY_A.1);
    m.t += 2000;
    m.nrf_state = RF_CONNECT;
    run(&mut kb, &mut m, 300);
    let r = m.reports();
    let kinds: Vec<u8> = r[..5].iter().map(|f| f[1]).collect();
    assert_eq!(kinds, vec![CMD_RPT_MS, CMD_RPT_BYTE_KB, CMD_RPT_BIT_KB, CMD_RPT_CONSUME, CMD_RPT_SYS], "one current-state report per endpoint");
    assert!(r.iter().all(|f| f[4..f.len() - 1].iter().all(|&b| b == 0)), "the old tap is not replayed");
}

/// While typing (<2s idle) the current reports are re-sent every ~100ms.
#[test]
fn periodic_resend_while_active() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    kb.matrix_event(&mut m, KEY_A.0, KEY_A.1, true);
    m.tx.clear();
    run(&mut kb, &mut m, 1000);
    let n = m.sent(CMD_RPT_BYTE_KB).len();
    assert!((8..=10).contains(&n), "{n}");
    assert_eq!(m.sent(CMD_RPT_BIT_KB).len(), n, "bitmap re-sent alongside");
    // A held key is activity (Sleep_Handle), so resends continue; after the
    // release they stop once the keyboard has been idle for 2s.
    // (The NumLock auto-on press also counts as activity, so allow for it.)
    kb.matrix_event(&mut m, KEY_A.0, KEY_A.1, false);
    run(&mut kb, &mut m, 3000);
    m.tx.clear();
    run(&mut kb, &mut m, 1000);
    assert!(m.sent(CMD_RPT_BYTE_KB).is_empty(), "stops after 2s idle");
}

/// Ten unanswered status polls reset the NRF without blocking the scan:
/// 100ms, reset low 50ms, high 50ms, then relink.
#[test]
fn sync_loss_resets_nrf_nonblocking() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    m.nrf_auto = false;
    m.reset_log.clear();
    run(&mut kb, &mut m, 2400);
    let lows: Vec<u32> = m.reset_log.iter().filter(|e| !e.1).map(|e| e.0).collect();
    assert_eq!(lows.len(), 1);
    let high = m.reset_log.iter().find(|e| e.1 && e.0 > lows[0]).unwrap().0;
    assert_eq!(high - lows[0], 50);
    assert!(m.sent(CMD_SET_LINK).iter().count() >= 1, "relinks after reset");
}

// ═════════════════════════════════════════════════════════════════════════
// sleep.c
// ═════════════════════════════════════════════════════════════════════════

/// Connected and idle for 6 minutes: SET_CONFIG to the NRF and LED power off.
/// A held key counts as activity.
#[test]
fn wireless_idle_sleep_and_held_key() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    kb.matrix_event(&mut m, KEY_A.0, KEY_A.1, true);
    run(&mut kb, &mut m, 370_000);
    assert!(!kb.f_wakeup_prepare, "held key keeps it awake");
    kb.matrix_event(&mut m, KEY_A.0, KEY_A.1, false);
    run(&mut kb, &mut m, 361_000);
    assert!(kb.f_wakeup_prepare);
    assert!(!m.led_on);
    assert_eq!(m.sent(CMD_SET_CONFIG).len(), 1);
    // Key wake: rf_wake() handshake + status, LEDs back on.
    m.tx.clear();
    kb.matrix_event(&mut m, KEY_A.0, KEY_A.1, true);
    run(&mut kb, &mut m, 100);
    assert!(m.led_on && !kb.f_wakeup_prepare);
    assert!(!m.sent(CMD_HAND).is_empty());
}

/// USB suspended for 1s → sleep; host resume wakes and clears key state.
#[test]
fn usb_suspend_sleep_and_resume() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    run(&mut kb, &mut m, 100);
    m.usb_suspended = true;
    run(&mut kb, &mut m, 900);
    assert!(!kb.f_wakeup_prepare, "debounced for 1s");
    run(&mut kb, &mut m, 200);
    assert!(kb.f_wakeup_prepare && !m.led_on);
    m.usb_suspended = false;
    run(&mut kb, &mut m, 100);
    assert!(!kb.f_wakeup_prepare && m.led_on);
}

/// A stray sleep request while the USB host is active is rejected.
#[test]
fn usb_active_rejects_sleep_request() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    kb.f_goto_sleep = true;
    run(&mut kb, &mut m, 100);
    assert!(!kb.f_wakeup_prepare && m.led_on);
}

// ═════════════════════════════════════════════════════════════════════════
// ansi.c — dial, link keys, device reset
// ═════════════════════════════════════════════════════════════════════════

/// The dial must read the same for 25 × 20ms before the link switches.
#[test]
fn dial_switch_debounced() {
    let mut m = Mock::new();
    m.nrf_saved = [LINK_BT_1, LINK_BT_1, LINK_BT_1];
    let mut kb = boot(&mut m);
    assert_eq!(kb.dev_info.link_mode, LINK_USB);
    m.dev_pin = false;
    run(&mut kb, &mut m, 480);
    assert_eq!(kb.dev_info.link_mode, LINK_USB);
    run(&mut kb, &mut m, 60);
    assert_eq!(kb.dev_info.link_mode, LINK_BT_1);
}

/// Short link-key tap switches channel at once; holding 3s starts pairing
/// and retries NEW_ADV twice while the radio hasn't acknowledged.
#[test]
fn link_key_tap_switches_hold_pairs() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
    tap(&mut kb, &mut m, 1, 3); // Fn+3 = BLE3
    assert_eq!(kb.dev_info.link_mode, LINK_BT_3);
    assert_eq!(m.sent(CMD_SET_LINK).last().unwrap()[4], LINK_BT_3);

    m.nrf_auto = false;
    m.tx.clear();
    kb.matrix_event(&mut m, 1, 2, true); // Fn+2 = BLE2, hold
    run(&mut kb, &mut m, 3200);
    assert_eq!(kb.dev_info.link_mode, LINK_BT_2);
    assert_eq!(m.sent(CMD_NEW_ADV).len(), 3, "NEW_ADV + 2 retries");
    kb.matrix_event(&mut m, 1, 2, false);
    assert_eq!(kb.dev_info.link_mode, LINK_BT_2);
}

/// Link keys do nothing while wired.
#[test]
fn link_keys_ignored_on_usb() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
    tap(&mut kb, &mut m, KEY_4.0, KEY_4.1); // Fn+4 = LNK_RF
    assert_eq!(kb.dev_info.link_mode, LINK_USB);
}

/// Holding Fn+[ for 3s: SET_LINK, CLR_DEVICE, eeconfig_init, three blinks,
/// side/RGB defaults (speed 255 − 2×52).
#[test]
fn device_reset() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    kb.rgb.speed = 10;
    kb.matrix_event(&mut m, KEY_FN.0, KEY_FN.1, true);
    kb.matrix_event(&mut m, 2, 11, true);
    run(&mut kb, &mut m, 3100);
    assert_eq!(m.ee_inits, 1);
    assert_eq!(m.flushes, 6);
    assert!(!m.sent(CMD_CLR_DEVICE).is_empty());
    assert_eq!(kb.rgb.speed, 151);
    assert_eq!((kb.rgb.hue, kb.rgb.sat, kb.rgb.val), (255, 255, 151));
}

// ═════════════════════════════════════════════════════════════════════════
// NumLock (v3.2.9)
// ═════════════════════════════════════════════════════════════════════════

fn numlock_presses(m: &Mock) -> usize {
    m.usb_nkro.windows(2).filter(|w| !nkro_has(&w[0].1, KC_NUM_LOCK) && nkro_has(&w[1].1, KC_NUM_LOCK)).count()
}

/// Power-on wired: NumLock turned on once after the host settles; a user's
/// NumLock-off is respected until the next re-enumeration (PC restart).
#[test]
fn numlock_on_at_power_on_and_pc_restart() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    run(&mut kb, &mut m, 1900);
    assert_eq!(numlock_presses(&m), 0);
    run(&mut kb, &mut m, 300);
    assert_eq!(numlock_presses(&m), 1);
    assert_eq!(m.usb_leds & 1, 1);
    m.usb_leds = 0;
    run(&mut kb, &mut m, 5000);
    assert_eq!(numlock_presses(&m), 1, "user's choice respected");
    kb.notify_usb_device_state_change_kb(UsbDeviceState::Init);
    kb.notify_usb_device_state_change_kb(UsbDeviceState::Configured);
    run(&mut kb, &mut m, 2200);
    assert_eq!(numlock_presses(&m), 2);
    assert_eq!(m.usb_leds & 1, 1);
}

/// macOS has no NumLock (the key is Clear): never pressed in Mac mode.
#[test]
fn numlock_skipped_in_mac_mode() {
    let mut m = Mock::new();
    m.sys_pin = true;
    let mut kb = boot(&mut m);
    run(&mut kb, &mut m, 5000);
    assert_eq!(numlock_presses(&m), 0);
}

/// Wireless reconnect re-applies NumLock on via the radio's LED report.
#[test]
fn numlock_on_wireless_reconnect() {
    let mut m = Mock::new();
    let mut kb = wireless(&mut m);
    m.nrf_led = 0;
    run(&mut kb, &mut m, 2500);
    // NumLock presses = rising edges of its bit in the bitmap frames.
    let num_frames = |m: &Mock| {
        let down: Vec<bool> = m.sent(CMD_RPT_BIT_KB).iter().map(|f| f[4 + 1 + 0x53 / 8] & (1 << (0x53 % 8)) != 0).collect();
        down.windows(2).filter(|w| !w[0] && w[1]).count() + down.first().map_or(0, |&d| d as usize)
    };
    assert_eq!(num_frames(&m), 1);
    m.nrf_led = 1;
    m.nrf_state = RF_DISCONNECT;
    run(&mut kb, &mut m, 600);
    m.nrf_led = 0;
    m.nrf_state = RF_CONNECT;
    run(&mut kb, &mut m, 2600);
    assert_eq!(num_frames(&m), 2);
}

// ═════════════════════════════════════════════════════════════════════════
// side.c
// ═════════════════════════════════════════════════════════════════════════

/// Caps Lock lights the left bar from the active host only (USB LEDs when
/// wired), not from a stale radio LED byte.
#[test]
fn caps_indicator_follows_active_host() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    run(&mut kb, &mut m, 4000); // past the link-show window
    kb.dev_info.rf_led = 0x02;
    run(&mut kb, &mut m, 20);
    let (b1, b2) = kb.rgb.build_pwm_buffers();
    let left = |b1: &[u8; 192], b2: &[u8; 192]| {
        let l = &rgb::LED_MAP[100];
        let b = if l.driver == 0 { b1 } else { b2 };
        [b[l.r as usize], b[l.g as usize], b[l.b as usize]]
    };
    assert_ne!(left(&b1, &b2), [0, 128, 128]);
    m.usb_leds = 0x02;
    run(&mut kb, &mut m, 20);
    let (b1, b2) = kb.rgb.build_pwm_buffers();
    assert_eq!(left(&b1, &b2), [0, 128, 128]);
}

/// Side speed/brightness controls clamp and persist, like light_*_control().
#[test]
fn side_controls_clamp_and_persist() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    let n = m.cfgs.len();
    for _ in 0..10 {
        kb.light_level_control(&mut m, true);
    }
    assert_eq!(kb.side.side_light, 5);
    assert_eq!(m.cfgs.len(), n + 2, "no write once clamped");
    kb.side.side_speed = 9;
    kb.light_speed_control(&mut m, true);
    assert_eq!(kb.side.side_speed, 1, "out-of-range resets to middle first");
}

/// Side LEDs (flags 0) are never painted by RGB effects.
#[test]
fn rgb_effects_leave_side_leds() {
    let mut r = rgb::RgbMatrix::new();
    r.set_mode(12);
    r.set_color(100, 1, 2, 3);
    for t in 0..200 {
        r.rgb_matrix_task(t, |_| true);
    }
    assert_eq!(r.color(100), [1, 2, 3]);
    assert_ne!(r.color(0), [0, 0, 0]);
}

// ═════════════════════════════════════════════════════════════════════════
// Debounce (asym_eager_defer_pk, DEBOUNCE=10)
// ═════════════════════════════════════════════════════════════════════════

/// Press reports on the first scan; contact chatter after it is absorbed;
/// release reports only after 10ms stable; quick distinct taps stay distinct.
#[test]
fn debounce_eager_press_deferred_release() {
    let mut mx = Matrix::new();
    let key = |down: bool| {
        let mut r = [0u32; MATRIX_ROWS];
        if down {
            r[0] = 1;
        }
        r
    };
    let mut events = vec![];
    // t=0 press, chatter 1..5, held to 40, chatter release 40..45, then idle.
    for t in 0..120u16 {
        let down = match t {
            0..=1 => true,
            2 => false,
            3..=39 => true,
            40 | 42 | 44 => false,
            41 | 43 => true,
            45..=59 => false,
            60..=79 => true,
            _ => false,
        };
        for e in mx.process(key(down), t) {
            events.push((t, e.pressed));
        }
    }
    assert_eq!(events[0], (0, true), "zero-latency press");
    assert_eq!(events.len(), 4, "{events:?}");
    assert!(events[1].0 >= 54 && !events[1].1, "release after 10ms stable");
    assert_eq!(events[2], (60, true));
}

#[test]
fn debouncer_ignores_unchanged_matrix() {
    let mut d = Debouncer::new();
    let raw = [0u32; MATRIX_ROWS];
    let mut cooked = [0u32; MATRIX_ROWS];
    assert!(!d.debounce(&raw, &mut cooked, false, 5));
}

// ═════════════════════════════════════════════════════════════════════════
// Keymap (layers verified against keymap.c by script)
// ═════════════════════════════════════════════════════════════════════════

#[allow(clippy::identity_op)]
#[test]
fn mo_layer() {
    assert_eq!(keymap::mo_layer(keymap::QK_MOMENTARY | 0), Some(0));
    assert_eq!(keymap::mo_layer(keymap::QK_MOMENTARY | 4), Some(4));
    assert_eq!(keymap::mo_layer(0x0000), None);
}
#[test]
fn win_layer_keys() {
    assert_eq!(keymap::get_keycode(2, 0, 2), 0x003A);
    assert_eq!(keymap::get_keycode(2, 0, 14), 0x0046);
    assert_eq!(keymap::get_keycode(2, KEY_NUM.0 as usize, KEY_NUM.1 as usize), 0x53);
    assert_eq!(keymap::get_keycode(0, ESC.0 as usize, ESC.1 as usize), 0x29);
}
#[test]
fn consumer_usage_mapping() {
    assert_eq!(keymap::consumer_usage(keymap::KC_BRID), 0x0070);
    assert_eq!(keymap::consumer_usage(keymap::KC_BRIU), 0x006F);
    assert_eq!(keymap::consumer_usage(keymap::KC_MPRV), 0x00B6);
    assert_eq!(keymap::consumer_usage(keymap::KC_MPLY), 0x00CD);
    assert_eq!(keymap::consumer_usage(keymap::KC_MNXT), 0x00B5);
    assert_eq!(keymap::consumer_usage(keymap::KC_MUTE), 0x00E2);
    assert_eq!(keymap::consumer_usage(keymap::KC_VOLD), 0x00EA);
    assert_eq!(keymap::consumer_usage(keymap::KC_VOLU), 0x00E9);
}

// ═════════════════════════════════════════════════════════════════════════
// RGB driver / EEPROM / USB descriptors
// ═════════════════════════════════════════════════════════════════════════

#[test]
fn led_map_110() {
    assert_eq!(rgb::LED_MAP.len(), 110);
    for i in 100..110 {
        assert_eq!(rgb::LED_MAP[i].driver, 1);
    }
}
#[test]
fn no_all_zero_entries() {
    for (i, l) in rgb::LED_MAP.iter().enumerate() {
        assert!(!(l.r == 0 && l.g == 0 && l.b == 0), "LED {} all zero", i);
    }
}
#[test]
fn eeprom_defaults() {
    let c = eeprom::UserConfig::default();
    assert_eq!((c.side_mode, c.side_brightness, c.side_speed), (0, 3, 2));
    assert_eq!((c.rgb_mode, c.rgb_hue, c.rgb_sat, c.rgb_val, c.rgb_speed), (12, 255, 255, 151, 127));
}

/// Dump rendered frames in the same format as the QMK harness
/// (tools/rgb_harness): `RGB_DUMP=path cargo test dump_rgb_frames -- --ignored`.
/// Each frame is driven through `rgb_matrix_task()` until its flush, with key
/// presses at fixed times for the reactive effects, exactly as the harness does.
#[test]
#[ignore]
fn dump_rgb_frames() {
    use std::io::Write;
    let path = std::env::var("RGB_DUMP").expect("RGB_DUMP");
    let mut out = std::fs::File::create(path).unwrap();
    let speeds = [0u8, 127, 255];
    let hsvs = [[255u8, 255, 151], [0, 255, 255], [170, 128, 200], [20, 0, 255]];
    let times = [0u32, 16, 37, 500, 1234, 4000, 30000, 65520, 70001, 1000003];
    let keys = [(3u8, 1u8), (1, 5), (4, 9), (2, 18), (5, 6)];
    let mut r = rgb::RgbMatrix::new();
    r.sethsv_noeeprom(0, 255, 255);
    for mode in 1..crate::led::rgb_matrix::RGB_MATRIX_EFFECT_MAX {
        for &sp in &speeds {
            for (hi, hsv) in hsvs.iter().enumerate() {
                r.speed = sp;
                r.sethsv_noeeprom(hsv[0], hsv[1], hsv[2]);
                r.set_mode(mode);
                for (ti, &t) in times.iter().enumerate() {
                    let (row, col) = keys[ti % keys.len()];
                    r.handle_key_event(row, col, true);
                    let mut flushed = false;
                    let mut frame = [[0u8; 3]; 100];
                    for _ in 0..64 {
                        r.rgb_matrix_task(t, |m| {
                            for i in 0..100 {
                                frame[i] = m.color(i);
                            }
                            flushed = true;
                            true
                        });
                        if flushed {
                            break;
                        }
                    }
                    write!(out, "{mode} {sp} {hi} {t}").unwrap();
                    for c in frame.iter() {
                        write!(out, " {:02x}{:02x}{:02x}", c[0], c[1], c[2]).unwrap();
                    }
                    writeln!(out).unwrap();
                }
            }
        }
    }
}

/// The radio remembers the last link, so after wired use it reports USB while
/// the dial now selects wireless. Power-on must still end up on the RF
/// driver, or every key goes to the USB endpoint and nothing types.
#[test]
fn wireless_power_on_after_wired_use_types_over_radio() {
    let mut m = Mock::new();
    m.dev_pin = false;
    m.nrf_saved = [LINK_USB, LINK_BT_1, LINK_BT_1];
    let mut kb = boot(&mut m);
    run(&mut kb, &mut m, 400);
    assert_eq!(kb.dev_info.link_mode, LINK_BT_1);
    assert_eq!(kb.driver, HostDriver::Rf);
    m.tx.clear();
    let usb_before = m.usb_nkro.len() + m.usb_kb.len();
    tap(&mut kb, &mut m, KEY_A.0, KEY_A.1);
    assert_eq!(m.usb_nkro.len() + m.usb_kb.len(), usb_before, "key went to USB");
    assert!(!m.reports().is_empty(), "key not sent over the radio");
}

/// A host that selected boot protocol (BIOS) only accepts 6KRO boot reports.
/// Wireless forces report protocol; returning to USB must restore the host's
/// choice, and the charging host's SET_PROTOCOL must not affect wireless.
#[test]
fn usb_protocol_survives_wireless_round_trip() {
    let mut m = Mock::new();
    let mut kb = boot(&mut m);
    kb.usb_set_protocol(0);
    assert_eq!(kb.keyboard_protocol, 0);

    m.dev_pin = false;
    m.nrf_saved = [LINK_BT_1, LINK_BT_1, LINK_BT_1];
    run(&mut kb, &mut m, 1000);
    assert_eq!(kb.driver, HostDriver::Rf);
    assert_eq!(kb.keyboard_protocol, 1);
    kb.usb_set_protocol(0);
    assert_eq!(kb.keyboard_protocol, 1, "USB host changed the wireless protocol");

    m.dev_pin = true;
    run(&mut kb, &mut m, 1000);
    assert_eq!(kb.driver, HostDriver::Usb);
    assert_eq!(kb.keyboard_protocol, 0);
    let (nkro_before, kb_before) = (m.usb_nkro.len(), m.usb_kb.len());
    tap(&mut kb, &mut m, KEY_A.0, KEY_A.1);
    assert_eq!(m.usb_nkro.len(), nkro_before, "NKRO report sent to a boot-protocol host");
    assert!(m.usb_kb.len() > kb_before);
}

/// Rainbow wave walks a 224-entry table and single-colour wave a 112-entry
/// one with the same index; switching from rainbow at any point must not
/// index past the shorter table (a panic hangs the firmware).
#[test]
fn side_wave_colour_switch_from_any_rainbow_position() {
    for start in 0..crate::led::side_tables::FLOW_COLOUR_TAB_LEN as u8 {
        let mut m = Mock::new();
        let mut kb = boot(&mut m);
        kb.side.side_mode = crate::led::side::SIDE_WAVE;
        kb.side.side_rgb = 1;
        kb.side.side_play_point = start;
        kb.side_colour_control(&mut m, true);
        assert_eq!(kb.side.side_rgb, 0);
        for _ in 0..200 {
            m.t += 5;
            kb.m_side_led_show(&mut m);
        }
    }
}
