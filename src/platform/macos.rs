// Compiled on Linux too (via `cfg(any(target_os = "macos", test))` upstream)
// so the macOS unit tests run cross-platform. The non-test, non-macOS build
// never calls these items, so dead-code analysis flags them on Linux.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

use crate::types::{PortInfo, Protocol};

pub fn get_connections() -> Result<Vec<PortInfo>> {
    let output = Command::new("lsof")
        .args(["-i", "-n", "-P"])
        .output()
        .context("Failed to execute lsof")?;

    let stdout = String::from_utf8(output.stdout).context("Invalid UTF-8 from lsof")?;

    Ok(parse_lsof_output(&stdout))
}

fn parse_lsof_output(output: &str) -> Vec<PortInfo> {
    output
        .lines()
        .skip(1)
        .filter_map(parse_lsof_line)
        .filter(|info| info.remote_address.is_some())
        .collect()
}

fn parse_lsof_line(line: &str) -> Option<PortInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 10 {
        return None;
    }

    let command = parts[0];
    let pid: u32 = parts[1].parse().ok()?;
    let protocol_type = parts[7];
    let name = parts[8];

    if !name.contains(':') {
        return None;
    }

    let protocol = match protocol_type {
        "TCP" => Protocol::Tcp,
        "UDP" => Protocol::Udp,
        _ => return None,
    };

    let port = extract_local_port(name)?;

    let (local_addr, remote_address) = if let Some((local, remote)) = name.split_once("->") {
        (local.to_string(), Some(remote.to_string()))
    } else {
        (name.to_string(), None)
    };

    Some(PortInfo {
        port,
        protocol,
        pid,
        process_name: command.to_string(),
        address: local_addr,
        remote_address,
        container: None,
        service_name: None,
        command_line: None,
        cwd: None,
        framework: None,
    })
}

fn extract_local_port(name: &str) -> Option<u16> {
    let local_part = if name.contains("->") {
        name.split("->").next()?
    } else {
        name.trim_end_matches(|c: char| c == ')' || c == '(' || c.is_alphabetic())
    };

    let port_str = local_part.rsplit(':').next()?;
    port_str.parse().ok()
}

/// Parse `ps -o pid=,args=` output into a PID-to-command map.
///
/// Each line has leading-space-padded PID followed by a space
/// and the full command string.
fn parse_ps_output(output: &str) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        // PID is the leading digits, command follows
        // the first space after the PID.
        let pid_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        if pid_end == 0 {
            continue; // no leading digits
        }
        let pid_str = &trimmed[..pid_end];
        let rest = &trimmed[pid_end..];
        if let Ok(pid) = pid_str.parse::<u32>() {
            // Skip the single space separator.
            let cmd = rest.strip_prefix(' ').unwrap_or(rest);
            if !cmd.is_empty() {
                map.insert(pid, cmd.to_string());
            }
        }
    }
    map
}

/// Parse `lsof -Fn` output for CWD file descriptors.
///
/// **Caller must pass `-d cwd`** to lsof so that only CWD
/// entries appear in the output. This parser captures every
/// `n` line without validating the preceding `f` line type.
fn parse_lsof_cwd_output(output: &str) -> HashMap<u32, PathBuf> {
    let mut map = HashMap::new();
    let mut current_pid: Option<u32> = None;
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        match line.as_bytes()[0] {
            b'p' => {
                current_pid = line[1..].parse::<u32>().ok();
            }
            b'n' => {
                if let Some(pid) = current_pid {
                    let path = &line[1..];
                    if !path.is_empty() {
                        map.insert(pid, PathBuf::from(path));
                    }
                }
            }
            // Skip 'f' and any other line types.
            _ => {}
        }
    }
    map
}

/// Resolve command lines for all PIDs in a single `ps` call.
///
/// Returns an empty map on subprocess failure.
fn batch_resolve_cmdlines(pids: &[u32]) -> HashMap<u32, String> {
    if pids.is_empty() {
        return HashMap::new();
    }
    let csv: String = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let output = Command::new("ps")
        .args(["-p", &csv, "-o", "pid=,args="])
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            parse_ps_output(&stdout)
        }
        Err(_) => HashMap::new(),
    }
}

/// Resolve working directories for all PIDs in one `lsof`.
///
/// Returns an empty map on subprocess failure.
// See also: ancestry::git::read_process_cwd
// (per-PID lsof, returns String)
fn batch_resolve_cwds(pids: &[u32]) -> HashMap<u32, PathBuf> {
    if pids.is_empty() {
        return HashMap::new();
    }
    let csv: String = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &csv, "-Fn"])
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            parse_lsof_cwd_output(&stdout)
        }
        Err(_) => HashMap::new(),
    }
}

/// Enrich `PortInfo` entries with command lines and CWDs.
///
/// Collects unique PIDs, runs one batched `ps` call and one
/// batched `lsof` call, then distributes results back.
pub fn resolve_process_details(ports: &mut [PortInfo]) {
    let pids: Vec<u32> = {
        let mut seen = std::collections::HashSet::new();
        ports
            .iter()
            .filter(|p| p.pid > 0)
            .filter_map(|p| {
                if seen.insert(p.pid) {
                    Some(p.pid)
                } else {
                    None
                }
            })
            .collect()
    };
    if pids.is_empty() {
        return;
    }

    let cmdlines = batch_resolve_cmdlines(&pids);
    let cwds = batch_resolve_cwds(&pids);

    for port in ports.iter_mut() {
        if let Some(cmd) = cmdlines.get(&port.pid) {
            port.command_line = Some(cmd.clone());
        }
        if let Some(cwd) = cwds.get(&port.pid) {
            port.cwd = Some(cwd.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lsof_line_established() {
        let line = "node      12345 user   23u  IPv4 0x1234567890abcdef      0t0  TCP 127.0.0.1:3000->192.168.1.5:54321 (ESTABLISHED)";

        let result = parse_lsof_line(line).unwrap();

        assert_eq!(result.process_name, "node");
        assert_eq!(result.pid, 12345);
        assert_eq!(result.port, 3000);
        assert_eq!(result.protocol, Protocol::Tcp);
        assert_eq!(result.address, "127.0.0.1:3000");
        assert_eq!(result.remote_address, Some("192.168.1.5:54321".to_string()));
    }

    #[test]
    fn test_parse_lsof_line_listen() {
        let line =
            "node      12345 user   24u  IPv4 0x1234567890abcdef      0t0  TCP *:3000 (LISTEN)";

        let result = parse_lsof_line(line).unwrap();

        assert_eq!(result.port, 3000);
        assert!(result.remote_address.is_none());
    }

    #[test]
    fn test_parse_lsof_output_filters_established() {
        let output = "COMMAND   PID USER  FD  TYPE DEVICE SIZE/OFF NODE NAME
node      12345 user   23u  IPv4 0x123      0t0  TCP 127.0.0.1:3000->192.168.1.5:54321 (ESTABLISHED)
node      12345 user   24u  IPv4 0x456      0t0  TCP *:3000 (LISTEN)";

        let result = parse_lsof_output(output);

        assert_eq!(result.len(), 1);
        assert!(result[0].remote_address.is_some());
    }

    #[test]
    fn test_extract_local_port_established() {
        assert_eq!(
            extract_local_port("127.0.0.1:3000->192.168.1.5:54321"),
            Some(3000)
        );
    }

    #[test]
    fn test_extract_local_port_listen() {
        assert_eq!(extract_local_port("*:8080"), Some(8080));
    }

    #[test]
    fn test_extract_local_port_ipv6() {
        assert_eq!(extract_local_port("[::1]:5432"), Some(5432));
    }

    // --- Parser tests (no #[cfg] gate: pure string parsing) ---

    #[test]
    fn test_parse_ps_output_normal() {
        let output = concat!(
            "  1234 /usr/bin/node server.js --port 3000\n",
            "  5678 /usr/local/bin/python3 app.py\n",
        );
        let map = parse_ps_output(output);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(&1234).unwrap(),
            "/usr/bin/node server.js --port 3000"
        );
        assert_eq!(map.get(&5678).unwrap(), "/usr/local/bin/python3 app.py");
    }

    #[test]
    fn test_parse_ps_output_empty() {
        let map = parse_ps_output("");
        assert!(map.is_empty());
    }

    #[test]
    fn test_parse_ps_output_malformed_line() {
        // Lines without a leading PID should be skipped.
        let output = concat!(
            "COMMAND not a real line\n",
            "  999 /bin/sleep 60\n",
            "  abc not-a-pid\n",
        );
        let map = parse_ps_output(output);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&999).unwrap(), "/bin/sleep 60");
    }

    #[test]
    fn test_parse_lsof_cwd_output_normal() {
        let output = concat!(
            "p1234\n",
            "fcwd\n",
            "n/Users/dev/project\n",
            "p5678\n",
            "fcwd\n",
            "n/tmp\n",
        );
        let map = parse_lsof_cwd_output(output);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(&1234).unwrap(),
            &PathBuf::from("/Users/dev/project")
        );
        assert_eq!(map.get(&5678).unwrap(), &PathBuf::from("/tmp"));
    }

    #[test]
    fn test_parse_lsof_cwd_output_empty() {
        let map = parse_lsof_cwd_output("");
        assert!(map.is_empty());
    }

    // --- Integration test (macOS only) ---

    #[cfg(target_os = "macos")]
    #[test]
    fn test_resolve_own_process_macos() {
        let our_pid = std::process::id();
        let mut ports = vec![PortInfo {
            port: 9999,
            protocol: Protocol::Tcp,
            pid: our_pid,
            process_name: "test".to_string(),
            address: "127.0.0.1:9999".to_string(),
            remote_address: None,
            container: None,
            service_name: None,
            command_line: None,
            cwd: None,
            framework: None,
        }];
        resolve_process_details(&mut ports);
        let has_detail = ports[0].command_line.is_some() || ports[0].cwd.is_some();
        assert!(
            has_detail,
            "Expected at least command_line or cwd \
             to be populated for our own process"
        );
    }
}
