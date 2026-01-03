# AGENTS.md - ports CLI Tool

> Modern cross-platform port inspector in Rust. Replacement for ss/netstat/lsof.

## Build Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo run -- 3000              # Run with arguments
cargo check                    # Type checking only (fast)
cargo clippy -- -D warnings    # Lints (MUST pass before commit)
cargo fmt                      # Format all code
cargo fmt -- --check           # Check formatting (CI mode)
```

## Test Commands

```bash
cargo test                             # Run all tests
cargo test test_parse_hex_addr         # Run single test by name
cargo test test_name -- --exact        # Exact match
cargo test proc_parser::tests          # All tests in module
cargo test -- --nocapture              # Show println! output
cargo test --lib                       # Unit tests only
cargo test --test integration          # Integration tests only
```

## Git Workflow

### Branch Strategy
- `main` - Production-ready code only. Protected.
- `dev` - Integration branch. All features merge here first.
- `feature/*`, `fix/*` - Branches off `dev`

### Atomic Commits
Each commit must be: **single purpose**, **self-contained** (builds + tests pass), **reversible**.

```bash
# Good: Atomic commits
git commit -m "feat: add TcpEntry struct for /proc/net/tcp parsing"
git commit -m "feat: implement parse_hex_addr for IP conversion"
git commit -m "test: add unit tests for parse_hex_addr"

# Bad: Mixed concerns
git commit -m "Add parsing and tests and fix formatting"
```

**Commit format**: `<type>: <description>` — Types: feat, fix, refactor, test, docs, chore

**Merge flow**: `feature/* -> dev` (squash/rebase), `dev -> main` (merge commit)

## Development Strategy: TDD

1. **Red**: Write failing test first
2. **Green**: Write minimal code to pass
3. **Refactor**: Clean up while tests pass

**Test naming**: `test_<function>_<scenario>_<expected>` (e.g., `test_parse_hex_addr_localhost_returns_127_0_0_1`)

## Code Style

### Imports
```rust
// Order: std -> external crates -> crate modules (blank line between groups)
use std::fs;
use anyhow::{Context, Result};
use crate::types::PortInfo;
```

### Types & Naming
- `u16` for ports, `u32` for PIDs, `u64` for inodes
- Structs: `PascalCase`, functions: `snake_case`, constants: `SCREAMING_SNAKE_CASE`
- Avoid `unwrap()` in library code; OK in tests
- Use `anyhow::Result` for application code

### Error Handling
```rust
use anyhow::{Context, Result, bail};

fn read_proc_file() -> Result<String> {
    fs::read_to_string("/proc/net/tcp")
        .context("Failed to read /proc/net/tcp")?
}

if port == 0 { bail!("Port cannot be zero"); }
```

### Documentation
```rust
/// Brief one-line description.
///
/// # Errors
/// Returns error if file cannot be read.
pub fn parse_hex_addr(hex: &str) -> Result<Ipv4Addr> { }
```

### Platform-Specific Code
```rust
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("Unsupported platform");
```

## Project Structure

```
ports/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point (~10 lines)
│   ├── lib.rs               # Public API, re-exports
│   ├── cli.rs               # Clap argument definitions
│   ├── types.rs             # Core data structures
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── list.rs          # List all ports
│   │   ├── query.rs         # Query by port/process
│   │   └── kill.rs          # Kill process
│   ├── output/
│   │   ├── mod.rs
│   │   └── table.rs         # Table formatting
│   └── platform/
│       ├── mod.rs           # Platform abstraction
│       ├── linux/           # Linux /proc implementation
│       └── fallback.rs      # listeners crate fallback
└── tests/
    └── integration.rs
```

## CI Requirements (Pre-commit)

Before pushing, ensure:
```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
```

## Dependencies Policy

- Prefer well-maintained crates (check last update, downloads)
- Core deps: `clap`, `anyhow`, `colored`, `comfy-table`, `listeners`
- Minimize dependency count; justify additions in PR
