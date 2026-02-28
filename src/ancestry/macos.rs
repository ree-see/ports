//! macOS-specific ancestry implementation using ps and launchctl.

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::git;
use super::{Ancestor, ProcessAncestry};

/// A cached snapshot of the full macOS process table.
struct ProcessTable {
    /// Map from PID to (name, ppid).
    entries: Arc<HashMap<u32, (String, u32)>>,
    fetched_at: Instant,
}

static PROCESS_TABLE: LazyLock<Mutex<Option<ProcessTable>>> = LazyLock::new(|| Mutex::new(None));

const TABLE_TTL: Duration = Duration::from_secs(5);

/// Ensure a fresh process table exists (for batch operations).
///
/// Returns a guard that callers can drop immediately — the table is
/// accessed through `walk_ppid_chain` which re-locks internally.
pub fn ensure_process_table() {
    let _ = get_or_refresh_table();
}

/// Build full ancestry for a PID on macOS.
pub fn build_ancestry(pid: u32) -> Option<ProcessAncestry> {
    let chain = walk_ppid_chain(pid);
    if chain.is_empty() {
        return None;
    }

    let source = super::detect_source(&chain, None);
    let warnings = Vec::new(); // macOS: no deleted-binary or zombie detection via ps
    let launchd_label = detect_launchd_label(pid);
    let git_context = git::read_process_cwd(pid).and_then(|cwd| git::detect_git_context(&cwd));

    Some(ProcessAncestry {
        chain,
        source,
        warnings,
        git_context,
        systemd_unit: None,
        launchd_label,
    })
}

/// Walk the PPID chain from `pid` up toward PID 1 using the cached process table.
///
/// Falls back to per-PID `ps` calls if the table can't be built.
fn walk_ppid_chain(pid: u32) -> Vec<Ancestor> {
    let table = get_or_refresh_table();
    let mut chain = Vec::new();
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();

    loop {
        if visited.contains(&current) {
            break;
        }
        visited.insert(current);

        let (name, ppid) = match table.get(&current) {
            Some((n, p)) => (n.clone(), *p),
            None => match read_single_process(current) {
                Some((n, p)) => (n, p),
                None => break,
            },
        };

        chain.push(Ancestor {
            pid: current,
            name,
            ppid,
        });

        if ppid == 0 || current == 1 {
            break;
        }
        current = ppid;
    }

    chain
}

/// Build or return a cached process table from `ps -A -o pid=,ppid=,comm=`.
fn get_or_refresh_table() -> Arc<HashMap<u32, (String, u32)>> {
    let mut guard = PROCESS_TABLE.lock().unwrap();

    if let Some(ref table) = *guard {
        if table.fetched_at.elapsed() < TABLE_TTL {
            return Arc::clone(&table.entries);
        }
    }

    let entries = Arc::new(build_process_table());
    *guard = Some(ProcessTable {
        entries: Arc::clone(&entries),
        fetched_at: Instant::now(),
    });

    entries
}

/// Parse `ps -A -o pid=,ppid=,comm=` into a HashMap.
fn build_process_table() -> HashMap<u32, (String, u32)> {
    let mut map = HashMap::new();

    let output = match Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,comm="])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return map,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Format: "  PID  PPID /path/to/comm" — variable whitespace between columns.
        // Use split_whitespace to skip runs of spaces, then collect the rest as comm.
        let mut tokens = trimmed.split_whitespace();
        let pid_str = match tokens.next() {
            Some(s) => s,
            None => continue,
        };
        let ppid_str = match tokens.next() {
            Some(s) => s,
            None => continue,
        };
        // The command may contain spaces; collect everything remaining.
        let comm: String = tokens.collect::<Vec<&str>>().join(" ");
        if comm.is_empty() {
            continue;
        }

        if let (Ok(pid), Ok(ppid)) = (pid_str.parse::<u32>(), ppid_str.parse::<u32>()) {
            // Extract just the binary name from the full path.
            let name = comm.rsplit('/').next().unwrap_or(&comm).to_string();
            map.insert(pid, (name, ppid));
        }
    }

    map
}

/// Fallback: read a single process via `ps -o pid=,ppid=,comm= -p <pid>`.
fn read_single_process(pid: u32) -> Option<(String, u32)> {
    let output = Command::new("ps")
        .args(["-o", "pid=,ppid=,comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();

    let mut tokens = line.split_whitespace();
    let _pid_str = tokens.next()?;
    let ppid_str = tokens.next()?;
    let comm: String = tokens.collect::<Vec<&str>>().join(" ");
    if comm.is_empty() {
        return None;
    }

    let ppid: u32 = ppid_str.parse().ok()?;
    let name = comm.rsplit('/').next().unwrap_or(&comm).to_string();

    Some((name, ppid))
}

/// Try to detect a launchd label for the given PID via `launchctl procinfo`.
fn detect_launchd_label(pid: u32) -> Option<String> {
    let output = Command::new("launchctl")
        .args(["procinfo", &pid.to_string()])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("label = ") {
            return Some(trimmed.trim_start_matches("label = ").to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_process_table_not_empty() {
        let table = build_process_table();
        assert!(!table.is_empty(), "Process table should have entries");
        // PID 1 (launchd) should always exist.
        assert!(
            table.contains_key(&1),
            "PID 1 should exist in process table"
        );
    }

    #[test]
    fn test_walk_ppid_chain_self() {
        let pid = std::process::id();
        let chain = walk_ppid_chain(pid);
        assert!(!chain.is_empty(), "Should find at least our own process");
        assert_eq!(chain[0].pid, pid);
    }

    #[test]
    fn test_walk_ppid_chain_terminates() {
        let chain = walk_ppid_chain(std::process::id());
        assert!(chain.len() < 100, "Chain should be reasonable length");
        // Chain should contain at least 2 entries (self + parent).
        assert!(chain.len() >= 2, "Chain should have at least 2 entries");
    }

    #[test]
    fn test_build_ancestry_self() {
        let pid = std::process::id();
        let ancestry = build_ancestry(pid);
        assert!(ancestry.is_some());
        let a = ancestry.unwrap();
        assert!(!a.chain.is_empty());
        assert_eq!(a.chain[0].pid, pid);
        // On macOS, should have no systemd unit.
        assert!(a.systemd_unit.is_none());
    }

    #[test]
    fn test_read_single_process_self() {
        let pid = std::process::id();
        let result = read_single_process(pid);
        assert!(result.is_some());
        let (name, ppid) = result.unwrap();
        assert!(!name.is_empty());
        assert!(ppid > 0);
    }

    #[test]
    fn test_read_single_process_nonexistent() {
        // PID 0 is the kernel, not readable via ps in the same way
        let result = read_single_process(99999999);
        assert!(result.is_none());
    }
}
