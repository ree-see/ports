use anyhow::Result;

use crate::output::{json, table};
use crate::platform;

pub fn execute(output_json: bool) -> Result<()> {
    let ports = platform::get_listening_ports()?;

    if output_json {
        json::print_ports(&ports);
    } else {
        table::print_ports(&ports);
    }

    Ok(())
}
