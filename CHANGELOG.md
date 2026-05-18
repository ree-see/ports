# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `ports history record` now auto-prunes the SQLite database so the cron recipe in the README (`*/5 * * * * ports history record`) cannot quietly grow the DB to multiple gigabytes. After every 20th record (about every 100 minutes on a 5-minute cron) the path deletes everything older than 30 days. VACUUM is skipped on the auto path so the snapshot stays fast; freed pages get reused by subsequent inserts. Use `ports history clean --keep <hours>` for a one-shot prune that also reclaims disk space. The behaviour is opt-out via `--no-auto-prune` and `PORTLS_HISTORY_NO_PRUNE=1`, and the retention window is configurable with `--auto-prune-hours <HOURS>` (rejected at 0; capped at ~100 years).
- `ports history record` also prints a yellow `warning: history DB is …` line on stderr (in both table and JSON modes, matching the existing Docker-daemon warning convention) when the on-disk database grows past 500 MB, so a runaway DB surfaces well before it hurts.
- History database schema is bumped to v2 with a new `meta` key/value table used to persist the auto-prune counter. A forward-compatibility sanity check refuses to operate on a DB whose `user_version` is newer than the binary supports — note this is **not** real downgrade protection (the older binary the user might downgrade to predates the check), only scaffolding for a future v3 binary to refuse a hypothetical v3 DB written by an even newer release.
- Cargo features gate the heaviest dependencies. Four features, all default-on, preserve `cargo install portls` behaviour: `docker` (`bollard`, `tokio`), `tui` (`ratatui`, `crossterm`, `dialoguer`), `history` (`rusqlite-bundled`, `chrono`), and `watch` (code-only gate, no extra deps). Install a slim binary with `cargo install portls --no-default-features` (drops Docker, the TUI, history, and watch mode — roughly a 54% release-binary size cut on macOS). Mix and match with `--features` for everything in between. Subcommands and flags whose feature is disabled return a clear runtime error instructing the user how to rebuild.
- GitHub Actions CI workflow (`.github/workflows/ci.yml`) gating every push and pull request to `main`/`dev` on `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked`. Matrix runs on `ubuntu-latest` and `macos-latest` so Linux/macOS asymmetries surface before publish.
- Docker daemon reachability is now surfaced. When `docker-proxy` listeners are observed but the daemon cannot be reached, table output prints a yellow `warning: docker daemon unreachable (...); container names omitted` line to stderr before the table. JSON output exposes the same signal as two flat fields (`docker_status`, `docker_reason`) so scripts can distinguish "no containers running" from "daemon down." Watch mode dedupes the warning on status transitions so the stderr stream stays quiet.
- Failed Docker fetches now cache for 500 ms (vs the 3 s cache on success), so a recovering daemon is rediscovered on the next watch/top tick instead of after the full success window.
- `DOCKER_HOST` URIs with embedded basic-auth credentials (e.g. `ssh://user:pass@host`) are redacted to `ssh://***@host` before the error chain hits stderr or JSON output.

### Changed

- **Behaviour change for existing cron users**: the first auto-prune after upgrading will delete every recorded snapshot older than 30 days, which on a long-running cron could be months of data. Set `--no-auto-prune` or `PORTLS_HISTORY_NO_PRUNE=1` on the cron line to preserve the old keep-forever behaviour, or use `--auto-prune-hours <HOURS>` to widen the window.
- **One-way schema bump**: downgrading to a pre-v0.7.0 binary leaves the DB at `user_version = 2`. Queries still work because the v2 schema is purely additive (a `meta` table the older binary doesn't reference), but auto-prune is silently disabled until you re-upgrade. There is no in-binary protection for this scenario in earlier releases.
- The internal `record_snapshot` function now takes an `&AutoPruneConfig` argument. The library API stays limited to `portls::Cli` and `portls::run` per the v0.4.0 trim, so this is not a semver-relevant change — flagged here only for the vigilant downstream consumer who imports through the unsupported library surface anyway.
- **BREAKING**: `ports --json` now emits an object `{"ports": [...], "docker_status": "ok" | "unreachable" | "not_queried", "docker_reason": null | "..."}` instead of a bare array. Scripts parsing `--json` must read `.ports` (e.g. `jq '.ports[]'` instead of `jq '.[]'`). The `--why --json` and `--json` flavours of `ports` and `ports <query>` all use the same wrapper; `ports why <target> --json` retains its existing array-of-`WhyEntry` shape.
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

- Three distinct Docker failure modes (tokio runtime creation, bollard connect, `list_containers` query) used to collapse into an empty container map indistinguishable from "no containers running." They now each surface as a labelled `Unreachable { reason }` (`tokio runtime: ...`, `docker connect: ...`, `docker query: ...`).
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
