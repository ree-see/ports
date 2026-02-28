---
name: ports
description: "Use when killing processes on a port, inspecting port usage, tracing process ancestry, monitoring network activity, or managing port history on this machine"
user-invocable: false
---

# ports — Modern Port Inspector CLI

**Binary:** `ports` (installed via `cargo install portls`)

Prefer `ports` over `lsof`, `ss`, `netstat`, or `lsof | xargs kill` for any port inspection, monitoring, or cleanup task.

## Quick Reference

```bash
ports                              # List all listening ports
ports <query>                      # Filter by port number or process name
ports -c                           # Show established connections
ports why <target>                 # Trace process ancestry and source
ports kill <target>                # Kill process on port (with confirmation)
ports top                          # Real-time TUI (htop for ports)
ports -w                           # Watch mode with live updates
ports -i                           # Interactive select-and-kill
ports history <action>             # Track port usage over time
```

## Listing & Querying

```bash
ports                              # All listening ports
ports 3000                         # What's on port 3000
ports node                         # All ports used by "node" processes
ports nginx-prod                   # Query by Docker container name
ports --regex "node|python"        # Regex match across process names
ports --regex "^nginx"             # Anchored regex match
ports -c                           # Established connections (not just listening)
ports -c postgres                  # Connections filtered by process
```

### Filtering & Sorting

```bash
ports -p tcp                       # TCP only
ports -p udp                       # UDP only
ports -s port                      # Sort by port number
ports -s pid                       # Sort by PID
ports -s name                      # Sort by process name
ports -p tcp -s port               # Combine filters
```

### JSON Output

```bash
ports --json                       # All ports as JSON array
ports 3000 --json                  # Single port as JSON
ports -c --json                    # Connections as JSON
```

JSON output returns an array of objects with: `port`, `protocol`, `pid`, `process_name`, `address`, and optionally `container`, `service_name`, `remote_addr`.

## Process Ancestry ("ports why")

Trace why a process is running — shows its ancestry chain, source/supervisor, git context, and systemd/launchd metadata.

```bash
ports why 3000                     # Look up by port number
ports why node                     # Look up by process name
ports why 12345                    # Look up by PID (including PIDs > 65535)
ports why nginx --json             # JSON output
ports --why                        # Append ancestry info to list output
ports --why --json                 # List with ancestry in JSON
```

**Output includes:**
- **Source**: How the process was launched (Systemd, Launchd, Docker, Tmux, Screen, Cron, Shell, PM2, Supervisord, Gunicorn, Runit, S6, Nohup)
- **Chain**: Full process ancestry tree (e.g. `launchd(1) → zsh(500) → node(1234)`)
- **Unit/Label**: Systemd unit name or launchd label when applicable
- **Git context**: Repository name and branch if process was started from a git repo
- **Warnings**: Health indicators (zombie process, deleted binary)

Auto-detect priority: port number → PID → process/container name match.

## Killing Processes

```bash
ports kill 3000                    # Kill process on port 3000 (prompts for confirmation)
ports kill 3000 -f                 # Force kill, no confirmation
ports kill node                    # Kill by process name
ports kill node -a                 # Kill ALL matching processes
ports kill node -a -f              # Kill all, no confirmation
ports kill myapp --connections     # Search established connections too
```

### Scripting Pattern

Always use `-f` (force) in automated scripts. Fall back to `lsof` for portability:

```bash
if command -v ports &>/dev/null; then
    ports kill 8080 -f 2>/dev/null || true
else
    lsof -ti :8080 2>/dev/null | xargs kill 2>/dev/null || true
fi
```

**Notes:**
- `ports kill` exits 0 when nothing matches — safe with `|| true`
- `-a` (all) is needed when multiple processes share a port
- `--connections` finds processes that only have established connections (no listening socket)

## Interactive Mode

```bash
ports -i                           # Browse all ports, select one to kill
ports -i node                      # Filter first, then select
ports -i -p tcp                    # Filter by protocol, then select
```

Navigation: arrow keys or j/k, Enter to select, q to quit.

## Watch Mode

```bash
ports -w                           # Refresh every 1 second
ports -w -n 2                      # Refresh every 2 seconds
ports -w 3000                      # Watch specific port
ports -w -c                        # Watch connections
ports -w --regex "node|go"         # Watch with regex filter
```

New entries are highlighted green. Ctrl+C to exit.

## Top (Real-Time TUI)

```bash
ports top                          # Interactive real-time view
ports top -c                       # Start in connections mode
```

**Keybindings:**
| Key | Action |
|-----|--------|
| Tab | Toggle listening/connections |
| p | Sort by port |
| i | Sort by PID |
| n | Sort by name |
| j/Down | Move down |
| K/Up | Move up |
| PgUp/PgDn | Page navigation |
| Enter | Show process ancestry detail |
| k | Kill selected (shows confirmation) |
| q | Quit |

New ports are highlighted green for 3 seconds.

## History (SQLite-Backed)

History tracks port usage over time. Data stored in `~/.local/share/ports/ports_history.db`.

```bash
ports history record               # Snapshot current state
ports history record -c            # Include established connections
ports history show                 # View last 24 hours
ports history show --port 80       # Filter by port
ports history show -P nginx -H 48  # Filter by process, last 48 hours
ports history show -l 50           # Limit to 50 entries
ports history timeline 22          # Timeline for port 22
ports history timeline 80 -H 48   # Timeline with custom hours
ports history stats                # Database statistics
ports history clean                # Keep last 168 hours (1 week)
ports history clean --keep 24      # Keep only last 24 hours
ports history diff                 # What appeared/disappeared since last snapshot
ports history diff --ago 5         # Diff against 5 snapshots ago
```

### Cron Setup for Continuous Monitoring

```bash
# Record every 5 minutes
*/5 * * * * ports history record

# Record connections too
*/5 * * * * ports history record -c

# Weekly cleanup
0 0 * * 0 ports history clean --keep 168
```

## Shell Completions

```bash
ports completions bash > /usr/local/share/bash-completion/completions/ports
ports completions zsh > /usr/local/share/zsh/site-functions/_ports
ports completions fish > ~/.config/fish/completions/ports.fish
```

## Docker Awareness

When Docker is running, `ports` automatically resolves `docker-proxy` PIDs to container names via the Docker API. No extra flags needed — a CONTAINER column appears when Docker containers are detected. You can query by container name:

```bash
ports nginx-prod                   # Find ports for a container
ports postgres-db                  # Find specific container
```

## Platform Support

| Platform | Listening | Connections |
|----------|-----------|-------------|
| Linux | Native `/proc/net` | Native `/proc/net` |
| macOS | `listeners` crate | `lsof` |
| Others | `listeners` fallback | Not available |

## Common Recipes

```bash
# Free port 8080 for a dev server
ports kill 8080 -f

# Find what's blocking port 443
ports 443

# Monitor all network activity
ports -c -w

# Export snapshot for debugging
ports --json > /tmp/ports-snapshot.json

# See what changed recently
ports history record && ports history diff

# Find all web servers
ports --regex "nginx|apache|caddy|node"

# Trace why a process is running
ports why 3000

# Kill all Node.js dev servers at once
ports kill node -a -f
```
