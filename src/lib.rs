//! # ports
//!
//! Modern cross-platform port inspector. A clean replacement for `ss`, `netstat`, and `lsof`.
//!
//! ## Features
//!
//! - List listening ports (TCP/UDP, IPv4/IPv6)
//! - Show established connections
//! - Filter by port number, process name, or protocol
//! - Kill processes by port or name
//! - Interactive selection mode
//! - Watch mode with live updates
//! - JSON output for scripting
//! - Shell completions (bash, zsh, fish)
//!
//! ## Platform Support
//!
//! - **Linux**: Native `/proc/net` parsing for TCP, TCP6, UDP, UDP6
//! - **macOS**: Uses `lsof` for connections, `listeners` crate for listening ports
//! - **Others**: Generic fallback via `listeners` crate

pub mod cli;
pub mod commands;
pub mod docker;
pub mod interactive;
pub mod output;
pub mod platform;
pub mod types;
pub mod watch;

pub use cli::Cli;

use std::io;
use std::time::Duration;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;

use types::PortInfo;

pub fn run(cli: Cli) -> Result<()> {
    if cli.interactive {
        return run_interactive(&cli);
    }

    if cli.watch {
        let filter = match &cli.command {
            Some(cli::Commands::List) => None,
            Some(cli::Commands::Kill { .. }) => {
                anyhow::bail!("Cannot use --watch with kill command");
            }
            Some(cli::Commands::Completions { .. }) => {
                anyhow::bail!("Cannot use --watch with completions command");
            }
            None => cli.query.clone(),
        };

        return watch::run(watch::WatchOptions {
            interval: Duration::from_secs_f64(cli.interval),
            json: cli.json,
            filter,
            connections: cli.connections,
            sort: cli.sort,
            protocol: cli.protocol,
        });
    }

    match &cli.command {
        Some(cli::Commands::List) => {
            commands::list::execute(cli.json, cli.connections, cli.sort, cli.protocol)
        }
        Some(cli::Commands::Kill { target, force, all }) => {
            commands::kill::execute(target, *force, *all)
        }
        Some(cli::Commands::Completions { shell }) => {
            generate(*shell, &mut Cli::command(), "ports", &mut io::stdout());
            Ok(())
        }
        None => match &cli.query {
            Some(query) => {
                commands::query::execute(query, cli.json, cli.connections, cli.sort, cli.protocol)
            }
            None => commands::list::execute(cli.json, cli.connections, cli.sort, cli.protocol),
        },
    }
}

fn run_interactive(cli: &Cli) -> Result<()> {
    let ports = if cli.connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    let ports = PortInfo::filter_protocol(ports, cli.protocol);
    // Enrich docker-proxy entries with container names
    let mut ports = PortInfo::enrich_with_docker(ports);

    if let Some(query) = &cli.query {
        let query_lower = query.to_lowercase();
        ports = if let Ok(port_num) = query.parse::<u16>() {
            ports.into_iter().filter(|p| p.port == port_num).collect()
        } else {
            ports
                .into_iter()
                .filter(|p| {
                    // Match process name or container name
                    p.process_name.to_lowercase().contains(&query_lower)
                        || p.container
                            .as_ref()
                            .map(|c| c.to_lowercase().contains(&query_lower))
                            .unwrap_or(false)
                })
                .collect()
        };
    }

    PortInfo::sort_vec(&mut ports, cli.sort);

    interactive::select_and_kill(&ports)
}
