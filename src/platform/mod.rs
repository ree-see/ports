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

fn resolve_services(mut ports: Vec<PortInfo>) -> Vec<PortInfo> {
    for p in &mut ports {
        p.resolve_service_name();
    }
    ports
}

#[cfg(target_os = "linux")]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    linux::get_listening_ports().map(resolve_services)
}

#[cfg(not(target_os = "linux"))]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    fallback::get_listening_ports().map(resolve_services)
}

#[cfg(target_os = "linux")]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    linux::get_established_connections().map(resolve_services)
}

#[cfg(target_os = "macos")]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    macos::get_connections().map(resolve_services)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    anyhow::bail!("--connections is only supported on Linux and macOS")
}
