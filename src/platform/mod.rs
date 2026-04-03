//! Platform-specific port enumeration.
//!
//! Uses native `/proc/net` parsing on Linux, `lsof` on macOS.

use anyhow::Result;

use crate::framework;
use crate::types::PortInfo;

fn enrich_docker(ports: Vec<PortInfo>) -> Vec<PortInfo> {
    PortInfo::enrich_with_docker(ports)
}

fn detect_frameworks(ports: Vec<PortInfo>) -> Vec<PortInfo> {
    framework::resolve_frameworks(ports)
}

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

/// Populate `command_line` and `cwd` on each `PortInfo`.
///
/// Dispatches by target OS, not by which module produced the
/// ports. On macOS, listening ports come from the fallback
/// module but are still enriched by the macOS resolver.
fn enrich_process_details(mut ports: Vec<PortInfo>) -> Vec<PortInfo> {
    #[cfg(target_os = "linux")]
    linux::process::resolve_process_details(&mut ports);

    #[cfg(target_os = "macos")]
    macos::resolve_process_details(&mut ports);

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fallback::resolve_process_details(&mut ports);

    ports
}

#[cfg(target_os = "linux")]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    linux::get_listening_ports()
        .map(resolve_services)
        .map(enrich_process_details)
        .map(enrich_docker)
        .map(detect_frameworks)
}

#[cfg(not(target_os = "linux"))]
pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    fallback::get_listening_ports()
        .map(resolve_services)
        .map(enrich_process_details)
        .map(enrich_docker)
        .map(detect_frameworks)
}

#[cfg(target_os = "linux")]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    linux::get_established_connections()
        .map(resolve_services)
        .map(enrich_process_details)
        .map(enrich_docker)
        .map(detect_frameworks)
}

#[cfg(target_os = "macos")]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    macos::get_connections()
        .map(resolve_services)
        .map(enrich_process_details)
        .map(enrich_docker)
        .map(detect_frameworks)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn get_connections() -> Result<Vec<PortInfo>> {
    anyhow::bail!("--connections is only supported on Linux and macOS")
}
