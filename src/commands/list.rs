use anyhow::Result;

use crate::output::{json, table};
use crate::platform;

pub fn execute(output_json: bool, connections: bool) -> Result<()> {
    let ports = if connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    if output_json {
        json::print_ports(&ports);
    } else {
        table::print_ports(&ports);
    }

    Ok(())
}
