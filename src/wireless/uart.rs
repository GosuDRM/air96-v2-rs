//! NRF52 radio link — 1:1 port of `keyboards/air96_v2/ansi/rf.c` (C v3.2.9).
//!
//! 460800 baud, 8E1. Frame: HEAD(0x5A) CMD ACK LEN DATA[LEN] CHECKSUM.
//! Outgoing checksum = sum(DATA) ^ HEAD; incoming checksum = sum(DATA).

use crate::hal::Hal;
use crate::kb::*;

pub const UART_HEAD: u8 = 0x5A;
pub const UART_MAX_LEN: usize = 64;
pub const FUNC_VALID_LEN: usize = 32;

pub const CMD_POWER_UP: u8 = 0xF0;
pub const CMD_SLEEP: u8 = 0xF1;
pub const CMD_HAND: u8 = 0xF2;
pub const CMD_SNIF: u8 = 0xF3;
pub const CMD_24G_SUSPEND: u8 = 0xF4;
pub const CMD_IDLE_EXIT: u8 = 0xFE;

pub const CMD_RPT_MS: u8 = 0xE0;
pub const CMD_RPT_BYTE_KB: u8 = 0xE1;
pub const CMD_RPT_BIT_KB: u8 = 0xE2;
pub const CMD_RPT_CONSUME: u8 = 0xE3;
pub const CMD_RPT_SYS: u8 = 0xE4;

pub const CMD_SET_LINK: u8 = 0xC0;
pub const CMD_SET_CONFIG: u8 = 0xC1;
pub const CMD_GET_CONFIG: u8 = 0xC2;
pub const CMD_SET_NAME: u8 = 0xC3;
pub const CMD_GET_NAME: u8 = 0xC4;
pub const CMD_CLR_DEVICE: u8 = 0xC5;
pub const CMD_NEW_ADV: u8 = 0xC7;
pub const CMD_RF_STS_SYSC: u8 = 0xC9;
pub const CMD_SET_24G_NAME: u8 = 0xCA;
pub const CMD_GO_TEST: u8 = 0xCF;
pub const CMD_RF_DFU: u8 = 0xB1;
pub const CMD_WRITE_DATA: u8 = 0x80;
pub const CMD_READ_DATA: u8 = 0x81;

/// `TYPE_RX_STATE` (ansi.h).
pub const RX_IDLE: u8 = 0;
pub const RX_RECEIVING: u8 = 1;
pub const RX_DONE: u8 = 2;
pub const RX_FAIL: u8 = 3;
pub const RX_OV_ERR: u8 = 4;
pub const RX_SUM_ERR: u8 = 5;
pub const RX_CMD_ERR: u8 = 6;
pub const RX_DATA_ERR: u8 = 7;
pub const RX_DATA_OV: u8 = 8;
pub const RX_FORMAT_ERR: u8 = 9;
pub const TX_OK: u8 = 0xE0;
pub const TX_DONE: u8 = 0xE1;
pub const TX_BUSY: u8 = 0xE2;
pub const TX_TIMEOUT: u8 = 0xE3;
pub const TX_DATA_ERR: u8 = 0xE4;

/// 32 complete taps fit during a short radio reset (press + release).
pub const RF_QUEUE_SIZE: usize = 64;
pub const RF_REPLAY_MAX_AGE_MS: u32 = 1000;
const REPORT_SIZES: [u8; 5] = [5, 8, 16, 2, 2];

#[derive(Clone, Copy)]
pub struct Queued {
    pub kind: u8,
    pub size: u8,
    pub data: [u8; 16],
    pub captured_at: u32,
}

/// `USART_MGR_STRUCT` (ansi.h).
pub struct UsartMgr {
    pub rxd_state: u8,
    pub rxd_len: u8,
    pub rxd_over_time: u8,
    pub txd_buf: [u8; UART_MAX_LEN],
    pub rxd_buf: [u8; UART_MAX_LEN],
}

/// rf.c globals and function-local statics.
pub struct RfState {
    pub usart_mgr: UsartMgr,
    pub uart_bit_report_buf: [u8; NKRO_REPORT_BITS + 1],
    pub func_tab: [u8; 32],
    pub bitkb_report_buf: [u8; NKRO_REPORT_BITS + 1],
    pub bytekb_report_buf: [u8; 8],
    pub sync_lost: u8,
    pub disconnect_delay: u8,
    pub f_bit_kb_act: bool,
    /* Recovery reports belong to the channel on which they were captured. */
    pub reset_phase: u8,
    pub rf_recovering: bool,
    pub pending_cmd: u8,
    pub waiting_ack: bool,
    pub rf_queue: [Queued; RF_QUEUE_SIZE],
    pub queue_head: u8,
    pub queue_count: u8,
    pub latest_reports: [[u8; 16]; 5],
    pub replay_timer: u32,
    // statics
    report_func_timer: u32,
    error_cnt: u8,
    sts_interval_timer: u32,
    link_state_temp: u8,
    reset_cooldown: u8,
    usb_sync_prescaler: u8,
    reset_timer: u32,
    rx_timeout_timer: u32,
}

impl Default for RfState {
    fn default() -> Self {
        Self::new()
    }
}

impl RfState {
    pub const fn new() -> Self {
        Self {
            usart_mgr: UsartMgr {
                rxd_state: RX_IDLE,
                rxd_len: 0,
                rxd_over_time: 0,
                txd_buf: [0; UART_MAX_LEN],
                rxd_buf: [0; UART_MAX_LEN],
            },
            uart_bit_report_buf: [0; NKRO_REPORT_BITS + 1],
            func_tab: [0; 32],
            bitkb_report_buf: [0; NKRO_REPORT_BITS + 1],
            bytekb_report_buf: [0; 8],
            sync_lost: 0,
            disconnect_delay: 0,
            f_bit_kb_act: false,
            reset_phase: 0,
            rf_recovering: false,
            pending_cmd: 0,
            waiting_ack: false,
            rf_queue: [Queued { kind: 0, size: 0, data: [0; 16], captured_at: 0 }; RF_QUEUE_SIZE],
            queue_head: 0,
            queue_count: 0,
            latest_reports: [[0; 16]; 5],
            replay_timer: 0,
            report_func_timer: 0,
            error_cnt: 0,
            sts_interval_timer: 0,
            link_state_temp: RF_DISCONNECT,
            reset_cooldown: 0,
            usb_sync_prescaler: 0,
            reset_timer: 0,
            rx_timeout_timer: 0,
        }
    }
}

/// `get_checksum()`: outgoing frames.
pub fn get_checksum(buf: &[u8]) -> u8 {
    buf.iter().fold(0u8, |a, &b| a.wrapping_add(b)) ^ UART_HEAD
}

impl Kb {
    /// `rf_link_down()`: link setup or loss — the module would drop reports,
    /// so queue them for replay.
    fn rf_link_down(&self) -> bool {
        matches!(self.dev_info.rf_state, RF_IDLE | RF_LINKING | RF_DISCONNECT)
    }

    /// `rf_queue_resync()`: old taps expire; current states still release
    /// any keys held by the host.
    fn rf_queue_resync(&mut self, hal: &impl Hal) {
        let rf = &mut self.rf;
        rf.queue_head = 0;
        rf.queue_count = 5;
        for i in 0..5 {
            rf.rf_queue[i] = Queued {
                kind: CMD_RPT_MS + i as u8,
                size: REPORT_SIZES[i],
                data: rf.latest_reports[i],
                captured_at: hal.now(),
            };
        }
    }

    /// `rf_reset_sync_lost()`.
    pub fn rf_reset_sync_lost(&mut self, hal: &mut impl Hal) {
        self.rf.sync_lost = 0;
        self.f_rf_reset = false;
        self.rf.reset_phase = 0;
        self.rf.rf_recovering = false;
        self.rf.queue_head = 0;
        self.rf.queue_count = 0;
        self.rf.latest_reports = [[0; 16]; 5];
        hal.nrf_reset_pin(true);
        self.rf.usart_mgr.rxd_len = 0;
        self.rf.usart_mgr.rxd_state = RX_IDLE;
    }

    /// `uart_auto_nkey_send()`. The dongle HID descriptor exposes usages
    /// 0..111. Give each usage a fixed endpoint: bitmap for 1..111, byte
    /// slots for higher usages, so a held key never moves between the
    /// dongle's independent keyboard interfaces.
    fn uart_auto_nkey_send(&mut self, hal: &mut impl Hal, now_bit_report: &[u8; NKRO_REPORT_BITS + 1]) {
        let size = NKRO_REPORT_BITS + 1;
        let mut next_byte = [0u8; 8];
        let mut next_bit = [0u8; 16];
        next_byte[0] = now_bit_report[0];
        next_bit[0] = now_bit_report[0];
        let mut slot = 2;

        for key in 112..((size - 1) * 8) {
            if now_bit_report[1 + key / 8] & (1 << (key % 8)) != 0 && slot < 8 {
                next_byte[slot] = key as u8;
                slot += 1;
            }
        }
        for key in 1..112usize {
            if 1 + key / 8 < size && now_bit_report[1 + key / 8] & (1 << (key % 8)) != 0 {
                next_bit[1 + key / 8] |= 1 << (key % 8);
            }
        }

        let rf = &mut self.rf;
        let f_byte_send = rf.bytekb_report_buf != next_byte;
        let f_bit_send = rf.uart_bit_report_buf[1..16] != next_bit[1..16]
            || (rf.f_bit_kb_act && rf.uart_bit_report_buf[0] != next_bit[0]);
        rf.bytekb_report_buf = next_byte;
        rf.uart_bit_report_buf[..16].copy_from_slice(&next_bit);

        if f_bit_send {
            /* Retain through idle so modifier changes and release retries reach
               the bitmap endpoint until m_break_all_key() clears both reports. */
            self.rf.f_bit_kb_act = true;
            let buf = self.rf.uart_bit_report_buf;
            self.uart_send_report(hal, CMD_RPT_BIT_KB, &buf[..16]);
        }
        if f_byte_send {
            let buf = self.rf.bytekb_report_buf;
            self.uart_send_report(hal, CMD_RPT_BYTE_KB, &buf);
        }
    }

    /// `uart_send_report_func()`: periodic resend and replay pacing.
    pub fn uart_send_report_func(&mut self, hal: &mut impl Hal) {
        if self.dev_info.link_mode == LINK_USB || self.f_wakeup_prepare || self.f_goto_sleep {
            return;
        }
        if self.f_rf_reset || self.rf.rf_recovering || self.rf.queue_count != 0 || self.rf_link_down() {
            self.rf_replay_task(hal);
            return;
        }
        self.keyboard_protocol = 1;

        if hal.now().wrapping_sub(self.rf.report_func_timer) > 100 {
            self.rf.report_func_timer = hal.now();
            if self.no_act_time <= 200 {
                let byte = self.rf.bytekb_report_buf;
                self.uart_send_report(hal, CMD_RPT_BYTE_KB, &byte);

                if self.rf.f_bit_kb_act {
                    let bit = self.rf.uart_bit_report_buf;
                    self.uart_send_report(hal, CMD_RPT_BIT_KB, &bit[..16]);
                }
            }
        }
    }

    /// `uart_send_consumer_report()`.
    pub fn uart_send_consumer_report(&mut self, hal: &mut impl Hal, usage: u16) {
        self.no_act_time = 0;
        self.uart_send_report(hal, CMD_RPT_CONSUME, &usage.to_le_bytes());
    }

    /// `uart_send_mouse_report()`.
    pub fn uart_send_mouse_report(&mut self, hal: &mut impl Hal, report: [u8; 5]) {
        self.no_act_time = 0;
        self.uart_send_report(hal, CMD_RPT_MS, &report);
    }

    /// `uart_send_system_report()`.
    pub fn uart_send_system_report(&mut self, hal: &mut impl Hal, usage: u16) {
        self.no_act_time = 0;
        self.uart_send_report(hal, CMD_RPT_SYS, &usage.to_le_bytes());
    }

    /// `uart_send_report_keyboard()`.
    pub fn uart_send_report_keyboard(&mut self, hal: &mut impl Hal, mut report: [u8; 8]) {
        self.no_act_time = 0;
        report[1] = 0;
        self.uart_send_report(hal, CMD_RPT_BYTE_KB, &report);
        self.rf.bytekb_report_buf = report;
    }

    /// `uart_send_report_nkro()`: `report` is `{mods, bits[30]}`.
    pub fn uart_send_report_nkro(&mut self, hal: &mut impl Hal, report: [u8; NKRO_REPORT_BITS + 1]) {
        self.no_act_time = 0;
        self.uart_auto_nkey_send(hal, &report);
        self.rf.bitkb_report_buf = report;
    }

    /// `RF_Protocol_Receive()`: parse the frame in `RXDBuf`.
    fn rf_protocol_receive(&mut self) {
        let m = &mut self.rf.usart_mgr;
        if m.rxd_state != RX_DONE {
            return;
        }
        if m.rxd_len > 4 {
            let rx_len = m.rxd_buf[3] as usize;
            if (m.rxd_len as usize) - 5 != rx_len {
                return;
            }
            /* Bounds check: RX_LEN comes from the RF module. */
            if rx_len > UART_MAX_LEN - 5 {
                m.rxd_state = RX_DATA_OV;
                return;
            }
            let check_sum = m.rxd_buf[4..4 + rx_len].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            if check_sum != m.rxd_buf[4 + rx_len] {
                m.rxd_state = RX_SUM_ERR;
                return;
            }
        } else if m.rxd_len == 3 {
            if m.rxd_buf[2] == 0xA0 {
                /* Short ACKs have no payload; do not reuse the last frame. */
                m.rxd_buf[3] = 0;
            } else {
                return;
            }
        } else {
            return;
        }

        let rx_cmd = m.rxd_buf[1];
        let rx_len = m.rxd_buf[3] as usize;
        let buf = m.rxd_buf;

        /* Only validated frames acknowledge commands or refresh recovery. */
        if self.rf.waiting_ack && rx_cmd == self.rf.pending_cmd {
            self.f_uart_ack = true;
        }
        self.rf.sync_lost = 0;

        match rx_cmd {
            CMD_HAND => self.f_rf_hand_ok = true,
            CMD_24G_SUSPEND => {
                if self.dev_info.link_mode == LINK_RF_24 {
                    self.f_goto_sleep = true;
                }
            }
            CMD_NEW_ADV => self.f_rf_new_adv_ok = true,
            CMD_RF_STS_SYSC => {
                if rx_len >= 5 {
                    if self.dev_info.link_mode == buf[4] {
                        self.rf.error_cnt = 0;

                        self.dev_info.rf_state = buf[5];
                        if self.rf.reset_phase == 0
                            && !self.f_send_channel
                            && !self.f_wakeup_prepare
                            && !self.f_goto_sleep
                            && self.dev_info.rf_state == RF_CONNECT
                        {
                            self.rf.rf_recovering = false;
                        }

                        if self.dev_info.rf_state == RF_CONNECT && (buf[6] & 0xF8) == 0 {
                            self.dev_info.rf_led = buf[6];
                        }

                        self.dev_info.rf_charge = buf[7];

                        if buf[8] <= 100 {
                            self.dev_info.rf_baterry = buf[8];
                        }
                    } else if self.dev_info.rf_state != RF_INVALID {
                        if self.rf.error_cnt >= 5 {
                            self.rf.error_cnt = 0;
                            self.f_send_channel = true;
                        } else {
                            self.rf.error_cnt += 1;
                        }
                    }
                    self.f_rf_sts_sysc_ok = true;
                }
            }
            CMD_READ_DATA => {
                if rx_len >= FUNC_VALID_LEN {
                    self.rf.func_tab.copy_from_slice(&buf[4..4 + FUNC_VALID_LEN]);
                    let func_tab = self.rf.func_tab;

                    if func_tab[4] <= LINK_USB {
                        self.dev_info.link_mode = func_tab[4];
                    }
                    if func_tab[5] < LINK_USB {
                        self.dev_info.rf_channel = func_tab[5];
                    }
                    if func_tab[6] <= LINK_BT_3 && func_tab[6] >= LINK_BT_1 {
                        self.dev_info.ble_channel = func_tab[6];
                    }
                    self.f_rf_read_data_ok = true;
                }
            }
            _ => {}
        }

        let m = &mut self.rf.usart_mgr;
        m.rxd_len = 0;
        m.rxd_state = RX_IDLE;
        m.rxd_over_time = 0;
    }

    /// `uart_send_cmd()`.
    pub fn uart_send_cmd(&mut self, hal: &mut impl Hal, cmd: u8, mut wait_ack: u8, delayms: u8) -> u8 {
        if delayms != 0 {
            hal.wait_ms(delayms as u32);
        }
        /* Drain old replies before changing the selected command's state. */
        self.uart_receive_pro(hal);

        let link_mode = self.dev_info.link_mode;
        let t = &mut self.rf.usart_mgr.txd_buf;
        t[0] = UART_HEAD;
        t[1] = cmd;
        t[2] = 0x00;
        t[3] = 0;

        match cmd {
            CMD_SLEEP | CMD_HAND => {
                t[3] = 1;
                t[4] = 0;
                t[5] = 0;
            }
            CMD_RF_STS_SYSC => {
                t[3] = 1;
                t[4] = link_mode;
                t[5] = link_mode;
            }
            CMD_SET_LINK => {
                self.dev_info.rf_state = RF_LINKING;
                t[3] = 1;
                t[4] = link_mode;
                t[5] = link_mode;
                self.rf_linking_time = 0;
                self.rf.disconnect_delay = 0xFF;
            }
            CMD_NEW_ADV => {
                self.dev_info.rf_state = RF_PAIRING;
                t[3] = 2;
                t[4] = link_mode;
                t[5] = 1;
                t[6] = link_mode + 1;
                self.rf_linking_time = 0;
                self.rf.disconnect_delay = 0xFF;
                self.f_rf_new_adv_ok = false;
            }
            CMD_CLR_DEVICE => {
                t[3] = 1;
                t[4] = 0;
                t[5] = 0;
            }
            CMD_SET_CONFIG => {
                t[3] = 1;
                t[4] = POWER_DOWN_DELAY;
                t[5] = POWER_DOWN_DELAY;
            }
            CMD_SET_NAME => {
                t[3] = 11;
                t[4] = 1;
                t[5] = 9;
                t[6..15].copy_from_slice(b"Air96 V2\0");
            }
            CMD_SET_24G_NAME => {
                /* UTF-16LE encoded name for 2.4G dongle */
                const NAME_24G: &[u8; 42] = b"N\0u\0P\0h\0y\0 \0A\0i\0r\09\06\0 \0V\02\0 \0D\0o\0n\0g\0l\0e\0";
                t[3] = 44;
                t[4] = 44;
                t[5] = 3;
                t[6..6 + NAME_24G.len()].copy_from_slice(NAME_24G);
            }
            CMD_READ_DATA => {
                t[3] = 2;
                t[4] = 0x00;
                t[5] = FUNC_VALID_LEN as u8;
                t[6] = FUNC_VALID_LEN as u8;
            }
            CMD_RF_DFU => {
                t[3] = 1;
                t[4] = 0;
                t[5] = 0;
            }
            _ => {}
        }

        let len = t[3] as usize;
        if len > UART_MAX_LEN - 5 {
            return TX_DATA_ERR;
        }

        self.rf.pending_cmd = cmd;
        self.rf.waiting_ack = wait_ack != 0;
        self.f_uart_ack = false;
        let t = &mut self.rf.usart_mgr.txd_buf;
        t[4 + len] = get_checksum(&t[4..4 + len]);
        let frame = *t;
        hal.uart_send_bytes(&frame[..len + 5]);

        if wait_ack != 0 {
            while wait_ack != 0 {
                wait_ack -= 1;
                hal.wait_ms(1);
                self.uart_receive_pro(hal);
                if self.f_uart_ack {
                    self.rf.waiting_ack = false;
                    return TX_OK;
                }
            }
        } else {
            return TX_OK;
        }

        self.rf.waiting_ack = false;
        TX_TIMEOUT
    }

    /// `rf_wake()`: Sleep_Handle owns wakeup. Preserve transitions until the
    /// selected radio channel reports connected, just as during reset recovery.
    pub fn rf_wake(&mut self, hal: &mut impl Hal) {
        /* rf_linking_time kept counting while asleep. */
        self.rf_linking_time = 0;
        self.rf.rf_recovering = true;
        self.dev_info.rf_state = RF_LINKING;
        self.uart_send_cmd(hal, CMD_HAND, 0, 1);
        self.uart_send_cmd(hal, CMD_RF_STS_SYSC, 0, 0);
    }

    /// `dev_sts_sync()`: RF module state sync.
    pub fn dev_sts_sync(&mut self, hal: &mut impl Hal) {
        if self.f_wakeup_prepare || self.f_goto_sleep {
            if self.rf.reset_phase != 0 {
                hal.nrf_reset_pin(true);
            }
            self.rf.reset_phase = 0;
            self.f_rf_reset = false;
            self.rf.sync_lost = 0;
            self.rf.sts_interval_timer = hal.now();
            return;
        }

        /* Service every scan, before the 200ms status throttle. */
        if self.rf.reset_phase == 0 && self.f_rf_reset {
            self.f_rf_reset = false;
            self.rf.reset_phase = 1;
            self.rf.rf_recovering = true;
            self.rf.reset_timer = hal.now();
        }
        if self.rf.reset_phase != 0 {
            let hold = if self.rf.reset_phase == 1 { 100 } else { 50 };
            if hal.now().wrapping_sub(self.rf.reset_timer) < hold {
                return;
            }
            self.rf.reset_timer = hal.now();
            if self.rf.reset_phase == 1 {
                hal.nrf_reset_pin(false);
                self.rf.reset_phase = 2;
            } else if self.rf.reset_phase == 2 {
                hal.nrf_reset_pin(true);
                self.rf.reset_phase = 3;
            } else {
                self.rf.reset_phase = 0;
                self.rf.reset_cooldown = 3;
                self.dev_info.rf_state = RF_LINKING;
                self.f_send_channel = true;
                self.rf.usart_mgr.rxd_len = 0;
                self.rf.usart_mgr.rxd_state = RX_IDLE;
                self.rf.sync_lost = 0;
                self.rf.sts_interval_timer = hal.now();
            }
            return;
        }

        if hal.now().wrapping_sub(self.rf.sts_interval_timer) < 200 {
            return;
        }
        self.rf.sts_interval_timer = hal.now();

        /* Post-reset cooldown: skip sync_lost counting to give NRF time to boot */
        if self.rf.reset_cooldown > 0 {
            self.rf.reset_cooldown -= 1;
        }

        if self.f_send_channel {
            self.rf.link_state_temp = RF_DISCONNECT;
            self.uart_send_cmd(hal, CMD_SET_LINK, 0, 0);
            self.f_send_channel = false;
        }

        if self.dev_info.link_mode == LINK_USB {
            if self.host_mode != HOST_USB_TYPE {
                self.host_mode = HOST_USB_TYPE;
                self.driver = HostDriver::Usb;
                self.m_break_all_key(hal);
            }
            self.rf_blink_cnt = 0;
        } else {
            if self.host_mode != HOST_RF_TYPE {
                self.host_mode = HOST_RF_TYPE;
                self.m_break_all_key(hal);
                self.driver = HostDriver::Rf;
            }

            if self.dev_info.rf_state != RF_CONNECT {
                if self.rf.disconnect_delay >= 10 {
                    if self.rf.link_state_temp != self.dev_info.rf_state {
                        self.rf_blink_cnt = 3;
                        self.rf_link_show_time = 0;
                        self.rf.link_state_temp = self.dev_info.rf_state;
                    }
                } else {
                    self.rf.disconnect_delay += 1;
                }
            } else {
                self.rf_linking_time = 0;
                self.rf.disconnect_delay = 0;
                self.rf_blink_cnt = 0;

                if self.rf.link_state_temp != RF_CONNECT {
                    self.rf.link_state_temp = RF_CONNECT;
                    self.rf_link_show_time = 0;
                }
            }
        }

        /* In USB mode, only sync every ~2 seconds (10 × 200ms) for battery info.
           In RF mode, sync every 200ms for connection state. */
        if self.dev_info.link_mode == LINK_USB {
            self.rf.usb_sync_prescaler += 1;
            if self.rf.usb_sync_prescaler >= 10 {
                self.rf.usb_sync_prescaler = 0;
                self.uart_send_cmd(hal, CMD_RF_STS_SYSC, 0, 0);
            }
        } else {
            self.rf.usb_sync_prescaler = 0;
            self.uart_send_cmd(hal, CMD_RF_STS_SYSC, 0, 0);
        }

        if self.dev_info.link_mode != LINK_USB && self.rf.reset_cooldown == 0 {
            self.rf.sync_lost += 1;
            if self.rf.sync_lost >= 10 {
                self.rf.sync_lost = 0;
                self.f_rf_reset = true;
            }
        }
    }

    /// `uart_send_report()`.
    pub fn uart_send_report(&mut self, hal: &mut impl Hal, report_type: u8, report_buf: &[u8]) {
        let report_size = report_buf.len();
        if report_size > UART_MAX_LEN - 5 {
            return;
        }
        if !self.f_dial_sw_init_ok {
            return;
        }
        if self.dev_info.link_mode == LINK_USB {
            return;
        }
        if !(CMD_RPT_MS..=CMD_RPT_SYS).contains(&report_type) || report_size > 16 {
            return;
        }
        let index = (report_type - CMD_RPT_MS) as usize;
        self.rf.latest_reports[index][..report_size].copy_from_slice(report_buf);
        /* Relative mouse movement must not be repeated during resynchronization. */
        if index == 0 {
            self.rf.latest_reports[0][1..5].fill(0);
        }
        if self.f_rf_reset
            || self.rf.rf_recovering
            || self.rf.queue_count != 0
            || self.f_wakeup_prepare
            || self.f_goto_sleep
            || self.rf_link_down()
        {
            if self.rf.queue_count as usize == RF_QUEUE_SIZE {
                /* Bounded outage: sacrifice old transitions, keep current states. */
                self.rf_queue_resync(hal);
                return;
            }
            let tail = (self.rf.queue_head as usize + self.rf.queue_count as usize) % RF_QUEUE_SIZE;
            let q = &mut self.rf.rf_queue[tail];
            q.captured_at = hal.now();
            q.kind = report_type;
            q.size = report_size as u8;
            q.data[..report_size].copy_from_slice(report_buf);
            self.rf.queue_count += 1;
            return;
        }

        let t = &mut self.rf.usart_mgr.txd_buf;
        t[0] = UART_HEAD;
        t[1] = report_type;
        t[2] = 0x01;
        t[3] = report_size as u8;
        t[4..4 + report_size].copy_from_slice(report_buf);
        t[4 + report_size] = get_checksum(report_buf);
        let frame = *t;
        hal.uart_send_bytes(&frame[..report_size + 5]);
    }

    /// `rf_replay_task()`: drain one report per interval, retaining order
    /// across newly arriving input.
    fn rf_replay_task(&mut self, hal: &mut impl Hal) {
        let interval = if self.dev_info.link_mode == LINK_RF_24 { 2 } else { 8 };
        if self.f_rf_reset
            || self.rf.rf_recovering
            || self.f_wakeup_prepare
            || self.f_goto_sleep
            || self.rf.queue_count == 0
            || hal.now().wrapping_sub(self.rf.replay_timer) < interval
        {
            return;
        }
        if self.dev_info.link_mode == LINK_USB || self.dev_info.rf_state != RF_CONNECT {
            return;
        }
        if hal.now().wrapping_sub(self.rf.rf_queue[self.rf.queue_head as usize].captured_at) >= RF_REPLAY_MAX_AGE_MS {
            self.rf_queue_resync(hal);
        }
        self.rf.replay_timer = hal.now();
        let q = self.rf.rf_queue[self.rf.queue_head as usize];
        let size = q.size as usize;
        let mut frame = [0u8; 21];
        frame[0] = UART_HEAD;
        frame[1] = q.kind;
        frame[2] = 1;
        frame[3] = q.size;
        frame[4..4 + size].copy_from_slice(&q.data[..size]);
        frame[4 + size] = get_checksum(&q.data[..size]);
        hal.uart_send_bytes(&frame[..size + 5]);
        self.rf.queue_head = ((self.rf.queue_head as usize + 1) % RF_QUEUE_SIZE) as u8;
        self.rf.queue_count -= 1;
    }

    /// `uart_receive_pro()`: byte-wise frame assembly.
    pub fn uart_receive_pro(&mut self, hal: &mut impl Hal) {
        /* Timeout recovery: discard a partial packet after 10ms without bytes. */
        if self.rf.usart_mgr.rxd_len > 0 && hal.now().wrapping_sub(self.rf.rx_timeout_timer) > 10 {
            self.rf.usart_mgr.rxd_len = 0;
            self.rf.usart_mgr.rxd_state = RX_IDLE;
        }

        while let Some(b) = hal.uart_read() {
            self.rf.rx_timeout_timer = hal.now();
            let m = &mut self.rf.usart_mgr;

            if m.rxd_len == 0 {
                if b == UART_HEAD {
                    m.rxd_buf[0] = b;
                    m.rxd_len = 1;
                }
            } else if m.rxd_len == 1 {
                m.rxd_buf[1] = b;
                m.rxd_len = 2;
            } else if m.rxd_len == 2 {
                m.rxd_buf[2] = b;
                m.rxd_len = 3;
                if b == 0xA0 {
                    m.rxd_state = RX_DONE;
                    self.rf_protocol_receive();
                    let m = &mut self.rf.usart_mgr;
                    m.rxd_len = 0;
                    m.rxd_state = RX_IDLE;
                }
            } else if m.rxd_len == 3 {
                if b as usize > UART_MAX_LEN - 5 {
                    m.rxd_len = 0;
                    m.rxd_state = RX_IDLE;
                } else {
                    m.rxd_buf[3] = b;
                    m.rxd_len = 4;
                }
            } else if (m.rxd_len as usize) < UART_MAX_LEN {
                m.rxd_buf[m.rxd_len as usize] = b;
                m.rxd_len += 1;
                if m.rxd_len as usize == 5 + m.rxd_buf[3] as usize {
                    m.rxd_state = RX_DONE;
                    self.rf_protocol_receive();
                    let m = &mut self.rf.usart_mgr;
                    m.rxd_len = 0;
                    m.rxd_state = RX_IDLE;
                }
            } else {
                m.rxd_len = 0;
                m.rxd_state = RX_IDLE;
            }
        }
    }

    /// `rf_init_try_cmd()`: send a command and poll for its response.
    fn rf_init_try_cmd(&mut self, hal: &mut impl Hal, cmd: u8, ok: fn(&Kb) -> bool, clear: fn(&mut Kb)) -> bool {
        clear(self);
        for _ in 0..10 {
            self.uart_send_cmd(hal, cmd, 0, 5);
            for _ in 0..10 {
                hal.wait_ms(1);
                self.uart_receive_pro(hal);
                if ok(self) {
                    return true;
                }
            }
        }
        false
    }

    /// `rf_device_init()`.
    pub fn rf_device_init(&mut self, hal: &mut impl Hal) {
        self.rf_init_try_cmd(hal, CMD_HAND, |k| k.f_rf_hand_ok, |k| k.f_rf_hand_ok = false);
        self.rf_init_try_cmd(hal, CMD_READ_DATA, |k| k.f_rf_read_data_ok, |k| k.f_rf_read_data_ok = false);
        self.rf_init_try_cmd(hal, CMD_RF_STS_SYSC, |k| k.f_rf_sts_sysc_ok, |k| k.f_rf_sts_sysc_ok = false);

        self.uart_send_cmd(hal, CMD_SET_NAME, 10, 5);

        self.uart_send_cmd(hal, CMD_SET_24G_NAME, 10, 5);
    }
}
