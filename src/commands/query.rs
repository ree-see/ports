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
) -> Result<()> {
    let ports = if connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    let ports = PortInfo::filter_protocol(ports, protocol);

    let mut filtered: Vec<_> = if let Ok(port_num) = query.parse::<u16>() {
        ports.into_iter().filter(|p| p.port == port_num).collect()
    } else {
        ports
            .into_iter()
            .filter(|p| {
                p.process_name
                    .to_lowercase()
                    .contains(&query.to_lowercase())
            })
            .collect()
    };

    PortInfo::sort_vec(&mut filtered, sort);

    if output_json {
        json::print_ports(&filtered);
    } else {
        table::print_ports(&filtered);
    }

    Ok(())
}
