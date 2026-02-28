//! Linux-specific ancestry implementation using /proc.

use std::collections::HashSet;
use std::fs;

use super::git;
use super::{Ancestor, HealthWarning, ProcessAncestry};

/// Build full ancestry for a PID on Linux.
pub fn build_ancestry(pid: u32) -> Option<ProcessAncestry> {
    let chain = walk_ppid_chain(pid);
    if chain.is_empty() {
        return None;
    }

    let cgroup = read_cgroup(pid);
    let source = super::detect_source(&chain, cgroup.as_deref());
    let warnings = detect_warnings(pid);
    let systemd_unit = detect_systemd_unit(pid);
    let git_context = git::read_process_cwd(pid).and_then(|cwd| git::detect_git_context(&cwd));

    Some(ProcessAncestry {
        chain,
        source,
        warnings,
        git_context,
        systemd_unit,
        launchd_label: None,
    })
}

/// Walk the PPID chain from `pid` up toward PID 1.
///
/// Parses `/proc/{pid}/stat` for each hop. Uses a visited set for cycle
/// protection. Returns the chain ordered from target (index 0) to root.
fn walk_ppid_chain(pid: u32) -> Vec<Ancestor> {
    let mut chain = Vec::new();
    let mut current = pid;
    let mut visited = HashSet::new();

    loop {
        if visited.contains(&current) {
            break;
        }
        visited.insert(current);

        let (name, ppid, _state) = match read_proc_stat(current) {
            Some(info) => info,
            None => break,
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

/// Parse `/proc/{pid}/stat` and return (comm, ppid, state).
///
/// Format: `pid (comm) state ppid ...`
/// comm can contain spaces and parentheses, so we find the LAST `)`.
fn read_proc_stat(pid: u32) -> Option<(String, u32, char)> {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;

    let comm_start = stat.find('(')?;
    let comm_end = stat.rfind(')')?;
    let name = stat[comm_start + 1..comm_end].to_string();

    // Fields after ") ": state ppid ...
    let rest = stat.get(comm_end + 2..)?;
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid: u32 = fields.next()?.parse().ok()?;

    Some((name, ppid, state))
}

/// Read `/proc/{pid}/cgroup` for source detection.
fn read_cgroup(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{}/cgroup", pid)).ok()
}

/// Extract systemd unit name from cgroup.
///
/// Looks for patterns like `0::/system.slice/nginx.service` and
/// extracts `nginx.service`.
fn detect_systemd_unit(pid: u32) -> Option<String> {
    let cgroup = read_cgroup(pid)?;

    for line in cgroup.lines() {
        if let Some(path) = line.rsplit(':').next() {
            if let Some(unit) = path.rsplit('/').next() {
                if unit.ends_with(".service") {
                    return Some(unit.to_string());
                }
            }
        }
    }
    None
}

/// Detect health warnings for a process.
fn detect_warnings(pid: u32) -> Vec<HealthWarning> {
    let mut warnings = Vec::new();

    // Check for deleted binary.
    let exe_path = format!("/proc/{}/exe", pid);
    if let Ok(target) = fs::read_link(&exe_path) {
        if target.to_string_lossy().contains("(deleted)") {
            warnings.push(HealthWarning::DeletedBinary);
        }
    }

    // Check for zombie state.
    if let Some((_name, _ppid, state)) = read_proc_stat(pid) {
        if state == 'Z' {
            warnings.push(HealthWarning::ZombieProcess);
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk_ppid_chain_self() {
        let pid = std::process::id();
        let chain = walk_ppid_chain(pid);
        assert!(!chain.is_empty(), "Should find at least our own process");
        assert_eq!(chain[0].pid, pid);
    }

    #[test]
    fn test_walk_ppid_chain_terminates_at_pid1() {
        let chain = walk_ppid_chain(1);
        // PID 1 should produce exactly one entry (itself, ppid=0)
        assert!(chain.len() <= 1);
    }

    #[test]
    fn test_walk_ppid_chain_no_infinite_loop() {
        // Walk an arbitrary PID — should always terminate
        let chain = walk_ppid_chain(std::process::id());
        assert!(chain.len() < 100, "Chain should be reasonable length");
    }

    #[test]
    fn test_read_proc_stat_self() {
        let pid = std::process::id();
        let result = read_proc_stat(pid);
        assert!(result.is_some());
        let (name, ppid, state) = result.unwrap();
        assert!(!name.is_empty());
        assert!(ppid > 0);
        // Running or sleeping
        assert!(
            matches!(state, 'R' | 'S' | 'D'),
            "Expected R/S/D state, got {}",
            state
        );
    }

    #[test]
    fn test_read_proc_stat_nonexistent() {
        let result = read_proc_stat(0);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_warnings_self() {
        let warnings = detect_warnings(std::process::id());
        // Our own process shouldn't have warnings
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_build_ancestry_self() {
        let pid = std::process::id();
        let ancestry = build_ancestry(pid);
        assert!(ancestry.is_some());
        let a = ancestry.unwrap();
        assert!(!a.chain.is_empty());
        assert_eq!(a.chain[0].pid, pid);
    }
}
