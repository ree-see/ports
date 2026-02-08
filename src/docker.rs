//! Docker container integration for mapping ports to containers.
//!
//! When a port is being forwarded by `docker-proxy`, this module can
//! determine which container the port is mapped to.

use std::collections::HashMap;
use std::process::Command;

use serde::Deserialize;

/// Container port mapping information.
#[derive(Debug, Clone, Deserialize)]
pub struct PortMapping {
    #[serde(rename = "HostPort")]
    pub host_port: String,
}

/// Container information from `docker ps`.
#[derive(Debug, Clone, Deserialize)]
struct ContainerJson {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Names")]
    names: String,
    #[serde(rename = "Ports")]
    ports: String,
}

/// Parsed container info with extracted port mappings.
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub ports: Vec<(u16, u16)>, // (host_port, container_port)
}

impl ContainerInfo {
    /// Get a display string for this container.
    pub fn display_name(&self) -> &str {
        &self.name
    }
}

/// Check if Docker is available on this system.
pub fn is_docker_available() -> bool {
    Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get a mapping of host ports to container information.
/// 
/// Returns a HashMap where keys are host ports and values are container info.
pub fn get_port_mappings() -> HashMap<u16, ContainerInfo> {
    let mut mappings = HashMap::new();

    // Try to get container info using docker ps with JSON format
    let output = match Command::new("docker")
        .args(["ps", "--format", "{{json .}}"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return mappings,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let container: ContainerJson = match serde_json::from_str(line) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse ports like "0.0.0.0:3000->80/tcp, :::3000->80/tcp"
        let port_pairs = parse_port_string(&container.ports);

        // Clean up container name (remove leading /)
        let name = container.names.trim_start_matches('/').to_string();

        let info = ContainerInfo {
            id: container.id.chars().take(12).collect(),
            name,
            ports: port_pairs.clone(),
        };

        for (host_port, _) in port_pairs {
            mappings.insert(host_port, info.clone());
        }
    }

    mappings
}

/// Parse Docker port string format: "0.0.0.0:3000->80/tcp, :::3000->80/tcp"
fn parse_port_string(ports: &str) -> Vec<(u16, u16)> {
    let mut result = Vec::new();

    for mapping in ports.split(',') {
        let mapping = mapping.trim();
        if mapping.is_empty() {
            continue;
        }

        // Match pattern: HOST:PORT->CONTAINER/PROTO
        // Examples: "0.0.0.0:3000->80/tcp", ":::3000->80/tcp"
        if let Some(arrow_pos) = mapping.find("->") {
            let host_part = &mapping[..arrow_pos];
            let container_part = &mapping[arrow_pos + 2..];

            // Extract host port (after last :)
            let host_port: u16 = host_part
                .rsplit(':')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            // Extract container port (before /)
            let container_port: u16 = container_part
                .split('/')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            if host_port > 0 && container_port > 0 {
                result.push((host_port, container_port));
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_port_string_single() {
        let ports = parse_port_string("0.0.0.0:3000->80/tcp");
        assert_eq!(ports, vec![(3000, 80)]);
    }

    #[test]
    fn test_parse_port_string_multiple() {
        let ports = parse_port_string("0.0.0.0:3000->80/tcp, :::3000->80/tcp");
        assert_eq!(ports, vec![(3000, 80), (3000, 80)]);
    }

    #[test]
    fn test_parse_port_string_ipv6() {
        let ports = parse_port_string(":::8080->8080/tcp");
        assert_eq!(ports, vec![(8080, 8080)]);
    }

    #[test]
    fn test_parse_port_string_empty() {
        let ports = parse_port_string("");
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_port_string_complex() {
        let ports = parse_port_string("0.0.0.0:443->443/tcp, 0.0.0.0:80->80/tcp, :::443->443/tcp");
        assert_eq!(ports.len(), 3);
        assert!(ports.contains(&(443, 443)));
        assert!(ports.contains(&(80, 80)));
    }
}
