use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::{bail, Context, Result};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use crate::platform;
use crate::types::PortInfo;

pub fn execute(target: &str, force: bool, all: bool, connections: bool) -> Result<()> {
    let mut ports = platform::get_listening_ports()?;
    if connections {
        ports.extend(platform::get_connections()?);
        // Deduplicate by PID+port to avoid double-reporting
        ports.sort_by_key(|p| (p.pid, p.port));
        ports.dedup_by_key(|p| (p.pid, p.port));
    }

    let matches = PortInfo::filter_by_query(ports, target, false)?;

    if matches.is_empty() {
        bail!("No process found matching '{}'", target);
    }

    let grouped = group_by_pid(&matches);

    if grouped.len() > 1 && !all {
        eprintln!("Multiple processes found:");
        for (pid, infos) in &grouped {
            let ports: Vec<_> = infos.iter().map(|p| p.port.to_string()).collect();
            eprintln!(
                "  PID {} ({}) on ports: {}",
                pid,
                infos[0].process_name,
                ports.join(", ")
            );
        }
        bail!("Specify a more specific target, use a port number, or use --all");
    }

    for (pid, infos) in &grouped {
        let process_name = &infos[0].process_name;
        let port_list: Vec<_> = infos.iter().map(|p| p.port.to_string()).collect();

        eprintln!(
            "PID {} ({}) listening on: {}",
            pid,
            process_name,
            port_list.join(", ")
        );
    }

    if !force && !confirm_kill()? {
        eprintln!("Aborted.");
        return Ok(());
    }

    let mut killed = 0;
    for (pid, _) in grouped {
        match kill_process(pid) {
            Ok(()) => {
                eprintln!("Killed PID {}", pid);
                killed += 1;
            }
            Err(e) => eprintln!("Failed to kill PID {}: {}", pid, e),
        }
    }

    if killed == 0 {
        bail!("Failed to kill any processes");
    }

    Ok(())
}

fn group_by_pid(ports: &[PortInfo]) -> HashMap<u32, Vec<&PortInfo>> {
    let mut map: HashMap<u32, Vec<&PortInfo>> = HashMap::new();
    for port in ports {
        map.entry(port.pid).or_default().push(port);
    }
    map
}

fn confirm_kill() -> Result<bool> {
    eprint!("Kill? [y/N]: ");
    io::stderr().flush().context("Failed to flush stderr")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;

    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub fn kill_process(pid: u32) -> Result<()> {
    kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
        .with_context(|| format!("Failed to kill PID {}", pid))?;
    Ok(())
}
