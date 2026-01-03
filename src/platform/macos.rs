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
        let line = "node      12345 user   24u  IPv4 0x1234567890abcdef      0t0  TCP *:3000 (LISTEN)";
        
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
        assert_eq!(extract_local_port("127.0.0.1:3000->192.168.1.5:54321"), Some(3000));
    }

    #[test]
    fn test_extract_local_port_listen() {
        assert_eq!(extract_local_port("*:8080"), Some(8080));
    }

    #[test]
    fn test_extract_local_port_ipv6() {
        assert_eq!(extract_local_port("[::1]:5432"), Some(5432));
    }
}
