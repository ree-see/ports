//! Interactive real-time port viewer (htop-style), built on ratatui.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::Terminal;

use crate::ancestry::{self, ProcessAncestry};
use crate::cli::SortField;
use crate::commands::kill::kill_process;
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
    /// Track which ports we've seen before (for highlighting new ones).
    seen_ports: HashMap<(u16, Protocol, u32), Instant>,
    /// When true, show kill confirmation overlay.
    confirm_kill: bool,
    /// Transient message shown in header (e.g. "Killed PID 1234").
    status_msg: Option<(String, Instant)>,
    /// PID for which ancestry detail popup is shown.
    detail_pid: Option<u32>,
    /// Cached ancestry for the detail popup.
    detail_ancestry: Option<ProcessAncestry>,
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
            confirm_kill: false,
            status_msg: None,
            detail_pid: None,
            detail_ancestry: None,
        }
    }
}

pub fn run(connections: bool) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, connections);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    connections: bool,
) -> Result<()> {
    let mut state = TopState::new(connections);
    let poll_timeout = Duration::from_millis(100);
    let refresh_interval = Duration::from_secs(1);
    let mut last_refresh = Instant::now()
        .checked_sub(refresh_interval)
        .unwrap_or_else(Instant::now);
    let mut ports: Vec<PortInfo> = Vec::new();
    let new_threshold = Duration::from_secs(3);
    let status_display_duration = Duration::from_secs(3);

    loop {
        let now = Instant::now();

        // Refresh data every second
        if now.duration_since(last_refresh) >= refresh_interval {
            ports = fetch_ports(&state)?;
            // Update seen_ports: insert any port not yet tracked
            for p in &ports {
                let key = (p.port, p.protocol, p.pid);
                state.seen_ports.entry(key).or_insert(now);
            }
            last_refresh = now;
        }

        // Clear expired status messages
        if let Some((_, ts)) = &state.status_msg {
            if now.duration_since(*ts) >= status_display_duration {
                state.status_msg = None;
            }
        }

        // Clamp selection
        let max_sel = ports.len().saturating_sub(1);
        if state.selected > max_sel {
            state.selected = max_sel;
        }

        // Draw
        let now = Instant::now(); // refresh after potential data fetch
        terminal.draw(|frame| {
            draw(frame, &mut state, &ports, now, new_threshold);
        })?;

        // Handle input with a short poll
        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                if state.confirm_kill {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Some(port) = ports.get(state.selected) {
                                let pid = port.pid;
                                let msg = match kill_process(pid) {
                                    Ok(()) => format!("Killed PID {}", pid),
                                    Err(e) => format!("Failed to kill PID {}: {}", pid, e),
                                };
                                state.status_msg = Some((msg, Instant::now()));
                            }
                            state.confirm_kill = false;
                        }
                        _ => {
                            state.confirm_kill = false;
                        }
                    }
                } else if state.detail_pid.is_some() {
                    // Dismiss detail popup on any key.
                    state.detail_pid = None;
                    state.detail_ancestry = None;
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }

                        // Show ancestry detail for selected process.
                        KeyCode::Enter => {
                            if let Some(port) = ports.get(state.selected) {
                                let pid = port.pid;
                                state.detail_pid = Some(pid);
                                state.detail_ancestry =
                                    ancestry::get_ancestry(pid, &port.process_name);
                            }
                        }

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

                        // Kill
                        KeyCode::Char('k') => {
                            if !ports.is_empty() {
                                state.confirm_kill = true;
                            }
                        }

                        // Navigation
                        KeyCode::Up | KeyCode::Char('K') => {
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
                            let visible = (height as usize).saturating_sub(6);
                            state.selected = state.selected.saturating_sub(visible);
                        }
                        KeyCode::PageDown => {
                            let (_, height) = terminal::size()?;
                            let visible = (height as usize).saturating_sub(6);
                            state.selected =
                                (state.selected + visible).min(ports.len().saturating_sub(1));
                        }
                        KeyCode::Home => state.selected = 0,
                        KeyCode::End => state.selected = ports.len().saturating_sub(1),

                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn fetch_ports(state: &TopState) -> Result<Vec<PortInfo>> {
    let mut ports = match state.mode {
        ViewMode::Listening => platform::get_listening_ports()?,
        ViewMode::Connections => platform::get_connections()?,
    };
    ports = PortInfo::enrich_with_docker(ports);
    PortInfo::sort_vec(&mut ports, Some(state.sort));
    Ok(ports)
}

fn draw(
    frame: &mut ratatui::Frame,
    state: &mut TopState,
    ports: &[PortInfo],
    now: Instant,
    new_threshold: Duration,
) {
    let area = frame.area();

    // Layout: header (1), stats (1), table (fill), footer (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Header ────────────────────────────────────────────────────────────
    let mode_str = match state.mode {
        ViewMode::Listening => "LISTENING",
        ViewMode::Connections => "CONNECTIONS",
    };
    let sort_str = match state.sort {
        SortField::Port => "port",
        SortField::Pid => "pid",
        SortField::Name => "name",
    };

    let header_text = if let Some((ref msg, _)) = state.status_msg {
        Line::from(vec![Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow),
        )])
    } else {
        Line::from(vec![Span::styled(
            format!(
                "ports top - {} ({} entries, sorted by {})",
                mode_str,
                ports.len(),
                sort_str
            ),
            Style::default().fg(Color::Cyan),
        )])
    };
    frame.render_widget(Paragraph::new(header_text), chunks[0]);

    // ── Stats ─────────────────────────────────────────────────────────────
    let tcp_count = ports.iter().filter(|p| p.protocol == Protocol::Tcp).count();
    let udp_count = ports.iter().filter(|p| p.protocol == Protocol::Udp).count();
    let process_count = ports
        .iter()
        .map(|p| p.pid)
        .collect::<std::collections::HashSet<_>>()
        .len();

    let stats_text = Line::from(vec![Span::styled(
        format!(
            "TCP: {}  UDP: {}  Processes: {}",
            tcp_count, udp_count, process_count
        ),
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(Paragraph::new(stats_text), chunks[1]);

    // ── Port table ────────────────────────────────────────────────────────
    let visible_rows = chunks[2].height as usize;

    // Adjust scroll to keep selection visible
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if state.selected >= state.scroll_offset + visible_rows {
        state.scroll_offset = state.selected.saturating_sub(visible_rows) + 1;
    }

    let is_connections = state.mode == ViewMode::Connections;
    let header_cells = if is_connections {
        vec!["PROTO", "PORT", "PID", "LOCAL", "REMOTE", "PROCESS"]
    } else {
        vec!["PROTO", "PORT", "PID", "ADDRESS", "PROCESS"]
    };

    let header = Row::new(header_cells.iter().map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    }));

    let rows: Vec<Row> = ports
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(visible_rows)
        .map(|(i, port)| {
            let is_selected = i == state.selected;
            let key = (port.port, port.protocol, port.pid);
            let is_new = state
                .seen_ports
                .get(&key)
                .map(|t| now.duration_since(*t) < new_threshold)
                .unwrap_or(true);

            let process_display = if let Some(ref container) = port.container {
                format!("{} ({})", port.process_name, container)
            } else {
                port.process_name.clone()
            };

            let cells: Vec<Cell> = if is_connections {
                let remote = port.remote_address.as_deref().unwrap_or("-");
                vec![
                    Cell::from(port.protocol.to_string()),
                    Cell::from(port.port.to_string()),
                    Cell::from(port.pid.to_string()),
                    Cell::from(port.address.clone()),
                    Cell::from(remote.to_string()),
                    Cell::from(process_display),
                ]
            } else {
                vec![
                    Cell::from(port.protocol.to_string()),
                    Cell::from(port.port.to_string()),
                    Cell::from(port.pid.to_string()),
                    Cell::from(port.address.clone()),
                    Cell::from(process_display),
                ]
            };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if is_new {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            Row::new(cells).style(style)
        })
        .collect();

    let widths = if is_connections {
        vec![
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(22),
            Constraint::Length(22),
            Constraint::Fill(1),
        ]
    } else {
        vec![
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(22),
            Constraint::Fill(1),
        ]
    };

    let mut table_state = TableState::default();
    // TableState doesn't control our custom scroll, but we still pass it for API compat.
    let table = Table::new(rows, widths).header(header);
    frame.render_stateful_widget(table, chunks[2], &mut table_state);

    // ── Footer ────────────────────────────────────────────────────────────
    let footer_text = if state.confirm_kill {
        Line::from(vec![Span::styled(
            "Kill selected process? [y]es / any key to cancel",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )])
    } else {
        Line::from(vec![Span::styled(
            "q:Quit  Tab:Toggle  p/i/n:Sort  ↑↓/j/K:Nav  PgUp/PgDn:Page  Enter:Info  k:Kill",
            Style::default().fg(Color::DarkGray),
        )])
    };
    frame.render_widget(Paragraph::new(footer_text), chunks[3]);

    // ── Kill confirmation popup ────────────────────────────────────────────
    if state.confirm_kill {
        if let Some(port) = ports.get(state.selected) {
            let popup_text = format!(
                "Kill PID {} ({}) on port {}?  [y]es / any key to cancel",
                port.pid, port.process_name, port.port
            );
            let popup_area = centered_rect(60, 3, area);
            frame.render_widget(Clear, popup_area);
            frame.render_widget(
                Paragraph::new(popup_text)
                    .block(Block::default().borders(Borders::ALL).title("Confirm Kill"))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Red)),
                popup_area,
            );
        }
    }

    // ── Ancestry detail popup ───────────────────────────────────────────────
    if let Some(detail_pid) = state.detail_pid {
        let mut lines: Vec<Line> = Vec::new();

        if let Some(ref a) = state.detail_ancestry {
            lines.push(Line::from(vec![
                Span::styled("Source:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", a.source), Style::default().fg(Color::Green)),
            ]));

            if let Some(ref unit) = a.systemd_unit {
                lines.push(Line::from(vec![
                    Span::styled("Unit:    ", Style::default().fg(Color::DarkGray)),
                    Span::styled(unit.clone(), Style::default()),
                ]));
            }

            if let Some(ref label) = a.launchd_label {
                lines.push(Line::from(vec![
                    Span::styled("Label:   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(label.clone(), Style::default()),
                ]));
            }

            let chain_str: String = a
                .chain
                .iter()
                .rev()
                .map(|anc| format!("{}({})", anc.name, anc.pid))
                .collect::<Vec<_>>()
                .join(" → ");
            lines.push(Line::from(vec![
                Span::styled("Chain:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(chain_str, Style::default()),
            ]));

            if let Some(ref git) = a.git_context {
                let branch_str = git
                    .branch
                    .as_deref()
                    .map(|b| format!(" ({})", b))
                    .unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::styled("Git:     ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{}{}", git.repo_name, branch_str),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
            }

            if !a.warnings.is_empty() {
                let w_str: String = a
                    .warnings
                    .iter()
                    .map(|w| format!("{}", w))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(Line::from(vec![
                    Span::styled("Warnings:", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {}", w_str), Style::default().fg(Color::Red)),
                ]));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "Ancestry data unavailable",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let popup_height = (lines.len() as u16) + 2; // +2 for borders
        let popup_area = centered_rect(70, popup_height, area);
        frame.render_widget(Clear, popup_area);
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Process Info - PID {}", detail_pid))
                        .title_alignment(Alignment::Center),
                )
                .style(Style::default()),
            popup_area,
        );
    }
}

/// Returns a centered `Rect` with the given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_width = r.width * percent_x / 100;
    let x = r.x + (r.width.saturating_sub(popup_width)) / 2;
    let y = r.y + r.height / 2;
    Rect {
        x,
        y,
        width: popup_width,
        height: height.min(r.height),
    }
}
