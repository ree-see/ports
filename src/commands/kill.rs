use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::{bail, Context, Result};
use sysinfo::{Pid, Signal, System};

use crate::platform;
use crate::types::PortInfo;

pub fn execute(target: &str, force: bool) -> Result<()> {
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

    let grouped = group_by_pid(&matches);

    if grouped.len() > 1 {
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
        bail!("Specify a more specific target or use a port number");
    }

    let (pid, infos) = grouped.into_iter().next().unwrap();
    let process_name = &infos[0].process_name;
    let ports: Vec<_> = infos.iter().map(|p| p.port.to_string()).collect();

    eprintln!(
        "PID {} ({}) listening on: {}",
        pid,
        process_name,
        ports.join(", ")
    );

    if !force && !confirm_kill()? {
        eprintln!("Aborted.");
        return Ok(());
    }

    kill_process(pid)?;
    eprintln!("Killed PID {}", pid);

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

fn kill_process(pid: u32) -> Result<()> {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let sysinfo_pid = Pid::from_u32(pid);
    let process = sys
        .process(sysinfo_pid)
        .context(format!("Process {} not found", pid))?;

    if !process.kill_with(Signal::Term).unwrap_or(false) {
        bail!("Failed to kill process {}", pid);
    }

    Ok(())
}
