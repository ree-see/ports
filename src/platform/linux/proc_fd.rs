use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
}

pub fn build_inode_to_process_map() -> Result<HashMap<u64, ProcessInfo>> {
    let mut map = HashMap::new();

    let proc_dir = fs::read_dir("/proc").context("Failed to read /proc")?;

    for entry in proc_dir.flatten() {
        let pid_str = entry.file_name();
        let pid_str = pid_str.to_string_lossy();

        if let Ok(pid) = pid_str.parse::<u32>() {
            if let Ok(process_info) = get_process_sockets(pid) {
                for inode in process_info.inodes {
                    map.insert(
                        inode,
                        ProcessInfo {
                            pid,
                            name: process_info.name.clone(),
                        },
                    );
                }
            }
        }
    }

    Ok(map)
}

struct ProcessSockets {
    name: String,
    inodes: Vec<u64>,
}

fn get_process_sockets(pid: u32) -> Result<ProcessSockets> {
    let name = read_process_name(pid)?;
    let inodes = read_socket_inodes(pid)?;

    Ok(ProcessSockets { name, inodes })
}

/// Known interpreters where comm name is more informative than binary name
const INTERPRETERS: &[&str] = &[
    "node", "python", "python3", "ruby", "perl", "php", "java", "bash", "sh", "zsh",
];

fn read_process_name(pid: u32) -> Result<String> {
    // Try exe symlink first for the full binary name (comm is limited to 15 chars)
    let exe_path = format!("/proc/{}/exe", pid);
    let exe_name = fs::read_link(&exe_path).ok().and_then(|exe| {
        exe.file_name().and_then(|name| {
            let name = name.to_string_lossy();
            if name.contains("(deleted)") || name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
    });

    // Read comm for comparison
    let comm_path = format!("/proc/{}/comm", pid);
    let comm_name = fs::read_to_string(&comm_path)
        .ok()
        .map(|s| s.trim().to_string());

    match (exe_name, comm_name) {
        // If exe is an interpreter, prefer comm (e.g., "node" -> "openclaw-gatewa")
        (Some(exe), Some(comm)) if INTERPRETERS.contains(&exe.as_str()) => Ok(comm),
        // Otherwise use exe (full name, no 15 char limit)
        (Some(exe), _) => Ok(exe),
        // Fall back to comm
        (None, Some(comm)) => Ok(comm),
        // Last resort
        (None, None) => anyhow::bail!("Could not read process name"),
    }
}

fn read_socket_inodes(pid: u32) -> Result<Vec<u64>> {
    let fd_path = format!("/proc/{}/fd", pid);
    let fd_dir = fs::read_dir(&fd_path).context("Failed to read fd dir")?;

    let mut inodes = Vec::new();

    for entry in fd_dir.flatten() {
        if let Ok(link_target) = fs::read_link(entry.path()) {
            if let Some(inode) = parse_socket_link(&link_target) {
                inodes.push(inode);
            }
        }
    }

    Ok(inodes)
}

fn parse_socket_link(path: &Path) -> Option<u64> {
    let s = path.to_string_lossy();

    if s.starts_with("socket:[") && s.ends_with(']') {
        let inode_str = &s[8..s.len() - 1];
        inode_str.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_socket_link_valid() {
        let path = PathBuf::from("socket:[12345]");
        assert_eq!(parse_socket_link(&path), Some(12345));
    }

    #[test]
    fn test_parse_socket_link_large_inode() {
        let path = PathBuf::from("socket:[9876543210]");
        assert_eq!(parse_socket_link(&path), Some(9876543210));
    }

    #[test]
    fn test_parse_socket_link_not_socket() {
        let path = PathBuf::from("/dev/null");
        assert_eq!(parse_socket_link(&path), None);
    }

    #[test]
    fn test_parse_socket_link_pipe() {
        let path = PathBuf::from("pipe:[12345]");
        assert_eq!(parse_socket_link(&path), None);
    }

    #[test]
    fn test_parse_socket_link_anon_inode() {
        let path = PathBuf::from("anon_inode:[eventfd]");
        assert_eq!(parse_socket_link(&path), None);
    }
}
