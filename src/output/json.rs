use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};

use crate::ancestry::ProcessAncestry;
use crate::types::{DockerStatus, PortInfo};

pub fn print_ports(ports: &[PortInfo], docker_status: &DockerStatus) {
    let output = wrap(ports_to_values(ports), docker_status);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("Failed to serialize to JSON")
    );
}

/// Print ports as JSON with ancestry data merged in.
pub fn print_ports_why(
    ports: &[PortInfo],
    ancestry_map: &HashMap<u32, ProcessAncestry>,
    docker_status: &DockerStatus,
) {
    #[derive(Serialize)]
    struct PortWithAncestry<'a> {
        #[serde(flatten)]
        port: &'a PortInfo,
        #[serde(skip_serializing_if = "Option::is_none")]
        ancestry: Option<&'a ProcessAncestry>,
    }

    let enriched: Vec<Value> = ports
        .iter()
        .map(|p| {
            serde_json::to_value(PortWithAncestry {
                port: p,
                ancestry: ancestry_map.get(&p.pid),
            })
            .expect("Failed to serialize port+ancestry to JSON")
        })
        .collect();

    let output = wrap(enriched, docker_status);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("Failed to serialize to JSON")
    );
}

fn ports_to_values(ports: &[PortInfo]) -> Vec<Value> {
    ports
        .iter()
        .map(|p| serde_json::to_value(p).expect("Failed to serialize port to JSON"))
        .collect()
}

/// Wrap the per-port array in the top-level object that carries docker
/// reachability. Two flat fields (`docker_status`, `docker_reason`)
/// rather than a nested enum so `jq '.docker_status'` is one hop.
fn wrap(ports: Vec<Value>, status: &DockerStatus) -> Value {
    json!({
        "ports": ports,
        "docker_status": status.as_tag(),
        "docker_reason": status.reason(),
    })
}
