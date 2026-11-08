#![cfg_attr(not(test), no_std)]

pub mod action;
pub mod config;
pub mod hal;
pub mod kb;
pub mod keyboard;
pub mod wireless;
pub mod led;
pub mod usb_hid;
pub mod via;

#[cfg(test)]
mod test;
#[cfg(test)]
mod usb_test;
#[cfg(test)]
mod action_test;
