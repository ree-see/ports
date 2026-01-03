use anyhow::Result;

use crate::output::table;
use crate::platform;

pub fn execute() -> Result<()> {
    let ports = platform::get_listening_ports()?;
    table::print_ports(&ports);
    Ok(())
}
