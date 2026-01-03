pub mod cli;
pub mod commands;
pub mod output;
pub mod platform;
pub mod types;

pub use cli::Cli;

use anyhow::Result;

pub fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Some(cli::Commands::List) => commands::list::execute(),
        Some(cli::Commands::Kill { target }) => commands::kill::execute(target),
        None => match &cli.query {
            Some(query) => commands::query::execute(query),
            None => commands::list::execute(),
        },
    }
}
