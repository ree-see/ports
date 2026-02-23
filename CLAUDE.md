# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`portls` is a Rust CLI tool published to crates.io as `portls`, but installs the binary as `ports`. It's a modern replacement for `ss`, `netstat`, and `lsof` — a cross-platform port inspector with watch mode, TUI, kill, interactive selection, Docker awareness, and SQLite-backed history.

## Commands

```bash
cargo build                        # debug build
cargo build --release              # release build
cargo run -- [args]                # run with args (e.g. cargo run -- --help)
cargo test                         # all tests
cargo test --test integration      # CLI smoke tests only
cargo test --test history_integration  # history feature tests only
cargo clippy                       # lint
cargo fmt                          # format
```

## Architecture

The crate is structured as a library (`src/lib.rs`) consumed by a thin binary (`src/main.rs` → `portls::run(cli)`). All subcommand logic lives in the library.

**Data flow**: `lib.rs::run()` dispatches based on CLI flags/subcommands → command handlers call `platform::get_listening_ports()` or `platform::get_connections()` → `PortInfo::enrich_with_docker()` adds container names for `docker-proxy` processes → output via `output::table` or `output::json`.

### Key modules

| Module | Purpose |
|--------|---------|
| `cli` | Clap `Cli` struct, `Commands` enum, `HistoryAction`, `SortField`, `ProtocolFilter` |
| `types` | `PortInfo` struct + `Protocol` enum; filter/sort helpers |
| `platform/` | Platform dispatch: Linux uses native `/proc/net`, macOS uses `lsof`, others use `listeners` crate |
| `platform/linux/` | `proc_parser.rs` parses `/proc/net/{tcp,tcp6,udp,udp6}`, `proc_fd.rs` maps inodes to PIDs |
| `commands/` | One file per subcommand: `list`, `query`, `kill`, `history` |
| `docker` | Calls `docker ps` to map host ports to container names; only invoked when `docker-proxy` processes are detected |
| `history` | SQLite via `rusqlite`: snapshots + ports tables; DB at `~/.local/share/ports/ports_history.db` |
| `interactive` | `dialoguer`-based interactive kill picker |
| `watch` | Polling loop with diff highlighting |
| `top` | `crossterm`-based TUI (htop-style) |
| `output/` | `table.rs` (comfy-table + colored), `json.rs` (serde_json) |

### Platform compilation

```rust
#[cfg(target_os = "linux")]   // native /proc/net parsing
#[cfg(target_os = "macos")]   // lsof-based connections
#[cfg(not(...))]              // listeners crate fallback
```

## Testing Notes

Integration tests in `tests/` spawn `cargo run --` as a subprocess. History integration tests set `HOME` and `XDG_DATA_HOME` env vars to an isolated `TempDir` to avoid touching the real history database:

```rust
Command::new("cargo").args(["run", "--"])
    .env("HOME", temp_home.path())
    .env("XDG_DATA_HOME", temp_home.path().join(".local/share"))
```

## History Database Schema

Two tables: `snapshots` (id, timestamp, unix_ts) and `ports` (snapshot_id FK, port, protocol, address, pid, process_name, container, state, remote_addr). Cascade deletes from snapshots → ports. Indexes on `snapshot_id`, `port`, and `unix_ts`.
