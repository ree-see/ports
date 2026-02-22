//! Core data types for port information.

use std::fmt;

use anyhow::Result;
use regex::Regex;
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
    /// Well-known service name for this port (e.g. "http", "ssh").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
}

static WELL_KNOWN_PORTS: &[(u16, &str)] = &[
    (21, "ftp"),
    (22, "ssh"),
    (25, "smtp"),
    (53, "dns"),
    (80, "http"),
    (110, "pop3"),
    (143, "imap"),
    (443, "https"),
    (465, "smtps"),
    (587, "submission"),
    (993, "imaps"),
    (995, "pop3s"),
    (1433, "mssql"),
    (3306, "mysql"),
    (3389, "rdp"),
    (5432, "postgres"),
    (5672, "amqp"),
    (6379, "redis"),
    (8080, "http-alt"),
    (8443, "https-alt"),
    (9200, "elasticsearch"),
    (27017, "mongodb"),
];

impl PortInfo {
    /// Populate the `service_name` field from the well-known port table.
    pub fn resolve_service_name(&mut self) {
        self.service_name = WELL_KNOWN_PORTS
            .iter()
            .find(|(p, _)| *p == self.port)
            .map(|(_, name)| name.to_string());
    }

    pub fn sort_vec(ports: &mut [PortInfo], sort: Option<SortField>) {
        match sort {
            Some(SortField::Port) => ports.sort_by_key(|p| p.port),
            Some(SortField::Pid) => ports.sort_by_key(|p| p.pid),
            Some(SortField::Name) => ports.sort_by(|a, b| a.process_name.cmp(&b.process_name)),
            None => {}
        }
    }

    /// Filter ports by port number or process/container name query.
    ///
    /// When `use_regex` is true, the query is compiled as a regex. Returns an
    /// `Err` with a clear message if the regex is invalid.
    pub fn filter_by_query(ports: Vec<PortInfo>, query: &str, use_regex: bool) -> Result<Vec<PortInfo>> {
        if use_regex {
            let re = Regex::new(query)
                .map_err(|e| anyhow::anyhow!("Invalid regex '{}': {}", query, e))?;
            return Ok(ports
                .into_iter()
                .filter(|p| {
                    re.is_match(&p.process_name)
                        || p.container
                            .as_ref()
                            .map(|c| re.is_match(c))
                            .unwrap_or(false)
                })
                .collect());
        }

        let query_lower = query.to_lowercase();
        if let Ok(port_num) = query.parse::<u16>() {
            Ok(ports.into_iter().filter(|p| p.port == port_num).collect())
        } else {
            Ok(ports
                .into_iter()
                .filter(|p| {
                    p.process_name.to_lowercase().contains(&query_lower)
                        || p.container
                            .as_ref()
                            .map(|c| c.to_lowercase().contains(&query_lower))
                            .unwrap_or(false)
                })
                .collect())
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
