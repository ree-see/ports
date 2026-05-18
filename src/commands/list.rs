use anyhow::Result;

use crate::ancestry;
use crate::cli::{ProtocolFilter, SortField};
use crate::filter;
use crate::output::{json, table};
use crate::platform;
use crate::types::PortInfo;

pub fn execute(
    output_json: bool,
    connections: bool,
    sort: Option<SortField>,
    protocol: Option<ProtocolFilter>,
    why: bool,
    dev: bool,
) -> Result<()> {
    let listing = if connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };
    let docker_status = listing.docker_status;
    let mut ports = PortInfo::filter_protocol(listing.ports, protocol);
    if dev {
        filter::retain_dev_only(&mut ports);
    }
    PortInfo::sort_vec(&mut ports, sort);

    if why {
        let pids_with_names: Vec<(u32, &str)> = ports
            .iter()
            .map(|p| (p.pid, p.process_name.as_str()))
            .collect();
        let ancestry_map = ancestry::get_ancestry_batch(&pids_with_names);
        if output_json {
            json::print_ports_why(&ports, &ancestry_map, &docker_status);
        } else {
            table::print_warning(&docker_status);
            table::print_ports_why(&ports, &ancestry_map);
        }
    } else if output_json {
        json::print_ports(&ports, &docker_status);
    } else {
        table::print_warning(&docker_status);
        table::print_ports(&ports);
    }

    Ok(())
}
