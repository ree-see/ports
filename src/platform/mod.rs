//! Platform-specific port enumeration.
//!
//! Uses native `/proc/net` parsing on Linux, `lsof` on macOS.

use anyhow::Result;

use crate::types::PortInfo;

#[cfg(any(target_os = "linux", test))]
pub mod linux;

#[cfg(any(target_os = "macos", test))]
pub mod macos;

mod fallback;

#[cfg(target_os = "linux")]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    linux::get_listening_ports()
}

#[cfg(not(target_os = "linux"))]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    fallback::get_listening_ports()
}

#[cfg(target_os = "linux")]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    linux::get_established_connections()
}

#[cfg(target_os = "macos")]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    macos::get_connections()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    anyhow::bail!("--connections is only supported on Linux and macOS")
}
