use anyhow::Result;

use crate::cli::{ProtocolFilter, SortField};
use crate::output::{json, table};
use crate::platform;
use crate::types::PortInfo;

pub fn execute(
    output_json: bool,
    connections: bool,
    sort: Option<SortField>,
    protocol: Option<ProtocolFilter>,
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

    if output_json {
        json::print_ports(&ports);
    } else {
        table::print_ports(&ports);
    }

    Ok(())
}
