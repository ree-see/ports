use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ports")]
#[command(version, about = "Modern cross-platform port inspector")]
pub struct Cli {
    /// Port number or process name to query
    pub query: Option<String>,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,

    /// Watch mode: refresh continuously
    #[arg(short, long, global = true)]
    pub watch: bool,

    /// Refresh interval in seconds (default: 1)
    #[arg(short = 'n', long, default_value = "1", global = true)]
    pub interval: f64,

    /// Show established connections instead of listening ports
    #[arg(short, long, global = true)]
    pub connections: bool,

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
