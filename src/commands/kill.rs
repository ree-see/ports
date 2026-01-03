use anyhow::{bail, Result};

use crate::platform;

pub fn execute(target: &str) -> Result<()> {
    let ports = platform::get_listening_ports()?;

    let matches: Vec<_> = if let Ok(port_num) = target.parse::<u16>() {
        ports.into_iter().filter(|p| p.port == port_num).collect()
    } else {
        ports
            .into_iter()
            .filter(|p| {
                p.process_name
                    .to_lowercase()
                    .contains(&target.to_lowercase())
            })
            .collect()
    };

    if matches.is_empty() {
        bail!("No process found matching '{}'", target);
    }

    if matches.len() > 1 {
        eprintln!("Multiple processes found:");
        for p in &matches {
            eprintln!("  {} (PID {}) on port {}", p.process_name, p.pid, p.port);
        }
        bail!("Specify a more specific target");
    }

    let port_info = &matches[0];
    eprintln!(
        "Would kill {} (PID {}) on port {}",
        port_info.process_name, port_info.pid, port_info.port
    );
    eprintln!("Kill functionality not yet implemented");

    Ok(())
}
