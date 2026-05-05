# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Fish shell completions no longer mix listening-port subcommands with files and
  directories from the current working directory. Users with an existing
  `~/.config/fish/completions/ports.fish` should regenerate it after upgrading:
  `ports completions fish > ~/.config/fish/completions/ports.fish`.

## [0.2.1] - 2026-02-22

### Added

- Claude Code skill shipped in repo (`.claude/commands/ports.md`) — any Claude Code user gets full CLI knowledge automatically
- `CLAUDE.md` project instructions for LLM context

## [0.1.0] - 2026-01-03

### Added

- Initial release
- List listening ports with `ports`
- Query by port number or process name
- Show established connections with `-c, --connections`
- Watch mode with `-w, --watch` and configurable interval `-n`
- Kill processes with `ports kill <target>`
  - Force kill with `-f, --force`
  - Kill all matching with `-a, --all`
- Interactive mode with `-i, --interactive` (navigate with arrows or j/k)
- Sort results with `-s, --sort <port|pid|name>`
- Filter by protocol with `-p, --protocol <tcp|udp>`
- JSON output with `--json`
- Shell completions with `ports completions <bash|zsh|fish>`

### Platform Support

- **Linux**: Native `/proc/net` parsing for TCP, TCP6, UDP, UDP6
- **macOS**: `lsof` for connections, `listeners` crate for listening ports
- **Others**: Generic `listeners` crate fallback
