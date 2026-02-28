# portls

Modern cross-platform port inspector in Rust. Clean replacement for `ss`, `netstat`, and `lsof`.

[![crates.io](https://img.shields.io/crates/v/portls.svg)](https://crates.io/crates/portls)

## Installation

```bash
cargo install portls
```

Or build from source:
```bash
git clone https://github.com/ree-see/ports
cd ports
cargo install --path .
```

This installs the `ports` command.

## Usage

### List all listening ports

```bash
ports
```

```
┌──────┬───────┬───────┬─────────────┬─────────┬─────────────┐
│ PORT │ PROTO │ PID   │ PROCESS     │ SERVICE │ ADDRESS     │
├──────┼───────┼───────┼─────────────┼─────────┼─────────────┤
│ 22   │ tcp   │ 1001  │ sshd        │ ssh     │ 0.0.0.0:22  │
│ 80   │ tcp   │ 1234  │ nginx       │ http    │ 0.0.0.0:80  │
│ 443  │ tcp   │ 1234  │ nginx       │ https   │ 0.0.0.0:443 │
│ 3000 │ tcp   │ 5678  │ node        │ -       │ 127.0.0.1   │
│ 5432 │ tcp   │ 9012  │ postgres    │ postgres│ 127.0.0.1   │
└──────┴───────┴───────┴─────────────┴─────────┴─────────────┘

5 result(s)
```

Well-known ports automatically show a SERVICE name (ssh, http, https, postgres, redis, etc.).

### Query by port or process name

```bash
ports 3000          # Find what's using port 3000
ports node          # Find all Node.js processes
```

### Regex filtering

```bash
ports --regex "node|python"     # Match multiple processes
ports --regex "^nginx"          # Anchored match
ports -w --regex "postgres"     # Regex in watch mode
```

### Show established connections

```bash
ports -c
ports -c postgres   # Filter by process
```

```
┌──────┬───────┬──────┬──────────┬─────────────────┬──────────────────┐
│ PORT │ PROTO │ PID  │ PROCESS  │ LOCAL           │ REMOTE           │
├──────┼───────┼──────┼──────────┼─────────────────┼──────────────────┤
│ 443  │ tcp   │ 1234 │ curl     │ 192.168.1.5:443 │ 93.184.216.34:80 │
└──────┴───────┴──────┴──────────┴─────────────────┴──────────────────┘
```

### Watch mode with live updates

```bash
ports -w                    # Refresh every 1 second
ports -w -n 2               # Refresh every 2 seconds
ports -w 3000               # Watch specific port
ports -w --regex "node|go"  # Watch with regex filter
```

New entries are highlighted in green.

### Explain why a port is open

```bash
ports why 3000              # Trace ancestry by port number
ports why node              # Trace ancestry by process name
ports why 54321             # Trace ancestry by PID
ports why node --json       # JSON output
```

```
Process: node (PID 12345)
  Ports:     3000/tcp
  Source:    shell
  Chain:     launchd(1) → Terminal(500) → zsh(12300) → npm(12340) → node(12345)
  Git:       my-app (main)
```

Traces the full process ancestry chain and identifies the source — who started it and why. Auto-detects the target as a port number, PID, or process name.

Source detection covers: systemd, launchd, Docker, cron, pm2, supervisord, gunicorn, runit, s6, tmux, screen, nohup, and direct shell invocations. Also detects git repo context and health warnings (deleted binaries, zombie processes).

The `--why` flag also works inline with regular queries:

```bash
ports 3000 --why            # Standard port query + ancestry info
ports node --why            # Process query + ancestry info
```

### Kill processes

```bash
ports kill 3000             # Kill process on port 3000 (with confirmation)
ports kill node -f          # Force kill without confirmation
ports kill node -a          # Kill all matching processes
ports kill 3000 --connections  # Search established connections too
```

### Interactive mode

```bash
ports -i                    # Select a port to kill interactively
ports -i node               # Filter by process, then select
ports -i -p tcp             # Filter by protocol, then select
```

Use ↑/↓ or j/k to navigate, Enter to select, q to quit.

### Real-time TUI (htop for ports)

```bash
ports top                   # Interactive real-time view
ports top -c                # Show connections instead of listening ports
```

Controls:
- `Tab` — Toggle between listening/connections mode
- `p`/`i`/`n` — Sort by port/pid/name
- `↑`/`↓`/`j`/`K` — Navigate
- `PgUp`/`PgDn` — Page navigation
- `k` — Kill selected process (shows confirmation popup)
- `q` — Quit

New ports are highlighted green for 3 seconds.

### Port usage history

Track port usage over time with SQLite-backed history:

```bash
ports history record        # Take a snapshot (run via cron)
ports history record -c     # Include established connections
ports history show          # View recent history
ports history show --port 80 --hours 48
ports history timeline 22   # Timeline for specific port
ports history stats         # Database statistics
ports history clean --keep 168  # Keep only 1 week (hours)
ports history diff          # Show ports that appeared/disappeared since last snapshot
ports history diff --ago 5  # Diff against 5 snapshots ago
```

Example `diff` output:
```
┌──────┬───────┬──────────┬─────────────┐
│ PORT │ PROTO │ PROCESS  │ ACTION      │
├──────┼───────┼──────────┼─────────────┤
│ 3000 │ tcp   │ node     │ appeared    │
│ 8080 │ tcp   │ python   │ disappeared │
└──────┴───────┴──────────┴─────────────┘
```

Example cron job for continuous monitoring:
```bash
# Record port state every 5 minutes
*/5 * * * * /usr/local/bin/ports history record
```

History data is stored in `~/.local/share/ports/ports_history.db`.

### Docker container awareness

When ports are forwarded by Docker, `ports` automatically shows which container they map to (via the Docker API — no subprocess overhead):

```bash
ports
```

```
┌──────┬───────┬──────┬──────────────┬───────────────┬─────────┬──────────────┐
│ PORT │ PROTO │ PID  │ PROCESS      │ CONTAINER     │ SERVICE │ ADDRESS      │
├──────┼───────┼──────┼──────────────┼───────────────┼─────────┼──────────────┤
│ 80   │ tcp   │ 1234 │ docker-proxy │ nginx-prod    │ http    │ 0.0.0.0:80   │
│ 443  │ tcp   │ 1234 │ docker-proxy │ nginx-prod    │ https   │ 0.0.0.0:443  │
│ 5432 │ tcp   │ 5678 │ docker-proxy │ postgres-db   │ postgres│ 0.0.0.0:5432 │
│ 3000 │ tcp   │ 9012 │ node         │ -             │ -       │ 127.0.0.1    │
└──────┴───────┴──────┴──────────────┴───────────────┴─────────┴──────────────┘
```

You can also query by container name:

```bash
ports nginx         # Find all ports for nginx containers
ports postgres-db   # Find specific container
```

### Filter and sort

```bash
ports -p tcp                # TCP only
ports -p udp                # UDP only
ports -s port               # Sort by port number
ports -s pid                # Sort by PID
ports -s name               # Sort by process name
```

### JSON output

```bash
ports --json
ports 3000 --json
ports -c --json
```

```json
[
  {
    "port": 3000,
    "protocol": "tcp",
    "pid": 5678,
    "process_name": "node",
    "address": "127.0.0.1:3000"
  }
]
```

## Shell Completions

```bash
# Generate and save
ports completions bash > /usr/local/share/bash-completion/completions/ports
ports completions zsh > /usr/local/share/zsh/site-functions/_ports
ports completions fish > ~/.config/fish/completions/ports.fish

# Or eval dynamically in shell config
eval "$(ports completions bash)"  # ~/.bashrc
eval "$(ports completions zsh)"   # ~/.zshrc
ports completions fish | source   # ~/.config/fish/config.fish
```

## Claude Code Integration

This repo ships with a [Claude Code](https://claude.ai/code) skill at `.claude/commands/ports.md`. When you open this project in Claude Code, it automatically knows the full `ports` CLI — every subcommand, flag, and common recipe. No MCP server to install, no config to set up.

Ask it to kill a port, monitor connections, check history diffs, or anything else `ports` can do and it'll use the CLI directly through bash.

## Platform Support

| Platform | Listening ports | Connections |
|----------|----------------|-------------|
| Linux    | Native `/proc/net` parsing | Native `/proc/net` |
| macOS    | `listeners` crate | `lsof` |
| Others   | `listeners` crate fallback | — |

## Examples

```bash
# Find what's blocking port 8080
ports 8080

# Find out why a port is open (trace process ancestry)
ports why 3000

# Quick ancestry info inline with a query
ports 8080 --why

# Monitor all network activity in real-time
ports -c -w

# Kill all Node.js processes on any port
ports kill node -a -f

# Kill a process that only has established connections (no listening socket)
ports kill myapp --connections

# Export all listening ports as JSON
ports --json > ports.json

# Watch PostgreSQL connections
ports -w -c postgres

# Find all web servers with regex
ports --regex "nginx|apache|caddy"

# Interactively select and kill a port
ports -i

# See what changed since the last snapshot
ports history record && ports history diff
```
