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

    let has_remote = ports.iter().any(|p| p.remote_address.is_some());
    let has_container = ports.iter().any(|p| p.container.is_some());
    let has_service = ports.iter().any(|p| p.service_name.is_some());

    let mut table = Table::new();

    // Build header based on what columns we need
    let mut headers = vec!["PORT", "PROTO", "PID", "PROCESS"];
    if has_service {
        headers.push("SERVICE");
    }
    if has_container {
        headers.push("CONTAINER");
    }
    if has_remote {
        headers.push("LOCAL");
        headers.push("REMOTE");
    } else {
        headers.push("ADDRESS");
    }
    table.set_header(headers);

    for port in ports {
        let is_new = new_ports.contains(port);
        let row_color = if is_new { Color::Green } else { Color::Reset };
        let proto_color = if is_new {
            Color::Green
        } else {
            match port.protocol {
                crate::types::Protocol::Tcp => Color::Cyan,
                crate::types::Protocol::Udp => Color::Magenta,
            }
        };

        let mut row = vec![
            Cell::new(port.port).fg(if is_new { Color::Green } else { Color::Cyan }),
            Cell::new(port.protocol).fg(proto_color),
            Cell::new(port.pid).fg(row_color),
            Cell::new(&port.process_name).fg(row_color),
        ];

        if has_service {
            let service = port.service_name.as_deref().unwrap_or("-");
            row.push(Cell::new(service).fg(row_color));
        }

        if has_container {
            let container = port.container.as_deref().unwrap_or("-");
            // Containers get yellow color for visibility
            let container_color = if port.container.is_some() && !is_new {
                Color::Yellow
            } else {
                row_color
            };
            row.push(Cell::new(container).fg(container_color));
        }

        row.push(Cell::new(&port.address).fg(row_color));

        if has_remote {
            let remote = port.remote_address.as_deref().unwrap_or("-");
            row.push(Cell::new(remote).fg(row_color));
        }

        table.add_row(row);
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
