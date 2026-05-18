use std::collections::{HashMap, HashSet};

use colored::Colorize;
use comfy_table::{Cell, Color, Table};

use crate::ancestry::ProcessAncestry;
use crate::types::{DockerStatus, PortInfo};

/// Print a yellow stderr warning when the Docker daemon was probed and
/// found unreachable. Silent for `Ok` and `NotQueried`.
///
/// Intentionally writes to stderr so `ports --json | jq` still works.
/// `pub(crate)` so raw-mode TUI contexts (top, interactive) can't even
/// reach for it — they call into `output::table` only via `print_ports*`,
/// which no longer warn implicitly.
pub(crate) fn print_warning(status: &DockerStatus) {
    if let DockerStatus::Unreachable { reason } = status {
        let line =
            format!("warning: docker daemon unreachable ({reason}); container names omitted");
        eprintln!("{}", line.yellow());
    }
}

pub fn print_ports(ports: &[PortInfo]) {
    print_ports_inner(ports, &HashSet::new())
}

#[allow(dead_code)] // only used by the `watch` feature
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
    let has_framework = ports.iter().any(|p| p.framework.is_some());

    let mut table = Table::new();

    // Build header based on what columns we need
    let mut headers = vec!["PORT", "PROTO", "PID", "PROCESS"];
    if has_service {
        headers.push("SERVICE");
    }
    if has_framework {
        headers.push("FRAMEWORK");
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

        if has_framework {
            let fw = port.framework.as_deref().unwrap_or("-");
            let fw_color = if port.framework.is_some() && !is_new {
                Color::Magenta
            } else {
                row_color
            };
            row.push(Cell::new(fw).fg(fw_color));
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

/// Print ports table with an extra SOURCE column from ancestry data.
pub fn print_ports_why(ports: &[PortInfo], ancestry_map: &HashMap<u32, ProcessAncestry>) {
    if ports.is_empty() {
        println!("{}", "No results found".yellow());
        return;
    }

    let has_remote = ports.iter().any(|p| p.remote_address.is_some());
    let has_container = ports.iter().any(|p| p.container.is_some());
    let has_service = ports.iter().any(|p| p.service_name.is_some());
    let has_framework = ports.iter().any(|p| p.framework.is_some());

    let mut table = Table::new();

    let mut headers = vec!["PORT", "PROTO", "PID", "PROCESS", "SOURCE"];
    if has_service {
        headers.push("SERVICE");
    }
    if has_framework {
        headers.push("FRAMEWORK");
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
        let source_str = ancestry_map
            .get(&port.pid)
            .map(|a| format!("{}", a.source))
            .unwrap_or_else(|| "-".to_string());

        let proto_color = match port.protocol {
            crate::types::Protocol::Tcp => Color::Cyan,
            crate::types::Protocol::Udp => Color::Magenta,
        };

        let mut row = vec![
            Cell::new(port.port).fg(Color::Cyan),
            Cell::new(port.protocol).fg(proto_color),
            Cell::new(port.pid),
            Cell::new(&port.process_name),
            Cell::new(&source_str).fg(Color::Green),
        ];

        if has_service {
            let service = port.service_name.as_deref().unwrap_or("-");
            row.push(Cell::new(service));
        }

        if has_framework {
            let fw = port.framework.as_deref().unwrap_or("-");
            let fw_color = if port.framework.is_some() {
                Color::Magenta
            } else {
                Color::Reset
            };
            row.push(Cell::new(fw).fg(fw_color));
        }

        if has_container {
            let container = port.container.as_deref().unwrap_or("-");
            let container_color = if port.container.is_some() {
                Color::Yellow
            } else {
                Color::Reset
            };
            row.push(Cell::new(container).fg(container_color));
        }

        row.push(Cell::new(&port.address));

        if has_remote {
            let remote = port.remote_address.as_deref().unwrap_or("-");
            row.push(Cell::new(remote));
        }

        table.add_row(row);
    }

    println!("{table}");
    println!("\n{} result(s)", ports.len().to_string().green());
}
