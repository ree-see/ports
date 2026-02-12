# ports

Modern cross-platform port inspector in Rust. Clean replacement for `ss`, `netstat`, and `lsof`.

## Installation

```bash
git clone https://github.com/ree-see/ports
cd ports
cargo install --path .
```

## Usage

### List all listening ports

```bash
ports
```

```
┌──────┬───────┬───────┬─────────────┬─────────────┐
│ PORT │ PROTO │ PID   │ PROCESS     │ ADDRESS     │
├──────┼───────┼───────┼─────────────┼─────────────┤
│ 80   │ tcp   │ 1234  │ nginx       │ 0.0.0.0:80  │
│ 443  │ tcp   │ 1234  │ nginx       │ 0.0.0.0:443 │
│ 3000 │ tcp   │ 5678  │ node        │ 127.0.0.1   │
│ 5432 │ tcp   │ 9012  │ postgres    │ 127.0.0.1   │
└──────┴───────┴───────┴─────────────┴─────────────┘

4 result(s)
```

### Query by port or process name

```bash
ports 3000          # Find what's using port 3000
ports node          # Find all Node.js processes
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
│ 443  │ tcp   │ 1234 │ curl     │ 192.168.1.5     │ 93.184.216.34    │
└──────┴───────┴──────┴──────────┴─────────────────┴──────────────────┘
```

### Watch mode with live updates

```bash
ports -w                    # Refresh every 1 second
ports -w -n 2               # Refresh every 2 seconds
ports -w 3000               # Watch specific port
```

New entries are highlighted in green.

### Kill processes

```bash
ports kill 3000             # Kill process on port 3000 (with confirmation)
ports kill node -f          # Force kill without confirmation
ports kill node -a          # Kill all matching processes
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
- `Tab` - Toggle between listening/connections mode
- `p`/`i`/`n` - Sort by port/pid/name
- `↑`/`↓`/`j`/`k` - Navigate
- `PgUp`/`PgDn` - Page navigation
- `q` - Quit

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
```

Example cron job for continuous monitoring:
```bash
# Record port state every 5 minutes
*/5 * * * * /usr/local/bin/ports history record
```

History data is stored in `~/.local/share/ports/ports_history.db`.

### Docker container awareness

When ports are forwarded by Docker, `ports` automatically shows which container they map to:

```bash
ports
```

```
┌──────┬───────┬──────┬──────────────┬───────────────┬──────────────┐
│ PORT │ PROTO │ PID  │ PROCESS      │ CONTAINER     │ ADDRESS      │
├──────┼───────┼──────┼──────────────┼───────────────┼──────────────┤
│ 80   │ tcp   │ 1234 │ docker-proxy │ nginx-prod    │ 0.0.0.0:80   │
│ 443  │ tcp   │ 1234 │ docker-proxy │ nginx-prod    │ 0.0.0.0:443  │
│ 5432 │ tcp   │ 5678 │ docker-proxy │ postgres-db   │ 0.0.0.0:5432 │
│ 3000 │ tcp   │ 9012 │ node         │ -             │ 127.0.0.1    │
└──────┴───────┴──────┴──────────────┴───────────────┴──────────────┘
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

## Platform Support

- **Linux**: Native `/proc/net` parsing for maximum performance
- **macOS**: Uses `lsof` fallback via `listeners` crate
- **Others**: Generic `listeners` crate fallback

## Examples

```bash
# Find what's blocking port 8080
ports 8080

# Monitor all network activity in real-time
ports -c -w

# Kill all Node.js processes on any port
ports kill node -a -f

# Export all listening ports as JSON
ports --json > ports.json

# Watch PostgreSQL connections
ports -w -c postgres

# Interactively select and kill a port
ports -i
```
