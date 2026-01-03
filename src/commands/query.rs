use anyhow::Result;

use crate::output::table;
use crate::platform;

pub fn execute(query: &str) -> Result<()> {
    let ports = platform::get_listening_ports()?;

    let filtered: Vec<_> = if let Ok(port_num) = query.parse::<u16>() {
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

    table::print_ports(&filtered);
    Ok(())
}
