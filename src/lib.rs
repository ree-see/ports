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

pub mod ancestry;
pub mod cli;
pub mod commands;
pub mod docker;
pub mod filter;
pub mod framework;
pub mod history;
pub mod interactive;
pub mod output;
pub mod platform;
pub mod project;
pub mod top;
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
            Some(cli::Commands::Top { .. }) => {
                anyhow::bail!("Cannot use --watch with top command (top has its own refresh)");
            }
            Some(cli::Commands::Why { .. }) => {
                anyhow::bail!("Cannot use --watch with why command");
            }
            Some(cli::Commands::History { .. }) => {
                anyhow::bail!("Cannot use --watch with history command");
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
            use_regex: cli.regex,
            why: cli.why,
            dev: cli.dev,
        });
    }

    match &cli.command {
        Some(cli::Commands::List) => commands::list::execute(
            cli.json,
            cli.connections,
            cli.sort,
            cli.protocol,
            cli.why,
            cli.dev,
        ),
        Some(cli::Commands::Kill {
            target,
            force,
            all,
            connections,
        }) => commands::kill::execute(target, *force, *all, *connections),
        Some(cli::Commands::Why { target }) => commands::why::execute(target, cli.json),
        Some(cli::Commands::Top { connections }) => top::run(*connections, cli.dev),
        Some(cli::Commands::Completions { shell }) => {
            use clap_complete::Shell;
            let mut cmd = Cli::command();
            if matches!(shell, Shell::Fish) {
                print!("{}", build_fish_completions(&mut cmd));
            } else {
                generate(*shell, &mut cmd, "ports", &mut io::stdout());
            }
            Ok(())
        }
        Some(cli::Commands::History { action }) => match action {
            cli::HistoryAction::Record { connections } => {
                commands::history::record(*connections, cli.json)
            }
            cli::HistoryAction::Show {
                port,
                process,
                hours,
                limit,
            } => commands::history::show(*port, process.clone(), Some(*hours), *limit, cli.json),
            cli::HistoryAction::Timeline { port, hours } => {
                commands::history::timeline(*port, *hours, cli.json)
            }
            cli::HistoryAction::Stats => commands::history::stats(cli.json),
            cli::HistoryAction::Clean { keep } => commands::history::cleanup(*keep, cli.json),
            cli::HistoryAction::Diff { ago } => commands::history::diff(*ago, cli.json),
        },
        None => match &cli.query {
            Some(query) => commands::query::execute(
                query,
                cli.json,
                cli.connections,
                cli.sort,
                cli.protocol,
                cli.regex,
                cli.why,
                cli.dev,
            ),
            None => commands::list::execute(
                cli.json,
                cli.connections,
                cli.sort,
                cli.protocol,
                cli.why,
                cli.dev,
            ),
        },
    }
}

fn run_interactive(cli: &Cli) -> Result<()> {
    let ports = if cli.connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    let mut ports = PortInfo::filter_protocol(ports, cli.protocol);
    if cli.dev {
        filter::retain_dev_only(&mut ports);
    }

    if let Some(query) = &cli.query {
        ports = PortInfo::filter_by_query(ports, query, cli.regex)?;
    }

    PortInfo::sort_vec(&mut ports, cli.sort);

    let ancestry_map = if cli.why {
        let pids_with_names: Vec<(u32, &str)> = ports
            .iter()
            .map(|p| (p.pid, p.process_name.as_str()))
            .collect();
        Some(ancestry::get_ancestry_batch(&pids_with_names))
    } else {
        None
    };

    interactive::select_and_kill(&ports, ancestry_map.as_ref())
}

fn build_fish_completions(cmd: &mut clap::Command) -> String {
    use clap_complete::Shell;
    let mut buf = Vec::new();
    generate(Shell::Fish, cmd, "ports", &mut buf);
    let body = String::from_utf8(buf).expect("clap_complete fish output is valid UTF-8");
    format!("complete -c ports -f\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_completions_suppress_files() {
        let mut cmd = Cli::command();
        let out = build_fish_completions(&mut cmd);
        assert!(
            out.starts_with("complete -c ports -f\n"),
            "fish completion must prepend `complete -c ports -f` so fish \
             doesn't fall back to file completion at the top level"
        );
        assert!(
            out.contains("-a \"kill\""),
            "subcommand candidates must still appear after the prefix"
        );
    }
}
