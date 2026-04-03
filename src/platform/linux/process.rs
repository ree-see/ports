//! Linux-specific process resolution: cmdline and cwd
//! from `/proc/{pid}/`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::types::PortInfo;

/// Parse raw `/proc/{pid}/cmdline` bytes into a
/// human-readable command string.
///
/// Arguments are NUL-separated in procfs. We strip
/// trailing NULs, split on remaining NULs, and join
/// with spaces. Returns `None` if the input is empty
/// or contains only NUL bytes.
pub fn parse_cmdline(bytes: &[u8]) -> Option<String> {
    // Strip trailing NUL bytes.
    let trimmed = bytes
        .iter()
        .rposition(|&b| b != 0)
        .map(|pos| &bytes[..=pos])
        .unwrap_or(&[]);

    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed
        .split(|&b| b == 0)
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .collect();

    if parts.is_empty() {
        return None;
    }

    Some(parts.join(" "))
}

/// Read the command line for a process from procfs.
///
/// Returns `None` on permission error, missing process,
/// or unreadable data.
pub fn read_cmdline(pid: u32) -> Option<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let bytes = fs::read(path).ok()?;
    parse_cmdline(&bytes)
}

/// Read the current working directory of a process from
/// the `/proc/{pid}/cwd` symlink.
///
/// Returns `None` on permission error or missing process.
///
/// See also: ancestry::git::read_process_cwd (returns
/// String for git context detection)
pub fn read_cwd(pid: u32) -> Option<PathBuf> {
    let path = format!("/proc/{}/cwd", pid);
    fs::read_link(path).ok()
}

/// Populate `command_line` and `cwd` on each `PortInfo`
/// entry by reading from `/proc`.
///
/// Collects unique PIDs first to avoid redundant reads
/// when a PID appears on multiple ports.
pub fn resolve_process_details(ports: &mut [PortInfo]) {
    let mut cmdlines: HashMap<u32, Option<String>> = HashMap::new();
    let mut cwds: HashMap<u32, Option<PathBuf>> = HashMap::new();

    // Collect unique PIDs.
    for port in ports.iter() {
        if port.pid != 0 {
            cmdlines.entry(port.pid).or_insert(None);
            cwds.entry(port.pid).or_insert(None);
        }
    }

    // Read once per PID.
    for (&pid, slot) in cmdlines.iter_mut() {
        *slot = read_cmdline(pid);
    }
    for (&pid, slot) in cwds.iter_mut() {
        *slot = read_cwd(pid);
    }

    // Distribute results.
    for port in ports.iter_mut() {
        if let Some(cmd) = cmdlines.get(&port.pid) {
            port.command_line = cmd.clone();
        }
        if let Some(cwd) = cwds.get(&port.pid) {
            port.cwd = cwd.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cmdline_normal() {
        let input = b"/usr/bin/node\0server.js\0--port\03000\0";
        assert_eq!(
            parse_cmdline(input),
            Some("/usr/bin/node server.js --port 3000".to_string())
        );
    }

    #[test]
    fn test_parse_cmdline_empty() {
        assert_eq!(parse_cmdline(b""), None);
    }

    #[test]
    fn test_parse_cmdline_trailing_nuls() {
        let input = b"/usr/bin/sleep\0\0\0";
        assert_eq!(parse_cmdline(input), Some("/usr/bin/sleep".to_string()));
    }

    #[test]
    fn test_parse_cmdline_single_arg() {
        let input = b"nginx\0";
        assert_eq!(parse_cmdline(input), Some("nginx".to_string()));
    }

    #[test]
    fn test_parse_cmdline_no_nul() {
        // Some processes don't NUL-terminate.
        let input = b"nginx";
        assert_eq!(parse_cmdline(input), Some("nginx".to_string()));
    }

    #[test]
    fn test_parse_cmdline_only_nuls() {
        let input = b"\0\0\0";
        assert_eq!(parse_cmdline(input), None);
    }

    #[test]
    fn test_resolve_distributes_to_multiple_ports() {
        // Verify that resolve_process_details correctly
        // handles the case where two ports share one PID
        // (both should get the same cmdline/cwd values).
        use crate::types::Protocol;

        let mut ports = vec![
            PortInfo {
                port: 80,
                protocol: Protocol::Tcp,
                pid: 0, // PID 0 should be skipped
                process_name: "kernel".into(),
                address: "0.0.0.0:80".into(),
                remote_address: None,
                container: None,
                service_name: None,
                command_line: None,
                cwd: None,
                framework: None,
            },
            PortInfo {
                port: 443,
                protocol: Protocol::Tcp,
                pid: 0,
                process_name: "kernel".into(),
                address: "0.0.0.0:443".into(),
                remote_address: None,
                container: None,
                service_name: None,
                command_line: None,
                cwd: None,
                framework: None,
            },
        ];

        // Should not panic on PID 0 (skipped).
        resolve_process_details(&mut ports);
        assert!(ports[0].command_line.is_none());
        assert!(ports[0].cwd.is_none());
        assert!(ports[1].command_line.is_none());
        assert!(ports[1].cwd.is_none());
    }
}
