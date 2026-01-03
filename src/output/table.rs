use std::collections::HashSet;

use colored::Colorize;
use comfy_table::{Cell, Color, Table};

use crate::types::PortInfo;

pub fn print_ports(ports: &[PortInfo]) {
    print_ports_inner(ports, &HashSet::new())
}

pub fn print_ports_watch(ports: &[PortInfo], new_ports: &HashSet<&PortInfo>) {
    print_ports_inner(ports, new_ports)
}

fn print_ports_inner(ports: &[PortInfo], new_ports: &HashSet<&PortInfo>) {
    if ports.is_empty() {
        println!("{}", "No results found".yellow());
        return;
    }

    let mut table = Table::new();
    table.set_header(vec!["PORT", "PROTO", "PID", "PROCESS", "ADDRESS"]);

    for port in ports {
        let is_new = new_ports.contains(port);
        let row_color = if is_new { Color::Green } else { Color::Reset };

        table.add_row(vec![
            Cell::new(port.port).fg(if is_new { Color::Green } else { Color::Cyan }),
            Cell::new(port.protocol).fg(row_color),
            Cell::new(port.pid).fg(row_color),
            Cell::new(&port.process_name).fg(row_color),
            Cell::new(&port.address).fg(row_color),
        ]);
    }

    println!("{table}");
    
    let count_str = ports.len().to_string();
    if new_ports.is_empty() {
        println!("\n{} result(s)", count_str.green());
    } else {
        println!(
            "\n{} result(s) ({} new)",
            count_str.green(),
            new_ports.len().to_string().green().bold()
        );
    }
}
