use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = ports::Cli::parse();
    ports::run(cli)
}
