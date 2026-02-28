use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

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

    /// Sort results by field
    #[arg(short, long, value_enum, global = true)]
    pub sort: Option<SortField>,

    /// Filter by protocol
    #[arg(short, long, value_enum, global = true)]
    pub protocol: Option<ProtocolFilter>,

    /// Interactive mode: select a port to kill
    #[arg(short, long, global = true)]
    pub interactive: bool,

    /// Treat query as a regular expression
    #[arg(long, global = true)]
    pub regex: bool,

    /// Show process ancestry and source information
    #[arg(long, global = true)]
    pub why: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SortField {
    Port,
    Pid,
    Name,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ProtocolFilter {
    Tcp,
    Udp,
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
        /// Kill all matching processes (instead of erroring on multiple matches)
        #[arg(short, long)]
        all: bool,
        /// Search established connections in addition to listening ports
        #[arg(long)]
        connections: bool,
    },
    /// Interactive real-time view (like htop for ports)
    Top {
        /// Show connections instead of listening ports
        #[arg(short, long)]
        connections: bool,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Show why a process is running (ancestry, source, supervisor)
    Why {
        /// Port number, process name, or PID to investigate
        target: String,
    },
    /// Track port usage over time
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },
}

#[derive(Subcommand)]
pub enum HistoryAction {
    /// Record current port state (run periodically via cron)
    Record {
        /// Include established connections (not just listening ports)
        #[arg(short, long)]
        connections: bool,
    },
    /// Show recorded history
    Show {
        /// Filter by port number
        #[arg(long)]
        port: Option<u16>,
        /// Filter by process name
        #[arg(short = 'P', long)]
        process: Option<String>,
        /// Hours of history to show (default: 24)
        #[arg(short = 'H', long, default_value = "24")]
        hours: i64,
        /// Maximum entries to show
        #[arg(short, long, default_value = "100")]
        limit: usize,
    },
    /// Show timeline for a specific port
    Timeline {
        /// Port number to show timeline for
        port: u16,
        /// Hours of history (default: 24)
        #[arg(short = 'H', long, default_value = "24")]
        hours: i64,
    },
    /// Show statistics about recorded history
    Stats,
    /// Clean up old history entries
    Clean {
        /// Hours of history to keep (default: 168 = 1 week)
        #[arg(short, long, default_value = "168")]
        keep: i64,
    },
    /// Show ports that appeared or disappeared between two snapshots
    Diff {
        /// Compare latest snapshot against this many snapshots ago (default: 1)
        #[arg(short, long, default_value = "1")]
        ago: usize,
    },
}
