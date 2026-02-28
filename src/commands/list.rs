use anyhow::Result;

use crate::ancestry;
use crate::cli::{ProtocolFilter, SortField};
use crate::output::{json, table};
use crate::platform;
use crate::types::PortInfo;

pub fn execute(
    output_json: bool,
    connections: bool,
    sort: Option<SortField>,
    protocol: Option<ProtocolFilter>,
    why: bool,
) -> Result<()> {
    let ports = if connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    let ports = PortInfo::filter_protocol(ports, protocol);
    // Enrich docker-proxy entries with container names
    let mut ports = PortInfo::enrich_with_docker(ports);
    PortInfo::sort_vec(&mut ports, sort);

    if why {
        let pids_with_names: Vec<(u32, &str)> = ports
            .iter()
            .map(|p| (p.pid, p.process_name.as_str()))
            .collect();
        let ancestry_map = ancestry::get_ancestry_batch(&pids_with_names);
        if output_json {
            json::print_ports_why(&ports, &ancestry_map);
        } else {
            table::print_ports_why(&ports, &ancestry_map);
        }
    } else if output_json {
        json::print_ports(&ports);
    } else {
        table::print_ports(&ports);
    }

    Ok(())
}
