# Plan: Process Ancestry & Source Detection ("ports why")

## Context

The Go tool [witr](https://github.com/pranshuparmar/witr) ("Why Is This Running?") answers process causality: given something running, trace its ancestry chain and identify which supervisor/manager is responsible. This is a natural extension of portls — when someone sees a process on a port, the next question is "where did that come from?"

This plan selectively integrates witr's best features as a native Rust implementation: ancestry chain walking, supervisor detection, health warnings, and git context. It avoids scope creep by NOT adding file lock detection, all 30+ supervisor detectors, or a full process TUI.

---

## Architecture Decision: Ancestry Lives OUTSIDE PortInfo

**The most critical design choice.** Ancestry data is stored in a parallel `HashMap<u32, ProcessAncestry>` keyed by PID, NOT as fields on `PortInfo`. This avoids:

- Breaking `Hash`/`Eq`/`PartialEq` derives (watch mode uses `HashSet<PortInfo>` for diff detection)
- Polluting the history database schema (no migration needed)
- False "new port" detections when ancestry data changes between poll cycles
- Breaking existing JSON consumers

---

## Phase 1: Core Ancestry Module

**New directory**: `src/ancestry/`

### Module structure

```
src/ancestry/
  mod.rs          — public API (get_ancestry, get_ancestry_batch), caching, data types
  linux.rs        — /proc-based chain walking, cgroup parsing, systemd detection, warnings
  macos.rs        — ps/launchctl-based chain walking, launchd detection
  source.rs       — tiered source detection algorithm (shared logic)
  git.rs          — git context detection (cross-platform)
```

This mirrors the existing `src/platform/` split and keeps each file focused.

### Data structures (in `mod.rs`)

```
Ancestor        { pid, name, ppid }
SourceType      enum: Systemd, Launchd, Docker, Cron, Shell, Pm2, Supervisord, Gunicorn, Runit, S6, Tmux, Screen, Nohup, Unknown
HealthWarning   enum: DeletedBinary, ZombieProcess
GitContext       { repo_root, branch }
ProcessAncestry { chain, source, warnings, git_context, systemd_unit, launchd_label }
```

Note: `User` variant removed (no clear detection path; `Shell` covers user-launched processes). `PublicBinding` removed from `HealthWarning` — it moves to `PortInfo::is_public_binding()` on `types.rs` since it's a network property, not a process property.

### Tiered source detection (in `source.rs`)

**This is the most important algorithm.** Source detection uses explicit priority tiers checked in order, NOT a first-match walk up the ancestry chain. This prevents `systemd -> bash -> node` from being classified as `Shell`.

```
Tier 1: Container detection (highest priority)
  - Linux: parse /proc/{pid}/cgroup for /docker/, /containerd/, /kubepods/
  - macOS: check ancestry chain for com.docker.backend
  → Returns Docker if matched

Tier 2: Platform init system
  - Linux: parse /proc/{pid}/cgroup for .service unit → Systemd
  - macOS: check if chain terminates at launchd AND launchctl knows the PID → Launchd
  → Returns Systemd or Launchd if matched

Tier 3: Process supervisors (check ancestry chain names)
  - Priority-ordered match list: pm2, supervisord, gunicorn, runit, s6
  - Walk chain from TOP (PID 1 side) to BOTTOM (target process)
  - First supervisor match wins (highest in tree = actual manager)
  → Returns the matched supervisor type

Tier 4: Multiplexers
  - Check ancestry for tmux, screen, nohup
  → Returns Tmux, Screen, or Nohup

Tier 5: Shell (lowest supervisor priority)
  - Check if direct parent is a shell (bash, sh, zsh, fish, tcsh)
  - BUT only if no higher-tier match was found
  - Skip if the shell itself is a child of systemd/launchd (that's Tier 2)
  → Returns Shell

Tier 6: Cron
  - Check ancestry for cron, crond, anacron
  - Separate tier because cron jobs can also have shell parents
  → Returns Cron

Default: Unknown
```

**Key insight**: Tiers 1-2 use system metadata (cgroup, launchctl) which is authoritative. Tiers 3-6 use ancestry name matching which is heuristic. Always prefer metadata over heuristics.

### Platform implementations

**Linux** (`ancestry/linux.rs`) — native `/proc` reads (zero subprocess calls):
- `walk_ppid_chain(pid)` — parse `/proc/{pid}/stat` for PPID, walk chain to PID 1 with cycle detection
- `read_cgroup(pid)` — read `/proc/{pid}/cgroup`, return raw string for source detection
- `detect_systemd_unit(pid)` — extract unit name from cgroup path (e.g., `nginx.service`)
- `detect_warnings(pid)` — check `/proc/{pid}/exe` for "(deleted)", check `/proc/{pid}/stat` for zombie state (Z)
- `read_process_cwd(pid)` — readlink `/proc/{pid}/cwd`

**macOS** (`ancestry/macos.rs`) — batch `ps` + `launchctl` subprocesses:
- `build_process_table()` — single `ps -A -o pid=,ppid=,comm=` call, parse into `HashMap<u32, (u32, String)>` (pid → (ppid, name)). Used for ALL ancestry lookups in that batch — avoids per-PID subprocess spawning.
- `walk_ppid_chain(pid, table)` — walk the in-memory table instead of per-hop `ps` calls
- `detect_launchd_label(pid)` — `launchctl procinfo <pid>` (note: may fail without root for other users' PIDs — returns `None` gracefully)
- `read_process_cwd(pid)` — `lsof -a -p <pid> -d cwd -Fn`
- No deleted-binary or zombie detection on macOS (these are Linux /proc features)

**Fallback** — returns `None` (graceful degradation)

### Git context (`ancestry/git.rs`)

Cross-platform. Reads process cwd, walks up looking for `.git/HEAD`, parses branch ref.

**Limitation acknowledged**: Daemonized processes typically `chdir("/")` after starting. systemd services default to `WorkingDirectory=/`. For these, git context will always be `None`. This feature is primarily useful for dev-mode interpreted-language processes (node, python, ruby). System services will show no git context — this is correct behavior, not a bug.

### Public binding check — moved to `types.rs`

`PortInfo::is_public_binding(&self) -> bool` — checks if `self.address` starts with `0.0.0.0:`, `:::`, or `[::]:`. This is a network-level property of the port, not the process, so it belongs on `PortInfo` not in the ancestry module.

### Caching (in `mod.rs`)

```rust
struct CacheEntry {
    ancestry: ProcessAncestry,
    process_name: String,  // for PID reuse validation
}

type AncestryCache = HashMap<u32, CacheEntry>;
static CACHE: LazyLock<Mutex<(Instant, AncestryCache)>> = ...;
const CACHE_TTL: Duration = Duration::from_secs(10);
```

**PID reuse protection**: Cache entries store the process name at cache time. On cache hit, the caller's known process name is compared against the cached name. Mismatch → cache miss (PID was recycled). This prevents stale ancestry being returned for a different process that inherited the same PID.

- `get_ancestry(pid, process_name)` — single PID, cache-aware with name validation
- `get_ancestry_batch(pids_with_names)` — batch for `--why` on list output

### No new dependencies

Uses only: `std::fs`, `std::process::Command` (macOS), `serde::Serialize`, `std::sync::LazyLock`

**Files**: NEW `src/ancestry/mod.rs`, `src/ancestry/linux.rs`, `src/ancestry/macos.rs`, `src/ancestry/source.rs`, `src/ancestry/git.rs`. MODIFY `src/lib.rs` (add `pub mod ancestry;`), MODIFY `src/types.rs` (add `is_public_binding` method).

---

## Phase 2: `ports why <target>` Subcommand

**New file**: `src/commands/why.rs`

### CLI addition in `src/cli.rs`
```rust
// New variant in Commands enum:
Why {
    /// Port number, process name, or PID to investigate
    target: String,
}
```

### Command behavior
1. Fetch both listening ports + connections (maximum coverage)
2. **Auto-detect target type**: try port number first via `filter_by_query`, then if no matches and target parses as u32, try direct PID lookup (check if PID exists in the port list). No extra `--pid` flag needed.
3. **Ambiguity note**: If a numeric target matches as a port AND a PID exists with that number, port wins. The output includes a hint: `"(also matches PID {target}, use a process name to target it)"` so the user knows.
4. Deduplicate by PID
5. For each unique PID, fetch ancestry and render

### Output format (table mode)
```
Process: node (PID 14233)
  Ports: 3000/tcp, 3001/tcp
  Source: systemd
  Unit:   node-app.service
  Chain:  systemd(1) -> node(14233)
  Git:    /home/user/my-app (main)
  Warnings: public-bind, deleted-binary
```

### Output format (JSON mode)
Array of objects with `pid`, `process_name`, `port`, `protocol`, `address`, and nested `ancestry` object containing `chain`, `source`, `warnings`, `git_context`, `systemd_unit`.

**Files**: NEW `src/commands/why.rs`, MODIFY `src/commands/mod.rs`, MODIFY `src/cli.rs`, MODIFY `src/lib.rs`

---

## Phase 3: Kill Command Warning

**Modify**: `src/commands/kill.rs`

Before the kill confirmation prompt, check ancestry for the target PID(s). If managed by systemd or launchd, print an informational note:

```
PID 1234 (nginx) listening on: 80, 443
  Note: Managed by systemd (nginx.service). Process will likely restart.
Kill? [y/N]:
```

This is a warning only — does NOT block the kill.

---

## Phase 4: `--why` Flag on Existing Commands

### CLI addition
```rust
// New global flag on Cli struct:
#[arg(long, global = true)]
pub why: bool,
```

### Output layer changes

**`src/output/table.rs`**: Add `print_ports_why(ports, ancestry_map)` that adds a SOURCE column to the table. The existing `print_ports()` and `print_ports_watch()` signatures remain unchanged (backward compatible).

**`src/output/json.rs`**: Add `print_ports_why(ports, ancestry_map)` that enriches each JSON object with nested `ancestry` data. Existing `print_ports()` is unchanged.

### Command handler changes

`commands/list.rs` and `commands/query.rs` get a new `why: bool` parameter. When true, compute ancestry batch for all PIDs in result set and call the `_why` output variants.

### Watch mode support

`--why` + `--watch` is allowed. The 10s ancestry cache TTL means ancestry is only recomputed every ~10 poll cycles (default 1s interval), so the performance cost is negligible. Watch mode passes the ancestry map through to `print_ports_watch` when `--why` is active — the SOURCE column appears alongside the existing new-port highlighting.

### Interactive mode support

`--why` + `-i` is allowed. Before the dialoguer selection prompt, ancestry is fetched for all displayed PIDs. Each item in the interactive picker includes the source type: `" 3000 tcp  14233 node [systemd]"`. After selection, the kill confirmation shows the supervisor warning if applicable (same as Phase 3).

**Files**: MODIFY `src/output/table.rs`, `src/output/json.rs`, `src/commands/list.rs`, `src/commands/query.rs`, `src/lib.rs`, `src/watch.rs` (pass ancestry to output), `src/interactive.rs` (show source in picker)

---

## Phase 5: TUI Integration

**Modify**: `src/top.rs`

### New state fields
```rust
detail_pid: Option<u32>,
detail_ancestry: Option<ProcessAncestry>,
```

### Key binding
- `Enter` on selected row: toggle ancestry detail popup (lazy-loaded, cached)
- `Enter` again or `Esc`: dismiss popup

### Detail popup
Renders as a centered overlay (reuses existing `centered_rect` pattern from kill confirmation) showing: Source, Chain, Unit/Label, Git context, Warnings.

### Footer help text update
```
q:Quit  Tab:Toggle  p/i/n:Sort  ↑↓/j/K:Nav  PgUp/PgDn:Page  Enter:Info  k:Kill
```

---

## Second/Third-Order Effects Addressed

| Effect | Mitigation |
|--------|-----------|
| **PortInfo Hash/Eq breakage** | Ancestry in parallel map, not on PortInfo |
| **History schema migration** | No ancestry in history DB (ephemeral data) |
| **Watch mode performance** | 10s cache TTL → recompute every ~10 cycles, not every cycle |
| **TUI refresh cost** | Ancestry loaded on Enter only, 10s cache TTL |
| **Docker container overlap** | `container` field (display name) and `SourceType::Docker` (provenance) are complementary |
| **macOS support gap** | Batch `ps -A` for chain walking; `launchctl` may fail without root (returns None) |
| **macOS batch perf** | Single `ps -A` call builds in-memory table; all chain walks use that table |
| **Permission failures** | Every /proc read returns `Option`; `build_ancestry()` returns `Option<ProcessAncestry>` |
| **PID reuse / cache staleness** | Cache entries store process_name; mismatch on hit → cache miss |
| **Source detection priority** | Tiered algorithm: cgroup > init system > supervisor > multiplexer > shell > cron > unknown |
| **systemd→bash→process chains** | Tier 2 (cgroup .service detection) fires before Tier 5 (shell), so systemd wins correctly |
| **Filter interaction** | `filter_by_query` NOT extended to search ancestry (expensive, opt-in data) |
| **JSON backward compat** | Default JSON unchanged; `--why` adds `ancestry` key (additive only) |
| **Table width (80 cols)** | SOURCE column max 12 chars; PROCESS column shrinks via `Fill(1)` |
| **History diff false positives** | N/A — ancestry not stored in history |
| **Kill restart warning** | Informational note only, not a blocker |
| **Crate size** | ~600 lines new code (across 5 files), zero new dependencies |
| **Git context for daemons** | Returns None for cwd=/ which is correct; useful for dev-mode processes only |
| **PID/port ambiguity** | Port wins; output hints if PID also matches |

---

## Explicit Scope Boundaries (NOT doing)

1. File lock detection
2. Full process TUI/dashboard
3. All 30+ supervisor detectors (12 supported: systemd, launchd, Docker, cron, shell, pm2, supervisord, gunicorn, runit, s6, tmux, screen, nohup)
4. Ancestry fields on PortInfo struct
5. Ancestry in history database
6. `--source` filter flag
7. `/proc/{pid}/environ` parsing (privacy/security)
8. Network namespace detection
9. Modifying existing PortInfo serialization

---

## Implementation Order

1. **Phase 1** — Core ancestry module (foundation, no existing code broken)
2. **Phase 2** — `ports why` subcommand (standalone test surface for ancestry)
3. **Phase 3** — Kill command warning (minimal change, immediate value)
4. **Phase 4** — `--why` flag on list/query/watch/interactive (output layer changes, moderate risk)
5. **Phase 5** — TUI integration (self-contained, popup pattern exists)

---

## Testing Strategy

### Unit tests (in `src/ancestry/`)
- `SourceType::Display` and `HealthWarning::Display` formatting
- `PortInfo::is_public_binding()` with various address formats (in `types.rs` tests)
- `walk_ppid_chain(current_pid)` returns non-empty chain (Linux)
- `walk_ppid_chain(1)` terminates without infinite loop (Linux)
- **Tiered source detection**: mock chain `[systemd, bash, node]` → returns Systemd not Shell
- **Tiered source detection**: mock chain `[launchd, Terminal, zsh, node]` → returns Shell (user-launched, no service unit)
- **Tiered source detection**: mock chain with cgroup containing `/docker/` → returns Docker regardless of chain names
- Cache hit/miss behavior
- Cache PID reuse protection (name mismatch → miss)
- Git context detection (non-panicking, returns None for `/` cwd)
- macOS process table batch parsing

### Integration tests (new `tests/why_integration.rs`)
- `ports why --help` shows help
- `ports why nonexistent_xyz` fails gracefully with "No process found"
- `ports why --json <target>` produces valid JSON
- `ports --why` succeeds (may be empty, but no error)
- `ports --why --watch` succeeds (no longer rejected)

### Manual verification
- `ports why 22` on a machine with sshd → shows systemd/launchd source
- `ports why node` on a dev machine → shows shell source + git context
- `ports --why` → table with SOURCE column
- `ports --why --watch` → live table with SOURCE column that updates
- `ports top` → Enter on a row shows ancestry popup
- `ports kill nginx` on systemd-managed service → shows restart warning

---

## Files Summary

| Action | File |
|--------|------|
| NEW | `src/ancestry/mod.rs` — public API, types, caching |
| NEW | `src/ancestry/linux.rs` — /proc chain walking, cgroup, warnings |
| NEW | `src/ancestry/macos.rs` — batch ps, launchctl |
| NEW | `src/ancestry/source.rs` — tiered detection algorithm |
| NEW | `src/ancestry/git.rs` — git context detection |
| NEW | `src/commands/why.rs` — why subcommand handler |
| NEW | `tests/why_integration.rs` — integration tests |
| MODIFY | `src/lib.rs` — add module, dispatch, --why wiring |
| MODIFY | `src/cli.rs` — add Why subcommand, --why flag |
| MODIFY | `src/types.rs` — add `is_public_binding()` method |
| MODIFY | `src/commands/mod.rs` — add pub mod why |
| MODIFY | `src/commands/list.rs` — accept why param |
| MODIFY | `src/commands/query.rs` — accept why param |
| MODIFY | `src/commands/kill.rs` — restart warning |
| MODIFY | `src/output/table.rs` — SOURCE column |
| MODIFY | `src/output/json.rs` — ancestry enrichment |
| MODIFY | `src/watch.rs` — pass ancestry to output when --why |
| MODIFY | `src/interactive.rs` — show source in picker |
| MODIFY | `src/top.rs` — Enter key detail popup |
| UNCHANGED | `src/history.rs` — no schema changes |
| UNCHANGED | `src/docker.rs` — no changes |
| UNCHANGED | `Cargo.toml` — no new dependencies |
