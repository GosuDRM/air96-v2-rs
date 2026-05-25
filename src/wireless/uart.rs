//! NRF module UART protocol — full port of rf.c
//!
//! 460800 baud, 8E1 (8 data + even parity + 1 stop = 11 bits/byte)
//! Frame: HEAD(1) + CMD(1) + ACK(1) + LEN(1) + DATA(LEN) + CHECKSUM(1)
//!
//! Ported from rf.c (Persama, 2023) — GosuDRM 2026, all 29 fixes applied.

#![allow(clippy::needless_range_loop)]

// ── Frame constants (ansi.h) ──────────────────────────────────────────
pub const UART_HEAD: u8 = 0x5A;
pub const UART_MAX_LEN: usize = 64;

// ── RF states ─────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RfState {
    Idle = 0,
    Pairing = 1,
    Linking = 2,
    Connect = 3,
    Disconnect = 4,
    Sleep = 5,
    Snif = 6,
    Invalid = 0xFE,
    ErrState = 0xFF,
}

impl RfState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Idle, 1 => Self::Pairing, 2 => Self::Linking,
            3 => Self::Connect, 4 => Self::Disconnect, 5 => Self::Sleep,
            6 => Self::Snif, 0xFE => Self::Invalid, _ => Self::ErrState,
        }
    }
}

// ── Link modes ────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LinkMode {
    Rf24 = 0,
    Bt1 = 1,
    Bt2 = 2,
    Bt3 = 3,
    Usb = 4,
}

impl LinkMode {
    pub fn from_u8(v: u8) -> Self {
        match v { 0 => Self::Rf24, 1 => Self::Bt1, 2 => Self::Bt2, 3 => Self::Bt3, _ => Self::Usb }
    }
}

// ── Commands ──────────────────────────────────────────────────────────
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

// ── RX states ─────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RxState {
    Idle = 0,
    Receiving = 1,
    Done = 2,
    Fail = 3,
    OvErr = 4,
    SumErr = 5,
    CmdErr = 6,
    DataErr = 7,
    DataOv = 8,
    FormatErr = 9,
}

// ── TX result ─────────────────────────────────────────────────────────
pub const TX_OK: u8 = 0xE0;
pub const TX_DONE: u8 = 0xE1;
pub const TX_BUSY: u8 = 0xE2;
pub const TX_TIMEOUT: u8 = 0xE3;
pub const TX_DATA_ERR: u8 = 0xE4;

// ── Protocol manager ──────────────────────────────────────────────────

pub struct UartProtocol {
    pub rx_state: RxState,
    pub rx_buf: [u8; UART_MAX_LEN],
    pub rx_len: u8,
    pub tx_buf: [u8; UART_MAX_LEN],

    // Flags (ported from rf.c globals)
    pub f_uart_ack: bool,
    pub f_rf_hand_ok: bool,
    pub f_rf_read_data_ok: bool,
    pub f_rf_sts_sysc_ok: bool,
    pub f_rf_new_adv_ok: bool,
    pub f_rf_reset: bool,
    pub f_goto_sleep: bool,
    pub f_wakeup_prepare: bool,

    // Report buffers
    pub uart_bit_report_buf: [u8; 32],
    pub bitkb_report_buf: [u8; 32],
    pub bytekb_report_buf: [u8; 8],
    pub conkb_report: u16,
    pub syskb_report: u16,
    pub f_bit_kb_act: bool,
    pub uart_repeat_flag: bool,

    // Device info shadow (synced from RF module)
    pub link_mode: LinkMode,
    pub rf_channel: u8,
    pub ble_channel: u8,
    pub rf_state: RfState,
    pub rf_charge: u8,
    pub rf_led: u8,
    pub rf_battery: u8,
    pub sys_sw_state: u8,

    // Protocol state
    pub sync_lost: u8,
    pub disconnect_delay: u8,
    pub f_send_channel: bool,
    pub error_cnt: u8,
    pub uart_pending: core::cell::Cell<bool>,
}

impl Default for UartProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl UartProtocol {
    pub fn new() -> Self {
        Self {
            rx_state: RxState::Idle,
            rx_buf: [0; UART_MAX_LEN],
            rx_len: 0,
            tx_buf: [0; UART_MAX_LEN],
            f_uart_ack: false,
            f_rf_hand_ok: false,
            f_rf_read_data_ok: false,
            f_rf_sts_sysc_ok: false,
            f_rf_new_adv_ok: false,
            f_rf_reset: false,
            f_goto_sleep: false,
            f_wakeup_prepare: false,
            uart_bit_report_buf: [0; 32],
            bitkb_report_buf: [0; 32],
            bytekb_report_buf: [0; 8],
            conkb_report: 0,
            syskb_report: 0,
            f_bit_kb_act: false,
            uart_repeat_flag: false,
            link_mode: LinkMode::Usb,
            rf_channel: 0,
            ble_channel: 1,
            rf_state: RfState::Idle,
            rf_charge: 0,
            rf_led: 0,
            rf_battery: 100,
            sys_sw_state: 0xA2,
            sync_lost: 0,
            disconnect_delay: 0,
            f_send_channel: false,
            error_cnt: 0,
            uart_pending: core::cell::Cell::new(false),
        }
    }

    // ── Checksum (port of get_checksum, rf.c:564-574) ────────────────

    /// XOR-folded sum of data bytes, then XOR with UART_HEAD
    pub fn checksum(data: &[u8]) -> u8 {
        data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b)) ^ UART_HEAD
    }

    // ── Command builder (port of uart_send_cmd, rf.c:299-452) ─────────

    /// Build a command frame in tx_buf. Returns frame length (header + 4 + data + 1 checksum).
    /// Port of uart_send_cmd — applies B2 checksum fix for all commands.
    pub fn build_cmd(&mut self, cmd: u8, data: &[u8]) -> usize {
        self.tx_buf = [0; UART_MAX_LEN];
        self.tx_buf[0] = UART_HEAD;
        self.tx_buf[1] = cmd;
        self.tx_buf[2] = 0x00; // ack
        self.tx_buf[3] = data.len() as u8;
        self.tx_buf[4..4 + data.len()].copy_from_slice(data);
        self.tx_buf[4 + data.len()] = Self::checksum(data);
        self.uart_pending.set(true);
        data.len() + 5
    }

    /// Build command with protocol-aware logic (port of uart_send_cmd switch)
    pub fn build_link_cmd(&mut self, cmd: u8) -> usize {
        let len = match cmd {
            CMD_SLEEP | CMD_HAND => self.build_cmd(cmd, &[0]),
            CMD_SET_LINK => {
                let ch = self.link_mode as u8;
                self.rf_state = RfState::Linking;
                self.disconnect_delay = 0xFF;
                self.build_cmd(cmd, &[ch; 1]) // data: [link_mode] for checksum; TXDBuf[5] also had link_mode in C but checksum overwrites it
            }
            CMD_RF_STS_SYSC => {
                let ch = self.link_mode as u8;
                self.build_cmd(cmd, &[ch; 1])
            }
            CMD_NEW_ADV => {
                self.rf_state = RfState::Pairing;
                self.disconnect_delay = 0xFF;
                self.f_rf_new_adv_ok = false;
                let ch = self.link_mode as u8;
                self.build_cmd(cmd, &[ch, 1]) // mode, 1 — 2 data bytes, checksum at [6]
            }
            CMD_CLR_DEVICE => self.build_cmd(cmd, &[0]),
            CMD_SET_CONFIG => {
                // POWER_DOWN_DELAY = 24
                self.build_cmd(cmd, &[24; 1])
            }
            CMD_RF_DFU => self.build_cmd(cmd, &[0]),
            CMD_READ_DATA => {
                // C: no custom handler in uart_send_cmd switch — sent with zero payload
                self.build_cmd(cmd, &[])
            }
            CMD_SET_NAME => {
                // Format: [ID=1, len, "name\0"]
                // C firmware sends 17 bytes: ID(1) + len(15) + "NuPhy Air96 V2\0"
                let name = b"\x01\x0AAir96 V2\x00";
                self.tx_buf = [0; UART_MAX_LEN];
                self.tx_buf[0] = UART_HEAD;
                self.tx_buf[1] = cmd;
                self.tx_buf[2] = 0x00;
                self.tx_buf[3] = name.len() as u8;
                self.tx_buf[4..4 + name.len()].copy_from_slice(name);
                self.tx_buf[4 + name.len()] = Self::checksum(name);
                name.len() + 5
            }
            CMD_SET_24G_NAME => {
                // 44-byte name with interleaved nulls: valid_len + fixed + "Air96 V2 Dongle"
                let raw = b"\x2C\x03A\x00i\x00r\x009\x006\x00 \x00V\x002\x00 \x00D\x00o\x00n\x00g\x00l\x00e\x00";
                let len = raw.len();
                self.tx_buf = [0; UART_MAX_LEN];
                self.tx_buf[0] = UART_HEAD;
                self.tx_buf[1] = cmd;
                self.tx_buf[2] = 0x00;
                self.tx_buf[3] = len as u8;
                self.tx_buf[4..4 + len].copy_from_slice(raw);
                self.tx_buf[4 + len] = Self::checksum(raw);
                len + 5
            }
            _ => self.build_cmd(cmd, &[]),
        };
        self.uart_pending.set(true);
        len
    }

    // ── Report sender (port of uart_send_report, rf.c:582-602) ────────
    //      L2, L6, K1, K2 fixes applied

    /// Build a report frame. Called by wireless::report and periodic sender.
    /// Port of uart_send_report — single transmission (L6), no trailing wait (K2),
    /// inline wakeup check (K1), no rf_state guard (L2).
    pub fn build_report(&mut self, report_type: u8, report_buf: &[u8], report_size: usize) -> usize {
        self.tx_buf = [0; UART_MAX_LEN];
        self.tx_buf[0] = UART_HEAD;
        self.tx_buf[1] = report_type;
        self.tx_buf[2] = 0x01; // ack
        self.tx_buf[3] = report_size as u8;
        self.tx_buf[4..4 + report_size].copy_from_slice(report_buf);
        self.tx_buf[4 + report_size] = Self::checksum(report_buf);
        report_size + 5
    }

    // ── NKRO auto-send (port of uart_auto_nkey_send, rf.c:55-111) ─────

    /// Diff-based NKRO report. Detects changed keys, updates byte/bit buffers,
    /// returns true if a report should be sent.
    pub fn auto_nkey_send(&mut self, now_bit_report: &[u8; 32]) -> bool {
        let mut f_byte_send = false;
        let mut f_bit_send = false;

        // Check modifier byte
        if self.bitkb_report_buf[0] ^ now_bit_report[0] != 0 {
            self.bytekb_report_buf[0] = now_bit_report[0];
            f_byte_send = true;
        }

        let mut key_code: u8 = 0;
        for i in 1..32 {
            let change_mask = self.bitkb_report_buf[i] ^ now_bit_report[i];
            let mut offset_mask: u8 = 1;
            for _j in 0..8 {
                if change_mask & offset_mask != 0 {
                    if now_bit_report[i] & offset_mask != 0 {
                        // Key pressed — try to fit in byte report
                        let mut placed = false;
                        for bi in 2..8 {
                            if self.bytekb_report_buf[bi] == 0 {
                                self.bytekb_report_buf[bi] = key_code;
                                f_byte_send = true;
                                placed = true;
                                break;
                            }
                        }
                        if !placed {
                            self.uart_bit_report_buf[i] |= offset_mask;
                            f_bit_send = true;
                        }
                    } else {
                        // Key released — remove from byte report
                        let mut removed = false;
                        for bi in 2..8 {
                            if self.bytekb_report_buf[bi] == key_code {
                                self.bytekb_report_buf[bi] = 0;
                                f_byte_send = true;
                                removed = true;
                                break;
                            }
                        }
                        if !removed {
                            self.uart_bit_report_buf[i] &= !offset_mask;
                            f_bit_send = true;
                        }
                    }
                }
                key_code = key_code.wrapping_add(1);
                offset_mask <<= 1;
            }
        }

        self.f_bit_kb_act = f_bit_send;
        f_bit_send || f_byte_send
    }

    // ── Frame parser (port of RF_Protocol_Receive, rf.c:190-291) ──────

    /// Parse a received frame. Returns the command byte and data slice if valid.
    /// Port of RF_Protocol_Receive + uart_receive_pro.
    pub fn parse_frame(&mut self) -> Option<(u8, &[u8])> {
        if self.rx_state != RxState::Done {
            return None;
        }
        self.f_uart_ack = true;
        self.sync_lost = 0;

        if self.rx_len > 4 {
            // Validate length
            let data_len = self.rx_buf[3] as usize;
            if (self.rx_len as usize) - 5 != data_len {
                self.rx_state = RxState::FormatErr;
                self.rx_len = 0;
                return None;
            }
            // Validate checksum
            let data = &self.rx_buf[4..4 + data_len];
            let expected_csum = &self.rx_buf[4 + data_len];
            if Self::checksum(data) != *expected_csum {
                self.rx_state = RxState::SumErr;
                self.rx_len = 0;
                return None;
            }
            self.rx_state = RxState::Idle;
            self.rx_len = 0;
            Some((self.rx_buf[1], data))
        } else if self.rx_len == 3 && self.rx_buf[2] == 0xA0 {
            // Simple ACK frame
            self.rx_state = RxState::Idle;
            self.rx_len = 0;
            None
        } else {
            self.rx_state = RxState::Idle;
            self.rx_len = 0;
            None
        }
    }

    /// Dispatch a parsed frame to update device info (port of RF_Protocol_Receive switch)
    pub fn dispatch_frame(&mut self, cmd: u8, data: &[u8]) {
        match cmd {
            CMD_HAND => self.f_rf_hand_ok = true,
            CMD_24G_SUSPEND => self.f_goto_sleep = true,
            CMD_NEW_ADV => self.f_rf_new_adv_ok = true,
            CMD_RF_STS_SYSC => {
                if data.len() >= 5 && self.link_mode as u8 == data[0] {
                    self.error_cnt = 0;
                    self.rf_state = RfState::from_u8(data[1]);
                    if self.rf_state == RfState::Connect && (data[2] & 0xF8) == 0 {
                        self.rf_led = data[2];
                    }
                    self.rf_charge = data[3];
                    if data[4] <= 100 {
                        self.rf_battery = data[4];
                    }
                    // B5 fix: don't force battery to 100 when charging
                } else {
                    // Link mode mismatch — after 5 errors, trigger re-sync
                    if self.rf_state != RfState::Invalid {
                        if self.error_cnt >= 5 {
                            self.error_cnt = 0;
                            self.f_send_channel = true;
                        } else {
                            self.error_cnt += 1;
                        }
                    }
                }
                self.f_rf_sts_sysc_ok = true;
            }
            CMD_READ_DATA => {
                // C firmware: memcpy(func_tab, &RXDBuf[4], 32) then reads func_tab[4..6]
                if data.len() >= 7 {
                    if data[4] <= LinkMode::Usb as u8 {
                        self.link_mode = LinkMode::from_u8(data[4]);
                    }
                    if data[5] < LinkMode::Usb as u8 {
                        self.rf_channel = data[5];
                    }
                    if data[6] >= LinkMode::Bt1 as u8 && data[6] <= LinkMode::Bt3 as u8 {
                        self.ble_channel = data[6];
                    }
                }
                self.f_rf_read_data_ok = true;
            }
            _ => {}
        }
    }

    /// Queue received byte into rx_buf (port of uart_receive_pro inner loop)
    pub fn rx_queue_byte(&mut self, byte: u8) {
        if (self.rx_len as usize) < UART_MAX_LEN {
            self.rx_buf[self.rx_len as usize] = byte;
            self.rx_len += 1;
        }
    }

    /// Mark frame as complete and parse (called after RX pause)
    pub fn rx_finish(&mut self) -> Option<(u8, [u8; 32], u8)> {
        self.rx_state = RxState::Done;
        let cmd: u8;
        let mut data_copy: [u8; 32] = [0; 32];
        let data_len: u8;
        if let Some((c, d)) = self.parse_frame() {
            cmd = c;
            data_len = d.len().min(32) as u8;
            data_copy[..data_len as usize].copy_from_slice(&d[..data_len as usize]);
        } else {
            return None;
        }
        self.dispatch_frame(cmd, &data_copy[..data_len as usize]);
        Some((cmd, data_copy, data_len))
    }
}
