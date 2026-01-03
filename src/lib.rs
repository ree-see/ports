pub mod cli;
pub mod commands;
pub mod output;
pub mod platform;
pub mod types;
pub mod watch;

pub use cli::Cli;

use std::time::Duration;

use anyhow::Result;

pub fn run(cli: Cli) -> Result<()> {
    if cli.watch {
        let filter = match &cli.command {
            Some(cli::Commands::List) => None,
            Some(cli::Commands::Kill { .. }) => {
                anyhow::bail!("Cannot use --watch with kill command");
            }
            None => cli.query.clone(),
        };

        return watch::run(watch::WatchOptions {
            interval: Duration::from_secs_f64(cli.interval),
            json: cli.json,
            filter,
            connections: cli.connections,
        });
    }

    match &cli.command {
        Some(cli::Commands::List) => commands::list::execute(cli.json, cli.connections),
        Some(cli::Commands::Kill { target, force }) => commands::kill::execute(target, *force),
        None => match &cli.query {
            Some(query) => commands::query::execute(query, cli.json, cli.connections),
            None => commands::list::execute(cli.json, cli.connections),
        },
    }
}
