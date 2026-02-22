use anyhow::Result;

use crate::cli::{ProtocolFilter, SortField};
use crate::output::{json, table};
use crate::platform;
use crate::types::PortInfo;

pub fn execute(
    query: &str,
    output_json: bool,
    connections: bool,
    sort: Option<SortField>,
    protocol: Option<ProtocolFilter>,
    use_regex: bool,
) -> Result<()> {
    let ports = if connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    let ports = PortInfo::filter_protocol(ports, protocol);
    // Enrich docker-proxy entries with container names
    let ports = PortInfo::enrich_with_docker(ports);

    let mut filtered = PortInfo::filter_by_query(ports, query, use_regex)?;

    PortInfo::sort_vec(&mut filtered, sort);

    if output_json {
        json::print_ports(&filtered);
    } else {
        table::print_ports(&filtered);
    }

    Ok(())
}
