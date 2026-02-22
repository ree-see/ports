use std::collections::HashSet;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use anyhow::Result;

use crate::cli::{ProtocolFilter, SortField};
use crate::output::{json, table};
use crate::platform;
use crate::types::PortInfo;

pub struct WatchOptions {
    pub interval: Duration,
    pub json: bool,
    pub filter: Option<String>,
    pub connections: bool,
    pub sort: Option<SortField>,
    pub protocol: Option<ProtocolFilter>,
    pub use_regex: bool,
}

pub fn run(options: WatchOptions) -> Result<()> {
    let mut previous: HashSet<PortInfo> = HashSet::new();

    loop {
        clear_screen();

        let ports = if options.connections {
            platform::get_connections()?
        } else {
            platform::get_listening_ports()?
        };
        let ports = PortInfo::filter_protocol(ports, options.protocol);
        // Enrich docker-proxy entries with container names
        let ports = PortInfo::enrich_with_docker(ports);
        let mut filtered = filter_ports(ports, &options.filter, options.use_regex)?;
        PortInfo::sort_vec(&mut filtered, options.sort);

        if options.json {
            json::print_ports(&filtered);
        } else {
            let new_ports: HashSet<&PortInfo> = filtered
                .iter()
                .filter(|p| !previous.contains(*p))
                .collect();

            table::print_ports_watch(&filtered, &new_ports);
        }

        print_watch_status(&options);
        io::stdout().flush()?;

        previous = filtered.into_iter().collect();
        thread::sleep(options.interval);
    }
}

fn filter_ports(ports: Vec<PortInfo>, filter: &Option<String>, use_regex: bool) -> anyhow::Result<Vec<PortInfo>> {
    match filter {
        None => Ok(ports),
        Some(query) => PortInfo::filter_by_query(ports, query, use_regex),
    }
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

fn print_watch_status(options: &WatchOptions) {
    use colored::Colorize;
    let mode = if options.connections {
        "connections"
    } else {
        "listening"
    };
    println!(
        "\n{} {} (every {:.1}s, Ctrl+C to exit)",
        "Watching".dimmed(),
        mode.dimmed(),
        options.interval.as_secs_f64()
    );
}
