use std::collections::HashMap;

use serde::Serialize;

use crate::ancestry::ProcessAncestry;
use crate::types::PortInfo;

pub fn print_ports(ports: &[PortInfo]) {
    let json = serde_json::to_string_pretty(ports).expect("Failed to serialize to JSON");
    println!("{json}");
}

/// Print ports as JSON with ancestry data merged in.
pub fn print_ports_why(ports: &[PortInfo], ancestry_map: &HashMap<u32, ProcessAncestry>) {
    #[derive(Serialize)]
    struct PortWithAncestry<'a> {
        #[serde(flatten)]
        port: &'a PortInfo,
        #[serde(skip_serializing_if = "Option::is_none")]
        ancestry: Option<&'a ProcessAncestry>,
    }

    let enriched: Vec<PortWithAncestry> = ports
        .iter()
        .map(|p| PortWithAncestry {
            port: p,
            ancestry: ancestry_map.get(&p.pid),
        })
        .collect();

    let json = serde_json::to_string_pretty(&enriched).expect("Failed to serialize to JSON");
    println!("{json}");
}
