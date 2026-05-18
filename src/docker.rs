//! Docker container integration for mapping ports to containers.
//!
//! Uses the bollard API instead of spawning `docker ps`, with a 3-second TTL
//! cache to avoid repeated subprocess overhead in watch/top mode. Failed
//! fetches are cached for 500ms (much shorter) so daemon recovery surfaces
//! on the next tick rather than after the full 3-second success window.

use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use regex::Regex;

use crate::types::DockerStatus;

/// Container information from the Docker daemon.
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub name: String,
    pub image: Option<String>,
}

type PortCache = Option<(Instant, HashMap<u16, ContainerInfo>, DockerStatus)>;

// Global cache: (last_refresh, port_mappings, last_status)
pub(crate) static DOCKER_CACHE: LazyLock<Mutex<PortCache>> = LazyLock::new(|| Mutex::new(None));

const CACHE_TTL: Duration = Duration::from_secs(3);
const FAILURE_CACHE_TTL: Duration = Duration::from_millis(500);

/// Get a mapping of host ports to container information, plus the
/// reachability status of the Docker daemon.
///
/// Results are cached: successful fetches for 3 seconds (avoid bollard
/// overhead in hot loops), failed fetches for 500ms (so a recovering
/// daemon is rediscovered on the next watch tick).
pub fn get_port_mappings() -> (HashMap<u16, ContainerInfo>, DockerStatus) {
    let mut cache = DOCKER_CACHE.lock().unwrap();

    if let Some((last, ref data, ref status)) = *cache {
        let ttl = match status {
            DockerStatus::Ok => CACHE_TTL,
            // Failures expire faster so daemon recovery is reflected
            // on the next tick. NotQueried shouldn't end up here, but
            // treat it as a successful "no work to do" cache entry.
            DockerStatus::Unreachable { .. } => FAILURE_CACHE_TTL,
            DockerStatus::NotQueried => CACHE_TTL,
        };
        if last.elapsed() < ttl {
            return (data.clone(), status.clone());
        }
    }

    let (fresh, status) = fetch_with_status();
    *cache = Some((Instant::now(), fresh.clone(), status.clone()));
    (fresh, status)
}

/// Look up the Docker image name for a container on a given port.
///
/// Reads from the cached port mappings (does not trigger a refresh).
pub fn get_container_image(port: u16) -> Option<String> {
    let cache = DOCKER_CACHE.lock().unwrap();
    cache
        .as_ref()
        .and_then(|(_, data, _)| data.get(&port))
        .and_then(|info| info.image.clone())
}

/// Run the bollard fetch and translate every failure mode into a
/// labelled `DockerStatus::Unreachable`.
///
/// Three distinct failure sites — tokio runtime creation, bollard
/// connect, list_containers query — each get a prefix so the user can
/// tell them apart from the stderr warning or JSON `docker_reason`.
fn fetch_with_status() -> (HashMap<u16, ContainerInfo>, DockerStatus) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => return (HashMap::new(), unreachable_from("tokio runtime", &e)),
    };

    let connect_result = rt.block_on(async {
        bollard::Docker::connect_with_local_defaults()
            .map_err(|e| unreachable_from("docker connect", &e))
    });
    let docker = match connect_result {
        Ok(d) => d,
        Err(status) => return (HashMap::new(), status),
    };

    match rt.block_on(fetch_from_bollard(&docker)) {
        Ok(map) => (map, DockerStatus::Ok),
        Err(e) => (HashMap::new(), unreachable_from("docker query", &e)),
    }
}

/// Build an `Unreachable` status with a labelled reason. All three
/// failure sites route through this so the `prefix: details` shape is
/// enforced by construction rather than relying on convention.
fn unreachable_from(prefix: &str, e: &impl fmt::Display) -> DockerStatus {
    DockerStatus::Unreachable {
        reason: format!("{prefix}: {}", redact(&format!("{e}"))),
    }
}

/// Strip credentials from URI-like patterns so a `DOCKER_HOST` with
/// embedded basic auth (e.g. `ssh://user:pass@host`) does not leak
/// into stderr or JSON output via the bollard error chain.
fn redact(s: &str) -> String {
    static URI_RE: LazyLock<Regex> = LazyLock::new(|| {
        // (?i) so DOCKER_HOST=TCP://... is matched the same as tcp://...
        // Password class allows `/` (RFC 3986 permits it in userinfo),
        // anchored by the required `@` so we can't over-match.
        Regex::new(r"(?i)((?:tcp|ssh|https?|unix)://)[^@\s]+:[^@\s]+@").unwrap()
    });
    URI_RE.replace_all(s, "${1}***@").to_string()
}

async fn fetch_from_bollard(docker: &bollard::Docker) -> Result<HashMap<u16, ContainerInfo>> {
    use bollard::container::ListContainersOptions;
    use bollard::models::PortTypeEnum;

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
    use anyhow::anyhow;

    // Cache-mutating tests share the global `DOCKER_CACHE`. They are
    // consolidated into one test body so cargo's per-mod parallel runner
    // can't interleave them — the same race the framework.rs tier0 test
    // ran into during v0.4.0 review.
    #[test]
    fn docker_cache_round_trip() {
        // Sub-case 1: get_container_image reads from the cache.
        let mut map = HashMap::new();
        map.insert(
            8080,
            ContainerInfo {
                name: "web".to_string(),
                image: Some("postgres:16".to_string()),
            },
        );
        *DOCKER_CACHE.lock().unwrap() = Some((Instant::now(), map, DockerStatus::Ok));
        assert_eq!(get_container_image(8080), Some("postgres:16".to_string()));
        assert_eq!(get_container_image(9999), None);

        // Sub-case 2: success-status cache is returned by get_port_mappings.
        let mut map = HashMap::new();
        map.insert(
            3001,
            ContainerInfo {
                name: "web".to_string(),
                image: Some("nginx:1".to_string()),
            },
        );
        *DOCKER_CACHE.lock().unwrap() = Some((Instant::now(), map, DockerStatus::Ok));
        let (returned, status) = get_port_mappings();
        assert_eq!(status, DockerStatus::Ok);
        assert_eq!(returned.len(), 1);
        assert_eq!(returned.get(&3001).unwrap().name, "web");

        // Sub-case 3: failure-status cache is returned without retry.
        let reason = "docker connect: boom".to_string();
        *DOCKER_CACHE.lock().unwrap() = Some((
            Instant::now(),
            HashMap::new(),
            DockerStatus::Unreachable {
                reason: reason.clone(),
            },
        ));
        let (returned, status) = get_port_mappings();
        assert!(returned.is_empty());
        assert_eq!(status, DockerStatus::Unreachable { reason });

        // Clean up.
        *DOCKER_CACHE.lock().unwrap() = None;
    }

    #[test]
    fn failure_cache_ttl_shorter_than_success() {
        assert!(FAILURE_CACHE_TTL < CACHE_TTL);
    }

    #[test]
    fn unreachable_from_attaches_prefix() {
        let e = anyhow!("simulated tokio failure");
        let status = unreachable_from("tokio runtime", &e);
        match status {
            DockerStatus::Unreachable { reason } => {
                assert!(
                    reason.starts_with("tokio runtime: "),
                    "expected prefix `tokio runtime: `, got: {reason}"
                );
                assert!(reason.contains("simulated tokio failure"));
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn redact_strips_basic_auth_uri() {
        let redacted = redact("connect to tcp://user:secret@host:2375 failed");
        assert!(redacted.contains("tcp://***@host:2375"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("user:secret"));
    }

    #[test]
    fn redact_strips_ssh_basic_auth() {
        let redacted = redact("dial ssh://alice:hunter2@example.com:22");
        assert!(redacted.contains("ssh://***@example.com:22"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("alice"));
    }

    #[test]
    fn redact_case_insensitive_scheme() {
        // DOCKER_HOST users can capitalize the scheme; redact must match.
        let redacted = redact("dial SSH://ALICE:HUNTER2@example.com");
        assert!(redacted.contains("SSH://***@example.com"));
        assert!(!redacted.contains("HUNTER2"));
    }

    #[test]
    fn redact_allows_slash_in_password() {
        // RFC 3986 permits `/` unencoded in userinfo. The earlier regex
        // had `[^@\s/]+` which broke on this — and leaked the credentials.
        let redacted = redact("dial ssh://user:pa/ss@host");
        assert!(redacted.contains("ssh://***@host"));
        assert!(!redacted.contains("pa/ss"));
    }

    #[test]
    fn redact_passthrough_no_uri() {
        assert_eq!(redact("plain error message"), "plain error message");
    }

    #[test]
    fn redact_passthrough_uri_without_credentials() {
        // No user:pass@ — nothing to redact.
        let s = "connect to tcp://host:2375 failed";
        assert_eq!(redact(s), s);
    }
}
