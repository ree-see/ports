//! Docker container integration for mapping ports to containers.
//!
//! Uses the bollard API instead of spawning `docker ps`, with a 3-second TTL
//! cache to avoid repeated subprocess overhead in watch/top mode.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;

/// Container information from the Docker daemon.
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

type PortCache = Option<(Instant, HashMap<u16, ContainerInfo>)>;

// Global cache: (last_refresh, port_mappings)
static DOCKER_CACHE: LazyLock<Mutex<PortCache>> = LazyLock::new(|| Mutex::new(None));

const CACHE_TTL: Duration = Duration::from_secs(3);

/// Get a mapping of host ports to container information.
///
/// Results are cached for up to 3 seconds to avoid overhead in hot loops.
pub fn get_port_mappings() -> HashMap<u16, ContainerInfo> {
    let mut cache = DOCKER_CACHE.lock().unwrap();

    if let Some((last, ref data)) = *cache {
        if last.elapsed() < CACHE_TTL {
            return data.clone();
        }
    }

    let fresh = tokio::runtime::Runtime::new()
        .ok()
        .and_then(|rt| rt.block_on(fetch_from_bollard()).ok())
        .unwrap_or_default();

    *cache = Some((Instant::now(), fresh.clone()));
    fresh
}

async fn fetch_from_bollard() -> Result<HashMap<u16, ContainerInfo>> {
    use bollard::container::ListContainersOptions;
    use bollard::models::PortTypeEnum;

    let docker = bollard::Docker::connect_with_local_defaults()?;
    let options = ListContainersOptions::<String> {
        all: false,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;
    let mut mappings: HashMap<u16, ContainerInfo> = HashMap::new();

    for container in containers {
        let id = container
            .id
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(12)
            .collect::<String>();

        let name = container
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();

        let mut port_pairs: Vec<(u16, u16)> = Vec::new();

        if let Some(ports) = container.ports {
            for p in &ports {
                if p.typ != Some(PortTypeEnum::TCP) && p.typ != Some(PortTypeEnum::UDP) {
                    continue;
                }
                if let (Some(host_port), Some(private_port)) = (p.public_port, Some(p.private_port)) {
                    port_pairs.push((host_port, private_port));
                }
            }
        }

        let info = ContainerInfo {
            id,
            name,
            ports: port_pairs.clone(),
        };

        for (host_port, _) in port_pairs {
            mappings.insert(host_port, info.clone());
        }
    }

    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_info_display_name() {
        let info = ContainerInfo {
            id: "abc123".to_string(),
            name: "my-container".to_string(),
            ports: vec![(3000, 80)],
        };
        assert_eq!(info.display_name(), "my-container");
    }
}
