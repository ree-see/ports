use colored::Colorize;
use comfy_table::{Cell, Table};

use crate::types::PortInfo;

pub fn print_ports(ports: &[PortInfo]) {
    if ports.is_empty() {
        println!("{}", "No listening ports found".yellow());
        return;
    }

    let mut table = Table::new();
    table.set_header(vec!["PORT", "PROTO", "PID", "PROCESS", "ADDRESS"]);

    for port in ports {
        table.add_row(vec![
            Cell::new(port.port).fg(comfy_table::Color::Cyan),
            Cell::new(port.protocol),
            Cell::new(port.pid),
            Cell::new(&port.process_name),
            Cell::new(&port.address),
        ]);
    }

    println!("{table}");
    println!(
        "\n{} listening port(s) found",
        ports.len().to_string().green()
    );
}
