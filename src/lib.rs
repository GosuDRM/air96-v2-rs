#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod keyboard;
pub mod wireless;
pub mod led;
pub mod usb_hid;
pub mod via;

#[cfg(test)]
mod test;
