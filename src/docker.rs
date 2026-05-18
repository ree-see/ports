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
    pub name: String,
    pub image: Option<String>,
}

type PortCache = Option<(Instant, HashMap<u16, ContainerInfo>)>;

// Global cache: (last_refresh, port_mappings)
pub(crate) static DOCKER_CACHE: LazyLock<Mutex<PortCache>> = LazyLock::new(|| Mutex::new(None));

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

/// Look up the Docker image name for a container on a given port.
///
/// Reads from the cached port mappings (does not trigger a refresh).
pub fn get_container_image(port: u16) -> Option<String> {
    let cache = DOCKER_CACHE.lock().unwrap();
    cache
        .as_ref()
        .and_then(|(_, data)| data.get(&port))
        .and_then(|info| info.image.clone())
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
        let name = container
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();

        let image = container.image.clone();

        let info = ContainerInfo { name, image };

        if let Some(ports) = container.ports {
            for p in &ports {
                if p.typ != Some(PortTypeEnum::TCP) && p.typ != Some(PortTypeEnum::UDP) {
                    continue;
                }
                if let Some(host_port) = p.public_port {
                    mappings.insert(host_port, info.clone());
                }
            }
        }
    }

    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_container_image_from_cache() {
        // Populate the cache manually for testing.
        let mut map = HashMap::new();
        map.insert(
            8080,
            ContainerInfo {
                name: "web".to_string(),
                image: Some("postgres:16".to_string()),
            },
        );
        *DOCKER_CACHE.lock().unwrap() = Some((Instant::now(), map));

        assert_eq!(get_container_image(8080), Some("postgres:16".to_string()));
        assert_eq!(get_container_image(9999), None);

        // Clean up.
        *DOCKER_CACHE.lock().unwrap() = None;
    }
}
