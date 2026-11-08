//! Hardware seam between the C-equivalent keyboard logic (`kb`, `wireless`,
//! `led::side`) and the STM32 board. Every method stands in for one QMK /
//! ChibiOS call used by the C firmware, so the logic above it can be ported
//! line by line and exercised on the host with a mock.

/// QMK `enum usb_device_state` (tmk_core/protocol/usb_device_state.h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbDeviceState {
    NoInit,
    Init,
    Configured,
    Suspend,
}

pub trait Hal {
    /// `timer_read32()`: milliseconds since boot.
    fn now(&self) -> u32;
    /// `wait_ms()`. The board implementation keeps servicing USB, as the
    /// ChibiOS USB stack does from its interrupt while C blocks.
    fn wait_ms(&mut self, ms: u32);

    // ── NRF52 UART (rf.c) ────────────────────────────────────────────
    /// `UART_Send_Bytes()`: wakeup pin low, 50us, transmit, 50us + 24us/byte, pin high.
    fn uart_send_bytes(&mut self, buf: &[u8]);
    /// `uart_available()` + `uart_read()`.
    fn uart_read(&mut self) -> Option<u8>;
    /// `writePin(NRF_RESET_PIN, high)`.
    fn nrf_reset_pin(&mut self, high: bool);

    // ── Board pins ───────────────────────────────────────────────────
    /// `writePin(DC_BOOST_PIN / RGB_DRIVER_SDB1 / RGB_DRIVER_SDB2, on)`.
    fn led_power(&mut self, on: bool);
    /// `readPin(DEV_MODE_PIN)`: high = wired (USB) position.
    fn read_dev_mode_pin(&mut self) -> bool;
    /// `readPin(SYS_MODE_PIN)`: high = Mac position.
    fn read_sys_mode_pin(&mut self) -> bool;

    // ── USB host driver (chibios_driver) ─────────────────────────────
    fn usb_state(&self) -> UsbDeviceState;
    /// `USB_DRIVER.state == USB_SUSPENDED`.
    fn usb_suspended(&self) -> bool;
    /// `keyboard_led_state` as set by the host.
    fn usb_keyboard_leds(&self) -> u8;
    /// `send_nkro()`: mods + 30-byte usage bitmap.
    fn usb_send_nkro(&mut self, mods: u8, bits: &[u8; 30]);
    /// `send_keyboard()`: 8-byte boot report.
    fn usb_send_keyboard(&mut self, report: &[u8; 8]);
    /// `send_mouse()`: buttons, x, y, wheel, pan.
    fn usb_send_mouse(&mut self, report: &[u8; 5]);
    fn usb_send_consumer(&mut self, usage: u16);
    fn usb_send_system(&mut self, usage: u16);
    /// `USB_DRIVER.status & USB_GETSTATUS_REMOTE_WAKEUP_ENABLED`.
    fn usb_remote_wakeup_enabled(&self) -> bool;
    /// `usbWakeupHost()`.
    fn usb_wakeup_host(&mut self);

    // ── RGB matrix driver ────────────────────────────────────────────
    /// `rgb_matrix_update_pwm_buffers()`: flush the whole LED buffer now.
    fn rgb_update_pwm_buffers(&mut self, rgb: &mut crate::led::rgb::RgbMatrix);

    // ── eeconfig ─────────────────────────────────────────────────────
    /// `eeconfig_update_user_datablock()` plus the RGB matrix config the
    /// Rust port stores in the same flash page.
    fn eeconfig_update(&mut self, cfg: &crate::config::eeprom::UserConfig);
    /// `eeconfig_init()`: invalidate every stored setting (keymaps included).
    fn eeconfig_init(&mut self);
    /// `eeconfig_read_keymap()`: keymap_config.raw (0x1400 when never written).
    fn eeconfig_read_keymap(&self) -> u16;
    /// `eeconfig_update_keymap()`.
    fn eeconfig_update_keymap(&mut self, raw: u16);

    // ── MCU ──────────────────────────────────────────────────────────
    /// `bootloader_jump()`: enter the STM32 ROM DFU bootloader.
    fn bootloader_jump(&mut self);
    /// `mcu_reset()`: NVIC system reset.
    fn mcu_reset(&mut self);
}
