//! Process ancestry and source detection ("why is this running?").
//!
//! Answers the causality question: given a process bound to a port,
//! trace its parent chain and identify which supervisor or init system
//! is responsible for it.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

mod git;
mod source;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

// Re-export the tiered detection entry point for platform modules.
pub(crate) use source::detect_source;

/// A single process in the ancestry chain (ordered from target up to PID 1).
#[derive(Debug, Clone, Serialize)]
pub struct Ancestor {
    pub pid: u32,
    pub name: String,
    pub ppid: u32,
}

/// Detected source/supervisor type for a process.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Systemd,
    Launchd,
    Docker,
    Cron,
    Shell,
    Pm2,
    Supervisord,
    Gunicorn,
    Runit,
    S6,
    Tmux,
    Screen,
    Nohup,
    Unknown,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Systemd => write!(f, "systemd"),
            SourceType::Launchd => write!(f, "launchd"),
            SourceType::Docker => write!(f, "docker"),
            SourceType::Cron => write!(f, "cron"),
            SourceType::Shell => write!(f, "shell"),
            SourceType::Pm2 => write!(f, "pm2"),
            SourceType::Supervisord => write!(f, "supervisord"),
            SourceType::Gunicorn => write!(f, "gunicorn"),
            SourceType::Runit => write!(f, "runit"),
            SourceType::S6 => write!(f, "s6"),
            SourceType::Tmux => write!(f, "tmux"),
            SourceType::Screen => write!(f, "screen"),
            SourceType::Nohup => write!(f, "nohup"),
            SourceType::Unknown => write!(f, "unknown"),
        }
    }
}

/// Health warnings detected for a process.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthWarning {
    DeletedBinary,
    ZombieProcess,
}

impl std::fmt::Display for HealthWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthWarning::DeletedBinary => write!(f, "deleted-binary"),
            HealthWarning::ZombieProcess => write!(f, "zombie"),
        }
    }
}

/// Git context for a process working directory.
#[derive(Debug, Clone, Serialize)]
pub struct GitContext {
    pub repo_name: String,
    pub branch: Option<String>,
}

/// Full ancestry and source information for a process.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessAncestry {
    pub chain: Vec<Ancestor>,
    pub source: SourceType,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<HealthWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_context: Option<GitContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub systemd_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launchd_label: Option<String>,
}

// ── Caching ─────────────────────────────────────────────────────────────────

struct CacheEntry {
    ancestry: ProcessAncestry,
    process_name: String,
}

type AncestryCache = HashMap<u32, CacheEntry>;

static CACHE: LazyLock<Mutex<(Instant, AncestryCache)>> =
    LazyLock::new(|| Mutex::new((Instant::now(), HashMap::new())));

const CACHE_TTL: Duration = Duration::from_secs(10);

/// Get ancestry for a single PID, using cache.
///
/// `process_name` is used to validate cache entries against PID reuse —
/// if the cached name doesn't match, the entry is treated as stale.
pub fn get_ancestry(pid: u32, process_name: &str) -> Option<ProcessAncestry> {
    let mut guard = CACHE.lock().unwrap();
    let (ref mut last_refresh, ref mut map) = *guard;

    if last_refresh.elapsed() > CACHE_TTL {
        map.clear();
        *last_refresh = Instant::now();
    }

    if let Some(entry) = map.get(&pid) {
        if entry.process_name == process_name {
            return Some(entry.ancestry.clone());
        }
        // PID reuse detected — name mismatch, treat as miss.
        map.remove(&pid);
    }

    // Drop the lock before doing I/O.
    drop(guard);

    let ancestry = build_ancestry(pid)?;

    let mut guard = CACHE.lock().unwrap();
    guard.1.insert(
        pid,
        CacheEntry {
            ancestry: ancestry.clone(),
            process_name: process_name.to_string(),
        },
    );

    Some(ancestry)
}

/// Build ancestry for a batch of PIDs (more efficient for --why on list).
pub fn get_ancestry_batch(pids_with_names: &[(u32, &str)]) -> HashMap<u32, ProcessAncestry> {
    // On macOS, build the process table once before walking chains.
    #[cfg(target_os = "macos")]
    macos::ensure_process_table();

    let mut result = HashMap::new();
    for &(pid, name) in pids_with_names {
        if let Some(a) = get_ancestry(pid, name) {
            result.insert(pid, a);
        }
    }
    result
}

// ── Platform dispatch ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn build_ancestry(pid: u32) -> Option<ProcessAncestry> {
    linux::build_ancestry(pid)
}

#[cfg(target_os = "macos")]
fn build_ancestry(pid: u32) -> Option<ProcessAncestry> {
    macos::build_ancestry(pid)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn build_ancestry(_pid: u32) -> Option<ProcessAncestry> {
    None
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_type_display() {
        assert_eq!(SourceType::Systemd.to_string(), "systemd");
        assert_eq!(SourceType::Launchd.to_string(), "launchd");
        assert_eq!(SourceType::Docker.to_string(), "docker");
        assert_eq!(SourceType::Shell.to_string(), "shell");
        assert_eq!(SourceType::Pm2.to_string(), "pm2");
        assert_eq!(SourceType::Unknown.to_string(), "unknown");
        assert_eq!(SourceType::Tmux.to_string(), "tmux");
        assert_eq!(SourceType::Nohup.to_string(), "nohup");
    }

    #[test]
    fn test_health_warning_display() {
        assert_eq!(HealthWarning::DeletedBinary.to_string(), "deleted-binary");
        assert_eq!(HealthWarning::ZombieProcess.to_string(), "zombie");
    }

    #[test]
    fn test_source_type_serialization() {
        let json = serde_json::to_string(&SourceType::Systemd).unwrap();
        assert_eq!(json, "\"systemd\"");
        let json = serde_json::to_string(&SourceType::Pm2).unwrap();
        assert_eq!(json, "\"pm2\"");
    }

    #[test]
    fn test_cache_returns_none_for_nonexistent_pid() {
        // PID 0 should never exist as a user process
        let result = get_ancestry(0, "nonexistent");
        assert!(result.is_none());
    }
}
