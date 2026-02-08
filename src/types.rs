//! Core data types for port information.

use std::fmt;

use serde::Serialize;

use crate::cli::{ProtocolFilter, SortField};
use crate::docker;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct PortInfo {
    pub port: u16,
    pub protocol: Protocol,
    pub pid: u32,
    pub process_name: String,
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_address: Option<String>,
    /// Container name if this port is forwarded by Docker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

impl PortInfo {
    pub fn sort_vec(ports: &mut [PortInfo], sort: Option<SortField>) {
        match sort {
            Some(SortField::Port) => ports.sort_by_key(|p| p.port),
            Some(SortField::Pid) => ports.sort_by_key(|p| p.pid),
            Some(SortField::Name) => ports.sort_by(|a, b| a.process_name.cmp(&b.process_name)),
            None => {}
        }
    }

    pub fn filter_protocol(ports: Vec<PortInfo>, filter: Option<ProtocolFilter>) -> Vec<PortInfo> {
        match filter {
            None => ports,
            Some(ProtocolFilter::Tcp) => {
                ports.into_iter().filter(|p| p.protocol == Protocol::Tcp).collect()
            }
            Some(ProtocolFilter::Udp) => {
                ports.into_iter().filter(|p| p.protocol == Protocol::Udp).collect()
            }
        }
    }

    /// Enrich ports with Docker container information.
    /// 
    /// For ports forwarded by `docker-proxy`, adds the container name.
    pub fn enrich_with_docker(ports: Vec<PortInfo>) -> Vec<PortInfo> {
        // Only fetch Docker info if we have docker-proxy entries
        let has_docker_proxy = ports.iter().any(|p| p.process_name.contains("docker-proxy"));
        if !has_docker_proxy {
            return ports;
        }

        let mappings = docker::get_port_mappings();
        if mappings.is_empty() {
            return ports;
        }

        ports
            .into_iter()
            .map(|mut p| {
                if p.process_name.contains("docker-proxy") {
                    if let Some(container) = mappings.get(&p.port) {
                        p.container = Some(container.name.clone());
                    }
                }
                p
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "tcp"),
            Protocol::Udp => write!(f, "udp"),
        }
    }
}
