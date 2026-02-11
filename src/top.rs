//! Interactive real-time port viewer (htop-style).

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::cli::SortField;
use crate::platform;
use crate::types::{PortInfo, Protocol};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Listening,
    Connections,
}

struct TopState {
    mode: ViewMode,
    sort: SortField,
    scroll_offset: usize,
    selected: usize,
    /// Track which ports we've seen before (for highlighting new ones)
    seen_ports: HashMap<(u16, Protocol, u32), Instant>,
}

impl TopState {
    fn new(connections: bool) -> Self {
        Self {
            mode: if connections {
                ViewMode::Connections
            } else {
                ViewMode::Listening
            },
            sort: SortField::Port,
            scroll_offset: 0,
            selected: 0,
            seen_ports: HashMap::new(),
        }
    }
}

pub fn run(connections: bool) -> Result<()> {
    let mut stdout = io::stdout();

    // Enter alternate screen and hide cursor
    execute!(stdout, EnterAlternateScreen, Hide)?;
    terminal::enable_raw_mode()?;

    let result = run_loop(&mut stdout, connections);

    // Cleanup
    terminal::disable_raw_mode()?;
    execute!(stdout, Show, LeaveAlternateScreen)?;

    result
}

fn run_loop(stdout: &mut io::Stdout, connections: bool) -> Result<()> {
    let mut state = TopState::new(connections);
    let poll_timeout = Duration::from_millis(100);

    loop {
        // Fetch current data
        let ports = fetch_ports(&state)?;

        // Mark new ports
        let now = Instant::now();
        let new_threshold = Duration::from_secs(3);

        // Render
        render(stdout, &state, &ports, now, new_threshold)?;

        // Update seen ports
        for p in &ports {
            let key = (p.port, p.protocol, p.pid);
            state.seen_ports.entry(key).or_insert(now);
        }

        // Handle input
        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,

                    // Toggle mode
                    KeyCode::Tab => {
                        state.mode = match state.mode {
                            ViewMode::Listening => ViewMode::Connections,
                            ViewMode::Connections => ViewMode::Listening,
                        };
                        state.scroll_offset = 0;
                        state.selected = 0;
                    }

                    // Sort
                    KeyCode::Char('p') => state.sort = SortField::Port,
                    KeyCode::Char('i') => state.sort = SortField::Pid,
                    KeyCode::Char('n') => state.sort = SortField::Name,

                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.selected > 0 {
                            state.selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.selected < ports.len().saturating_sub(1) {
                            state.selected += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        let (_, height) = terminal::size()?;
                        let visible = (height as usize).saturating_sub(8);
                        state.selected = state.selected.saturating_sub(visible);
                    }
                    KeyCode::PageDown => {
                        let (_, height) = terminal::size()?;
                        let visible = (height as usize).saturating_sub(8);
                        state.selected = (state.selected + visible).min(ports.len().saturating_sub(1));
                    }
                    KeyCode::Home => state.selected = 0,
                    KeyCode::End => state.selected = ports.len().saturating_sub(1),

                    _ => {}
                }
            }
        }

        // Small sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(50));
    }

    Ok(())
}

fn fetch_ports(state: &TopState) -> Result<Vec<PortInfo>> {
    let mut ports = match state.mode {
        ViewMode::Listening => platform::get_listening_ports()?,
        ViewMode::Connections => platform::get_connections()?,
    };

    // Enrich with Docker info
    ports = PortInfo::enrich_with_docker(ports);

    // Sort
    PortInfo::sort_vec(&mut ports, Some(state.sort));

    Ok(ports)
}

fn render(
    stdout: &mut io::Stdout,
    state: &TopState,
    ports: &[PortInfo],
    now: Instant,
    new_threshold: Duration,
) -> Result<()> {
    let (width, height) = terminal::size()?;
    let width = width as usize;
    let height = height as usize;

    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

    // Header
    render_header(stdout, state, ports, width)?;

    // Summary stats
    render_stats(stdout, ports)?;

    // Column headers
    render_column_headers(stdout, state, width)?;

    // Calculate visible rows (leave room for header, stats, column headers, and footer)
    let header_lines = 4;
    let footer_lines = 2;
    let visible_rows = height.saturating_sub(header_lines + footer_lines);

    // Adjust scroll to keep selection visible
    let scroll_offset = if state.selected < state.scroll_offset {
        state.selected
    } else if state.selected >= state.scroll_offset + visible_rows {
        state.selected - visible_rows + 1
    } else {
        state.scroll_offset
    };

    // Render ports
    for (i, port) in ports.iter().skip(scroll_offset).take(visible_rows).enumerate() {
        let row = header_lines + i;
        let is_selected = scroll_offset + i == state.selected;
        let key = (port.port, port.protocol, port.pid);
        let is_new = state
            .seen_ports
            .get(&key)
            .map(|t| now.duration_since(*t) < new_threshold)
            .unwrap_or(true);

        render_port_row(stdout, port, row, width, is_selected, is_new, state.mode)?;
    }

    // Footer
    render_footer(stdout, height)?;

    stdout.flush()?;
    Ok(())
}

fn render_header(
    stdout: &mut io::Stdout,
    state: &TopState,
    ports: &[PortInfo],
    _width: usize,
) -> Result<()> {
    let mode_str = match state.mode {
        ViewMode::Listening => "LISTENING",
        ViewMode::Connections => "CONNECTIONS",
    };

    let sort_str = match state.sort {
        SortField::Port => "port",
        SortField::Pid => "pid",
        SortField::Name => "name",
    };

    execute!(
        stdout,
        MoveTo(0, 0),
        SetForegroundColor(Color::Cyan),
        Print(format!("ports top - {} ({} entries, sorted by {})", mode_str, ports.len(), sort_str)),
        ResetColor,
    )?;

    Ok(())
}

fn render_stats(stdout: &mut io::Stdout, ports: &[PortInfo]) -> Result<()> {
    let tcp_count = ports.iter().filter(|p| p.protocol == Protocol::Tcp).count();
    let udp_count = ports.iter().filter(|p| p.protocol == Protocol::Udp).count();

    // Count unique processes
    let mut processes: HashMap<u32, &str> = HashMap::new();
    for p in ports {
        processes.entry(p.pid).or_insert(&p.process_name);
    }

    execute!(
        stdout,
        MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print(format!(
            "TCP: {}  UDP: {}  Processes: {}",
            tcp_count, udp_count, processes.len()
        )),
        ResetColor,
    )?;

    Ok(())
}

fn render_column_headers(stdout: &mut io::Stdout, state: &TopState, width: usize) -> Result<()> {
    execute!(stdout, MoveTo(0, 3))?;

    let headers = if state.mode == ViewMode::Connections {
        format!(
            "{:<7} {:<6} {:<7} {:<22} {:<22} {}",
            "PROTO", "PORT", "PID", "LOCAL", "REMOTE", "PROCESS"
        )
    } else {
        format!(
            "{:<7} {:<6} {:<7} {:<22} {}",
            "PROTO", "PORT", "PID", "ADDRESS", "PROCESS"
        )
    };

    execute!(
        stdout,
        SetForegroundColor(Color::Yellow),
        Print(format!("{:width$}", headers, width = width)),
        ResetColor,
    )?;

    Ok(())
}

fn render_port_row(
    stdout: &mut io::Stdout,
    port: &PortInfo,
    row: usize,
    width: usize,
    is_selected: bool,
    is_new: bool,
    mode: ViewMode,
) -> Result<()> {
    execute!(stdout, MoveTo(0, row as u16))?;

    // Build the line
    let process_display = if let Some(ref container) = port.container {
        format!("{} ({})", port.process_name, container)
    } else {
        port.process_name.clone()
    };

    let line = if mode == ViewMode::Connections {
        let remote = port.remote_address.as_deref().unwrap_or("-");
        format!(
            "{:<7} {:<6} {:<7} {:<22} {:<22} {}",
            port.protocol.to_string(),
            port.port,
            port.pid,
            truncate(&port.address, 22),
            truncate(remote, 22),
            process_display
        )
    } else {
        format!(
            "{:<7} {:<6} {:<7} {:<22} {}",
            port.protocol.to_string(),
            port.port,
            port.pid,
            truncate(&port.address, 22),
            process_display
        )
    };

    let line = format!("{:width$}", line, width = width);

    // Color based on state
    if is_selected {
        execute!(
            stdout,
            SetForegroundColor(Color::Black),
            crossterm::style::SetBackgroundColor(Color::White),
            Print(&line),
            ResetColor,
        )?;
    } else if is_new {
        execute!(
            stdout,
            SetForegroundColor(Color::Green),
            Print(&line),
            ResetColor,
        )?;
    } else {
        execute!(stdout, Print(&line))?;
    }

    Ok(())
}

fn render_footer(stdout: &mut io::Stdout, height: usize) -> Result<()> {
    execute!(
        stdout,
        MoveTo(0, (height - 1) as u16),
        SetForegroundColor(Color::DarkGrey),
        Print("q:Quit  Tab:Toggle mode  p/i/n:Sort by port/pid/name  ↑↓/jk:Navigate  PgUp/PgDn:Page"),
        ResetColor,
    )?;

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
