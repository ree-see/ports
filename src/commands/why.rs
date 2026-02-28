//! `ports why <target>` subcommand — trace process ancestry.

use std::collections::HashMap;

use anyhow::Result;
use colored::Colorize;

use crate::ancestry::{self, ProcessAncestry};
use crate::platform;
use crate::types::PortInfo;

pub fn execute(target: &str, output_json: bool) -> Result<()> {
    // Fetch both listening ports and connections for maximum coverage.
    let mut ports = platform::get_listening_ports()?;
    if let Ok(conns) = platform::get_connections() {
        ports.extend(conns);
    }
    let ports = PortInfo::enrich_with_docker(ports);

    // Auto-detect target type: try port number first, then PID, then name.
    let matches = if let Ok(port_num) = target.parse::<u16>() {
        let by_port: Vec<_> = ports
            .iter()
            .filter(|p| p.port == port_num)
            .cloned()
            .collect();
        if by_port.is_empty() {
            // u16 fits in u32 — try as PID.
            ports
                .iter()
                .filter(|p| p.pid == port_num as u32)
                .cloned()
                .collect()
        } else {
            by_port
        }
    } else if let Ok(pid) = target.parse::<u32>() {
        // Doesn't fit in u16, so it can only be a PID.
        ports.iter().filter(|p| p.pid == pid).cloned().collect()
    } else {
        let target_lower = target.to_lowercase();
        ports
            .iter()
            .filter(|p| {
                p.process_name.to_lowercase().contains(&target_lower)
                    || p.container
                        .as_ref()
                        .map(|c| c.to_lowercase().contains(&target_lower))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    };

    if matches.is_empty() {
        if output_json {
            println!("[]");
        } else {
            eprintln!(
                "{} No process found matching '{}'",
                "Error:".red().bold(),
                target
            );
        }
        return Ok(());
    }

    // Deduplicate by PID.
    let mut seen_pids = std::collections::HashSet::new();
    let unique: Vec<_> = matches
        .into_iter()
        .filter(|p| seen_pids.insert(p.pid))
        .collect();

    // Gather ports-per-PID for display.
    let mut ports_by_pid: HashMap<u32, Vec<&PortInfo>> = HashMap::new();
    for p in &ports {
        ports_by_pid.entry(p.pid).or_default().push(p);
    }

    // Fetch ancestry for each unique PID.
    let pids_with_names: Vec<(u32, &str)> = unique
        .iter()
        .map(|p| (p.pid, p.process_name.as_str()))
        .collect();
    let ancestry_map = ancestry::get_ancestry_batch(&pids_with_names);

    if output_json {
        print_json(&unique, &ports_by_pid, &ancestry_map);
    } else {
        print_table(&unique, &ports_by_pid, &ancestry_map);
    }

    Ok(())
}

fn print_table(
    processes: &[PortInfo],
    ports_by_pid: &HashMap<u32, Vec<&PortInfo>>,
    ancestry_map: &HashMap<u32, ProcessAncestry>,
) {
    for (i, proc_info) in processes.iter().enumerate() {
        if i > 0 {
            println!();
        }

        let pid = proc_info.pid;
        let name = &proc_info.process_name;

        // Header line.
        println!(
            "{} {} (PID {})",
            "Process:".cyan().bold(),
            name.bold(),
            pid.to_string().yellow()
        );

        // Ports this PID is using.
        if let Some(pid_ports) = ports_by_pid.get(&pid) {
            let port_strs: Vec<String> = pid_ports
                .iter()
                .map(|p| format!("{}/{}", p.port, p.protocol))
                .collect();
            println!("  {:<10} {}", "Ports:".dimmed(), port_strs.join(", "));
        }

        // Ancestry details.
        if let Some(ancestry) = ancestry_map.get(&pid) {
            println!(
                "  {:<10} {}",
                "Source:".dimmed(),
                format!("{}", ancestry.source).green()
            );

            if let Some(ref unit) = ancestry.systemd_unit {
                println!("  {:<10} {}", "Unit:".dimmed(), unit);
            }

            if let Some(ref label) = ancestry.launchd_label {
                println!("  {:<10} {}", "Label:".dimmed(), label);
            }

            // Chain display: root -> ... -> target
            let chain_str: Vec<String> = ancestry
                .chain
                .iter()
                .rev()
                .map(|a| {
                    if a.pid == pid {
                        format!("{}({})", a.name.bold(), a.pid)
                    } else {
                        format!("{}({})", a.name, a.pid)
                    }
                })
                .collect();
            println!("  {:<10} {}", "Chain:".dimmed(), chain_str.join(" → "));

            if let Some(ref git) = ancestry.git_context {
                let branch_str = git
                    .branch
                    .as_deref()
                    .map(|b| format!(" ({})", b))
                    .unwrap_or_default();
                println!(
                    "  {:<10} {}{}",
                    "Git:".dimmed(),
                    git.repo_name,
                    branch_str.green()
                );
            }

            if !ancestry.warnings.is_empty() {
                let warning_strs: Vec<String> =
                    ancestry.warnings.iter().map(|w| format!("{}", w)).collect();
                println!(
                    "  {:<10} {}",
                    "Warnings:".dimmed(),
                    warning_strs.join(", ").red()
                );
            }
        } else {
            println!(
                "  {:<10} {}",
                "Source:".dimmed(),
                "unknown (ancestry unavailable)".dimmed()
            );
        }
    }
}

fn print_json(
    processes: &[PortInfo],
    ports_by_pid: &HashMap<u32, Vec<&PortInfo>>,
    ancestry_map: &HashMap<u32, ProcessAncestry>,
) {
    use serde::Serialize;

    #[derive(Serialize)]
    struct WhyEntry {
        pid: u32,
        process_name: String,
        ports: Vec<PortEntry>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ancestry: Option<ProcessAncestry>,
    }

    #[derive(Serialize)]
    struct PortEntry {
        port: u16,
        protocol: String,
        address: String,
    }

    let entries: Vec<WhyEntry> = processes
        .iter()
        .map(|p| {
            let port_entries = ports_by_pid
                .get(&p.pid)
                .map(|pp| {
                    pp.iter()
                        .map(|pi| PortEntry {
                            port: pi.port,
                            protocol: format!("{}", pi.protocol),
                            address: pi.address.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            WhyEntry {
                pid: p.pid,
                process_name: p.process_name.clone(),
                ports: port_entries,
                ancestry: ancestry_map.get(&p.pid).cloned(),
            }
        })
        .collect();

    let json = serde_json::to_string_pretty(&entries).expect("Failed to serialize to JSON");
    println!("{json}");
}
