use anyhow::Result;

use crate::types::PortInfo;

#[cfg(any(target_os = "linux", test))]
pub mod linux;

mod fallback;

#[cfg(target_os = "linux")]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    linux::get_listening_ports()
}

#[cfg(not(target_os = "linux"))]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    fallback::get_listening_ports()
}
