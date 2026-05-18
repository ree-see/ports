//! Platform-specific port enumeration.
//!
//! Uses native `/proc/net` parsing on Linux, `lsof` on macOS.

use anyhow::Result;

use crate::framework;
use crate::types::{DockerStatus, PortInfo};

#[cfg(any(target_os = "linux", test))]
pub mod linux;

#[cfg(any(target_os = "macos", test))]
pub mod macos;

mod fallback;

/// A port enumeration result, including how reachable the Docker
/// daemon was when enriching container names.
pub struct PortListing {
    pub ports: Vec<PortInfo>,
    pub docker_status: DockerStatus,
}

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

/// Run the full enrichment pipeline on a raw port vec.
///
/// Order is intentional: well-known service names first (cheap), then
/// per-process details (PID-fanout), then Docker container names (cache
/// hit fast, miss slow), then framework detection (consumes everything
/// upstream). Docker is the only step that yields a status worth
/// surfacing — the rest can't fail in a way users need to know about.
fn enrich(ports: Vec<PortInfo>) -> PortListing {
    let ports = resolve_services(ports);
    let ports = enrich_process_details(ports);
    let (ports, docker_status) = PortInfo::enrich_with_docker(ports);
    let ports = framework::resolve_frameworks(ports);
    PortListing {
        ports,
        docker_status,
    }
}

#[cfg(target_os = "linux")]
pub fn get_listening_ports() -> Result<PortListing> {
    linux::get_listening_ports().map(enrich)
}

#[cfg(not(target_os = "linux"))]
pub fn get_listening_ports() -> Result<PortListing> {
    fallback::get_listening_ports().map(enrich)
}

#[cfg(target_os = "linux")]
pub fn get_connections() -> Result<PortListing> {
    linux::get_established_connections().map(enrich)
}

#[cfg(target_os = "macos")]
pub fn get_connections() -> Result<PortListing> {
    macos::get_connections().map(enrich)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn get_connections() -> Result<PortListing> {
    anyhow::bail!("--connections is only supported on Linux and macOS")
}
