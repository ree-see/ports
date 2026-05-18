# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cargo features gate the heaviest dependencies. Four features, all default-on, preserve `cargo install portls` behaviour: `docker` (`bollard`, `tokio`), `tui` (`ratatui`, `crossterm`, `dialoguer`), `history` (`rusqlite-bundled`, `chrono`), and `watch` (code-only gate, no extra deps). Install a slim binary with `cargo install portls --no-default-features` (drops Docker, the TUI, history, and watch mode — roughly a 54% release-binary size cut on macOS). Mix and match with `--features` for everything in between. Subcommands and flags whose feature is disabled return a clear runtime error instructing the user how to rebuild.
- GitHub Actions CI workflow (`.github/workflows/ci.yml`) gating every push and pull request to `main`/`dev` on `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked`. Matrix runs on `ubuntu-latest` and `macos-latest` so Linux/macOS asymmetries surface before publish.

### Changed

- **BREAKING**: `ports completions <shell>` now installs the completion file
  to the shell's standard user directory by default, instead of printing to
  stdout. Per-shell paths: fish to `~/.config/fish/completions/ports.fish`,
  bash to `~/.local/share/bash-completion/completions/ports`, zsh to
  `~/.zsh/completions/_ports`. PowerShell and Elvish auto-install are not
  supported — use `--print` and redirect manually. Migration: if you had
  `ports completions fish > file` in your dotfiles, change it to
  `ports completions fish --print > file`.
- **BREAKING**: the library API is trimmed to `portls::Cli` and `portls::run`
  only. All other modules (`portls::framework`, `portls::types`,
  `portls::platform`, `portls::commands`, etc.) are now crate-private and
  cannot be imported by downstream library consumers. `portls` ships a binary
  (`ports`); the library surface is intentionally minimal and is **not**
  considered part of the crate's public API for semver purposes. Downstream
  library consumers (none known) should pin to `0.3.x` or vendor the modules
  they depend on. The `ports` binary is unaffected.

### Fixed

- Fish shell completions no longer mix listening-port subcommands with files and
  directories from the current working directory. Users with an existing
  `~/.config/fish/completions/ports.fish` should regenerate it after upgrading:
  `ports completions fish` (now installs in place).

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
