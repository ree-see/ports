use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ports")]
#[command(version, about = "Modern cross-platform port inspector")]
pub struct Cli {
    /// Port number or process name to query
    pub query: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List all listening ports
    List,
    /// Kill process using a port or by name
    Kill {
        /// Port number or process name
        target: String,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}
