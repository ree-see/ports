use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = portls::Cli::parse();
    portls::run(cli)
}
