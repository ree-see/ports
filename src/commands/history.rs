//! History command implementation

use anyhow::Result;
use chrono::Local;
use colored::Colorize;
use comfy_table::{
    presets::UTF8_FULL_CONDENSED, Attribute, Cell, Color, ContentArrangement, Table,
};

use crate::history::{self, DiffAction, HistoryQuery};

/// Record a snapshot of current port state
pub fn record(include_connections: bool, json: bool) -> Result<()> {
    let result = history::record_snapshot(include_connections)?;

    if json {
        let output = serde_json::json!({
            "snapshot_id": result.snapshot_id,
            "port_count": result.port_count,
            "timestamp": result.timestamp.to_rfc3339(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} Recorded {} ports at {}",
            "✓".green(),
            result.port_count.to_string().cyan(),
            result
                .timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
        );
    }

    Ok(())
}

/// Show history for a port or process
pub fn show(
    port: Option<u16>,
    process: Option<String>,
    hours: Option<i64>,
    limit: usize,
    json: bool,
) -> Result<()> {
    let query = HistoryQuery {
        port,
        process,
        hours,
        limit,
    };

    let entries = history::get_history(&query)?;

    if json {
        let output: Vec<_> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "timestamp": e.timestamp.to_rfc3339(),
                    "port": e.port,
                    "protocol": e.protocol,
                    "address": e.address,
                    "pid": e.pid,
                    "process_name": e.process_name,
                    "container": e.container,
                    "state": e.state,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("{}", "No history found matching your query.".yellow());
        println!("Run {} to start recording.", "ports history record".cyan());
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Time").add_attribute(Attribute::Bold),
            Cell::new("Port").add_attribute(Attribute::Bold),
            Cell::new("Proto").add_attribute(Attribute::Bold),
            Cell::new("Process").add_attribute(Attribute::Bold),
            Cell::new("State").add_attribute(Attribute::Bold),
        ]);

    for entry in entries {
        let local_time = entry.timestamp.with_timezone(&Local);
        let time_str = local_time.format("%m-%d %H:%M").to_string();

        let process_display = if let Some(ref container) = entry.container {
            format!("{} ({})", entry.process_name, container)
        } else {
            entry.process_name.clone()
        };

        let state_cell = match entry.state.as_deref() {
            Some("LISTEN") => Cell::new("LISTEN").fg(Color::Green),
            Some("ESTABLISHED") => Cell::new("ESTABLISHED").fg(Color::Cyan),
            Some(s) => Cell::new(s).fg(Color::Yellow),
            None => Cell::new("-"),
        };

        table.add_row(vec![
            Cell::new(time_str),
            Cell::new(entry.port).fg(Color::Cyan),
            Cell::new(&entry.protocol),
            Cell::new(process_display),
            state_cell,
        ]);
    }

    println!("{table}");
    Ok(())
}

/// Show statistics about recorded history
pub fn stats(json: bool) -> Result<()> {
    let stats = history::get_stats()?;
    let top_ports = history::get_top_ports(10)?;

    if json {
        let output = serde_json::json!({
            "snapshot_count": stats.snapshot_count,
            "total_entries": stats.total_entries,
            "unique_ports": stats.unique_ports,
            "oldest_snapshot": stats.oldest_snapshot.map(|dt| dt.to_rfc3339()),
            "newest_snapshot": stats.newest_snapshot.map(|dt| dt.to_rfc3339()),
            "db_size_bytes": stats.db_size_bytes,
            "top_ports": top_ports.iter().map(|(port, proto, count)| {
                serde_json::json!({
                    "port": port,
                    "protocol": proto,
                    "occurrences": count,
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("{}", "📊 History Statistics".bold());
    println!();
    println!(
        "  Snapshots:    {}",
        stats.snapshot_count.to_string().cyan()
    );
    println!("  Port entries: {}", stats.total_entries.to_string().cyan());
    println!("  Unique ports: {}", stats.unique_ports.to_string().cyan());
    println!(
        "  Database:     {}",
        history::format_bytes(stats.db_size_bytes).cyan()
    );

    if let Some(oldest) = stats.oldest_snapshot {
        let local = oldest.with_timezone(&Local);
        println!(
            "  Oldest:       {}",
            local.format("%Y-%m-%d %H:%M").to_string().dimmed()
        );
    }
    if let Some(newest) = stats.newest_snapshot {
        let local = newest.with_timezone(&Local);
        println!(
            "  Newest:       {}",
            local.format("%Y-%m-%d %H:%M").to_string().dimmed()
        );
    }

    if !top_ports.is_empty() {
        println!();
        println!("{}", "🔝 Most Recorded Ports".bold());
        for (port, proto, count) in top_ports {
            println!(
                "  {:>5}/{:<3}  {} occurrences",
                port.to_string().cyan(),
                proto,
                count
            );
        }
    }

    Ok(())
}

/// Show timeline for a specific port
pub fn timeline(port: u16, hours: i64, json: bool) -> Result<()> {
    let entries = history::get_port_timeline(port, hours)?;

    if json {
        let output: Vec<_> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "timestamp": e.timestamp.to_rfc3339(),
                    "protocol": e.protocol,
                    "process_name": e.process_name,
                    "container": e.container,
                    "state": e.state,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!(
            "{}",
            format!(
                "No history found for port {} in the last {} hours.",
                port, hours
            )
            .yellow()
        );
        return Ok(());
    }

    println!(
        "{}",
        format!("📅 Timeline for port {} (last {} hours)", port, hours).bold()
    );
    println!();

    let mut prev_process: Option<String> = None;

    for entry in entries {
        let local_time = entry.timestamp.with_timezone(&Local);
        let time_str = local_time.format("%m-%d %H:%M:%S").to_string();

        let process_display = if let Some(ref container) = entry.container {
            format!("{} ({})", entry.process_name, container)
        } else {
            entry.process_name.clone()
        };

        // Show change indicator
        let indicator = if prev_process.as_ref() != Some(&process_display) {
            prev_process = Some(process_display.clone());
            "→".green()
        } else {
            "·".dimmed()
        };

        let state_str = entry.state.as_deref().unwrap_or("-");

        println!(
            "  {} {} {} {} {}",
            time_str.dimmed(),
            indicator,
            entry.protocol,
            process_display.cyan(),
            state_str.dimmed()
        );
    }

    Ok(())
}

/// Show diff between two snapshots
pub fn diff(ago: usize, json: bool) -> Result<()> {
    let entries = history::get_diff(ago)?;

    if json {
        let output: Vec<_> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "port": e.port,
                    "protocol": e.protocol,
                    "process_name": e.process_name,
                    "action": match e.action {
                        DiffAction::Appeared => "appeared",
                        DiffAction::Disappeared => "disappeared",
                    },
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("{}", "No changes detected between snapshots.".yellow());
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("PORT").add_attribute(Attribute::Bold),
            Cell::new("PROTO").add_attribute(Attribute::Bold),
            Cell::new("PROCESS").add_attribute(Attribute::Bold),
            Cell::new("ACTION").add_attribute(Attribute::Bold),
        ]);

    for entry in &entries {
        let (action_cell, port_color) = match entry.action {
            DiffAction::Appeared => (Cell::new("appeared").fg(Color::Green), Color::Green),
            DiffAction::Disappeared => (Cell::new("disappeared").fg(Color::Red), Color::Red),
        };

        table.add_row(vec![
            Cell::new(entry.port).fg(port_color),
            Cell::new(&entry.protocol),
            Cell::new(&entry.process_name),
            action_cell,
        ]);
    }

    println!("{table}");
    Ok(())
}

/// Clean up old history
pub fn cleanup(keep_hours: i64, json: bool) -> Result<()> {
    let result = history::cleanup(keep_hours)?;

    if json {
        let output = serde_json::json!({
            "snapshots_deleted": result.snapshots_deleted,
            "entries_deleted": result.entries_deleted,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} Cleaned up {} snapshots ({} port entries)",
            "✓".green(),
            result.snapshots_deleted.to_string().cyan(),
            result.entries_deleted.to_string().cyan()
        );
    }

    Ok(())
}
