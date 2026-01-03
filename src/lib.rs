pub mod cli;
pub mod commands;
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

pub fn run(cli: Cli) -> Result<()> {
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
