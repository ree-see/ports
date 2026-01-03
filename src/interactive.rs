use anyhow::{bail, Result};
use dialoguer::{theme::ColorfulTheme, Select};

use crate::commands::kill::kill_process;
use crate::types::PortInfo;

pub fn select_and_kill(ports: &[PortInfo]) -> Result<()> {
    if ports.is_empty() {
        bail!("No ports to select from");
    }

    let items: Vec<String> = ports
        .iter()
        .map(|p| {
            format!(
                "{:>5} {:4} {:>6} {}",
                p.port, p.protocol, p.pid, p.process_name
            )
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a port to kill (↑/↓ or j/k to navigate, Enter to select, q to quit)")
        .items(&items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(idx) => {
            let port = &ports[idx];
            eprintln!(
                "Killing PID {} ({}) on port {}",
                port.pid, port.process_name, port.port
            );
            kill_process(port.pid)?;
            eprintln!("Killed PID {}", port.pid);
            Ok(())
        }
        None => {
            eprintln!("Cancelled");
            Ok(())
        }
    }
}
