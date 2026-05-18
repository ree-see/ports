//! Core data types for port information.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use anyhow::Result;
use regex::Regex;
use serde::Serialize;

use crate::cli::{ProtocolFilter, SortField};
#[cfg(feature = "docker")]
use crate::docker;

/// Reachability of the Docker daemon for the most recent enrichment pass.
///
/// Surfaced in JSON output (two flat fields: `docker_status` and
/// `docker_reason`) and used by table output to decide whether to print
/// a stderr warning. `NotQueried` means no `docker-proxy` process was
/// observed and the daemon was never contacted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// With `docker` off the only construction site is the NotQueried
// default — Ok and Unreachable are dormant but kept in the type so JSON
// shape and downstream consumers stay stable across feature configs.
#[cfg_attr(not(feature = "docker"), allow(dead_code))]
pub enum DockerStatus {
    #[default]
    NotQueried,
    Ok,
    Unreachable {
        reason: String,
    },
}

impl DockerStatus {
    /// Tag string used in JSON output's `docker_status` field.
    pub fn as_tag(&self) -> &'static str {
        match self {
            DockerStatus::NotQueried => "not_queried",
            DockerStatus::Ok => "ok",
            DockerStatus::Unreachable { .. } => "unreachable",
        }
    }

    /// Reason string for the JSON `docker_reason` field; `None` for
    /// `Ok`/`NotQueried` so the field serializes as `null`.
    pub fn reason(&self) -> Option<&str> {
        match self {
            DockerStatus::Unreachable { reason } => Some(reason.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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
    /// Full command line of the process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_line: Option<String>,
    /// Working directory of the process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Detected framework or runtime (e.g. "Next.js", "Django").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
}

// Manual Hash/Eq excludes command_line and cwd so that watch
// mode does not flag a CWD change as a "new" port.
impl PartialEq for PortInfo {
    fn eq(&self, other: &Self) -> bool {
        self.port == other.port
            && self.protocol == other.protocol
            && self.pid == other.pid
            && self.process_name == other.process_name
            && self.address == other.address
            && self.remote_address == other.remote_address
            && self.container == other.container
            && self.service_name == other.service_name
    }
}

impl Eq for PortInfo {}

impl Hash for PortInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.port.hash(state);
        self.protocol.hash(state);
        self.pid.hash(state);
        self.process_name.hash(state);
        self.address.hash(state);
        self.remote_address.hash(state);
        self.container.hash(state);
        self.service_name.hash(state);
    }
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
    pub fn filter_by_query(
        ports: Vec<PortInfo>,
        query: &str,
        use_regex: bool,
    ) -> Result<Vec<PortInfo>> {
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
                        || p.framework
                            .as_ref()
                            .map(|f| re.is_match(f))
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
                        || p.framework
                            .as_ref()
                            .map(|f| f.to_lowercase().contains(&query_lower))
                            .unwrap_or(false)
                })
                .collect())
        }
    }

    pub fn filter_protocol(ports: Vec<PortInfo>, filter: Option<ProtocolFilter>) -> Vec<PortInfo> {
        match filter {
            None => ports,
            Some(ProtocolFilter::Tcp) => ports
                .into_iter()
                .filter(|p| p.protocol == Protocol::Tcp)
                .collect(),
            Some(ProtocolFilter::Udp) => ports
                .into_iter()
                .filter(|p| p.protocol == Protocol::Udp)
                .collect(),
        }
    }

    /// Enrich ports with Docker container information, returning the
    /// daemon reachability status alongside the populated ports.
    ///
    /// For ports forwarded by `docker-proxy`, adds the container name.
    /// Returns `DockerStatus::NotQueried` when no `docker-proxy` is
    /// observed (no daemon contact attempted), `Ok` on a successful
    /// fetch, or `Unreachable { reason }` when the daemon could not
    /// be reached.
    #[cfg(feature = "docker")]
    pub fn enrich_with_docker(ports: Vec<PortInfo>) -> (Vec<PortInfo>, DockerStatus) {
        let has_docker_proxy = ports
            .iter()
            .any(|p| p.process_name.contains("docker-proxy"));
        if !has_docker_proxy {
            return (ports, DockerStatus::NotQueried);
        }

        let (mappings, status) = docker::get_port_mappings();

        let enriched = ports
            .into_iter()
            .map(|mut p| {
                if p.process_name.contains("docker-proxy") {
                    if let Some(container) = mappings.get(&p.port) {
                        p.container = Some(container.name.clone());
                    }
                }
                p
            })
            .collect();

        (enriched, status)
    }

    /// No-op when the `docker` feature is disabled.
    #[cfg(not(feature = "docker"))]
    pub fn enrich_with_docker(ports: Vec<PortInfo>) -> (Vec<PortInfo>, DockerStatus) {
        (ports, DockerStatus::NotQueried)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn docker_status_default_is_not_queried() {
        assert_eq!(DockerStatus::default(), DockerStatus::NotQueried);
    }

    #[test]
    fn docker_status_as_tag() {
        assert_eq!(DockerStatus::NotQueried.as_tag(), "not_queried");
        assert_eq!(DockerStatus::Ok.as_tag(), "ok");
        assert_eq!(
            DockerStatus::Unreachable { reason: "x".into() }.as_tag(),
            "unreachable"
        );
    }

    #[test]
    fn docker_status_reason() {
        assert_eq!(DockerStatus::NotQueried.reason(), None);
        assert_eq!(DockerStatus::Ok.reason(), None);
        assert_eq!(
            DockerStatus::Unreachable {
                reason: "boom".into()
            }
            .reason(),
            Some("boom"),
        );
    }

    fn make_port_info() -> PortInfo {
        PortInfo {
            port: 8080,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "node".to_string(),
            address: "127.0.0.1:8080".to_string(),
            remote_address: None,
            container: None,
            service_name: None,
            command_line: None,
            cwd: None,
            framework: None,
        }
    }

    #[test]
    fn eq_ignores_framework() {
        let a = make_port_info();
        let mut b = make_port_info();
        b.framework = Some("Next.js".into());
        assert_eq!(a, b);
    }

    #[test]
    fn hash_ignores_framework() {
        let a = make_port_info();
        let mut b = make_port_info();
        b.framework = Some("Next.js".into());

        let mut set = HashSet::new();
        set.insert(a);
        assert!(
            set.contains(&b),
            "HashSet should treat entries differing \
             only in framework as identical"
        );
    }

    #[test]
    fn eq_ignores_command_line_and_cwd() {
        let a = make_port_info();
        let mut b = make_port_info();
        b.command_line = Some("/usr/bin/node server.js".into());
        b.cwd = Some(PathBuf::from("/home/user/project"));

        assert_eq!(a, b);
    }

    #[test]
    fn hash_ignores_command_line_and_cwd() {
        let a = make_port_info();
        let mut b = make_port_info();
        b.command_line = Some("/usr/bin/node server.js".into());
        b.cwd = Some(PathBuf::from("/home/user/project"));

        let mut set = HashSet::new();
        set.insert(a);
        assert!(
            set.contains(&b),
            "HashSet should treat entries differing only \
             in command_line/cwd as identical"
        );
    }
}
