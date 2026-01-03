use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSocket {
    pub local_addr: IpAddr,
    pub local_port: u16,
    pub remote_addr: IpAddr,
    pub remote_port: u16,
    pub state: SocketState,
    pub inode: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Established,
    SynSent,
    SynRecv,
    FinWait1,
    FinWait2,
    TimeWait,
    Close,
    CloseWait,
    LastAck,
    Listen,
    Closing,
    Unknown(u8),
}

impl SocketState {
    fn from_hex(hex: &str) -> Result<Self> {
        let num = u8::from_str_radix(hex, 16).context("Invalid state hex")?;
        Ok(match num {
            1 => SocketState::Established,
            2 => SocketState::SynSent,
            3 => SocketState::SynRecv,
            4 => SocketState::FinWait1,
            5 => SocketState::FinWait2,
            6 => SocketState::TimeWait,
            7 => SocketState::Close,
            8 => SocketState::CloseWait,
            9 => SocketState::LastAck,
            10 => SocketState::Listen,
            11 => SocketState::Closing,
            n => SocketState::Unknown(n),
        })
    }
}

pub fn parse_hex_addr(hex: &str) -> Result<Ipv4Addr> {
    let num = u32::from_str_radix(hex, 16).context("Invalid address hex")?;
    Ok(Ipv4Addr::new(
        (num & 0xFF) as u8,
        ((num >> 8) & 0xFF) as u8,
        ((num >> 16) & 0xFF) as u8,
        ((num >> 24) & 0xFF) as u8,
    ))
}

pub fn parse_hex_port(hex: &str) -> Result<u16> {
    u16::from_str_radix(hex, 16).context("Invalid port hex")
}

pub fn parse_hex_addr_v6(hex: &str) -> Result<Ipv6Addr> {
    if hex.len() != 32 {
        bail!("IPv6 address must be 32 hex chars, got {}", hex.len());
    }

    let mut octets = [0u8; 16];

    for i in 0..4 {
        let word_hex = &hex[i * 8..(i + 1) * 8];
        let word = u32::from_str_radix(word_hex, 16).context("Invalid IPv6 hex")?;

        let base = i * 4;
        octets[base] = (word & 0xFF) as u8;
        octets[base + 1] = ((word >> 8) & 0xFF) as u8;
        octets[base + 2] = ((word >> 16) & 0xFF) as u8;
        octets[base + 3] = ((word >> 24) & 0xFF) as u8;
    }

    Ok(Ipv6Addr::from(octets))
}

pub fn parse_hex_addr_any(hex: &str) -> Result<IpAddr> {
    match hex.len() {
        8 => Ok(IpAddr::V4(parse_hex_addr(hex)?)),
        32 => Ok(IpAddr::V6(parse_hex_addr_v6(hex)?)),
        n => bail!("Invalid address length: {} (expected 8 or 32)", n),
    }
}

pub fn parse_socket_line(line: &str) -> Result<RawSocket> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 10 {
        bail!("Invalid socket line: not enough fields");
    }

    let local = parts[1];
    let remote = parts[2];
    let state_hex = parts[3];
    let inode_str = parts[9];

    let (local_addr_hex, local_port_hex) = local
        .split_once(':')
        .context("Invalid local address format")?;
    let (remote_addr_hex, remote_port_hex) = remote
        .split_once(':')
        .context("Invalid remote address format")?;

    Ok(RawSocket {
        local_addr: parse_hex_addr_any(local_addr_hex)?,
        local_port: parse_hex_port(local_port_hex)?,
        remote_addr: parse_hex_addr_any(remote_addr_hex)?,
        remote_port: parse_hex_port(remote_port_hex)?,
        state: SocketState::from_hex(state_hex)?,
        inode: inode_str.parse().context("Invalid inode")?,
    })
}

pub fn parse_proc_net_file(content: &str) -> Vec<RawSocket> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| parse_socket_line(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_addr_localhost() {
        let result = parse_hex_addr("0100007F").unwrap();
        assert_eq!(result, Ipv4Addr::new(127, 0, 0, 1));
    }

    #[test]
    fn test_parse_hex_addr_any() {
        let result = parse_hex_addr("00000000").unwrap();
        assert_eq!(result, Ipv4Addr::new(0, 0, 0, 0));
    }

    #[test]
    fn test_parse_hex_addr_192_168_1_5() {
        let result = parse_hex_addr("0501A8C0").unwrap();
        assert_eq!(result, Ipv4Addr::new(192, 168, 1, 5));
    }

    #[test]
    fn test_parse_hex_port_8080() {
        let result = parse_hex_port("1F90").unwrap();
        assert_eq!(result, 8080);
    }

    #[test]
    fn test_parse_hex_port_443() {
        let result = parse_hex_port("01BB").unwrap();
        assert_eq!(result, 443);
    }

    #[test]
    fn test_socket_state_listen() {
        let result = SocketState::from_hex("0A").unwrap();
        assert_eq!(result, SocketState::Listen);
    }

    #[test]
    fn test_socket_state_established() {
        let result = SocketState::from_hex("01").unwrap();
        assert_eq!(result, SocketState::Established);
    }

    #[test]
    fn test_parse_socket_line_listening_ipv4() {
        let line = "   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12345 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_socket_line(line).unwrap();
        
        assert_eq!(result.local_addr, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(result.local_port, 8080);
        assert_eq!(result.remote_addr, IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
        assert_eq!(result.remote_port, 0);
        assert_eq!(result.state, SocketState::Listen);
        assert_eq!(result.inode, 12345);
    }

    #[test]
    fn test_parse_socket_line_established_ipv4() {
        let line = "   1: 0100007F:1F90 0501A8C0:D431 01 00000000:00000000 00:00000000 00000000   500        0 12346 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_socket_line(line).unwrap();
        
        assert_eq!(result.local_addr, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(result.local_port, 8080);
        assert_eq!(result.remote_addr, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)));
        assert_eq!(result.remote_port, 54321);
        assert_eq!(result.state, SocketState::Established);
        assert_eq!(result.inode, 12346);
    }

    #[test]
    fn test_parse_socket_line_ipv6_listening() {
        let line = "   0: 00000000000000000000000001000000:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12347 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_socket_line(line).unwrap();
        
        assert_eq!(result.local_addr, IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
        assert_eq!(result.local_port, 8080);
        assert_eq!(result.state, SocketState::Listen);
    }

    #[test]
    fn test_parse_proc_net_file_skips_header() {
        let content = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12345 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_proc_net_file(content);
        
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local_port, 8080);
    }

    #[test]
    fn test_parse_proc_net_file_multiple_lines() {
        let content = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12345 1 0000000000000000 100 0 0 10 0
   1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12346 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_proc_net_file(content);
        
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].local_port, 8080);
        assert_eq!(result[1].local_port, 80);
    }

    #[test]
    fn test_parse_hex_addr_v6_loopback() {
        let result = parse_hex_addr_v6("00000000000000000000000001000000").unwrap();
        assert_eq!(result, Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
    }

    #[test]
    fn test_parse_hex_addr_v6_any() {
        let result = parse_hex_addr_v6("00000000000000000000000000000000").unwrap();
        assert_eq!(result, Ipv6Addr::UNSPECIFIED);
    }

    #[test]
    fn test_parse_hex_addr_v6_ipv4_mapped() {
        let result = parse_hex_addr_v6("0000000000000000FFFF00000100007F").unwrap();
        assert_eq!(result, "::ffff:127.0.0.1".parse::<Ipv6Addr>().unwrap());
    }

    #[test]
    fn test_parse_hex_addr_v6_invalid_length() {
        let result = parse_hex_addr_v6("0100007F");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hex_addr_any_ipv4() {
        let result = parse_hex_addr_any("0100007F").unwrap();
        assert_eq!(result, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn test_parse_hex_addr_any_ipv6() {
        let result = parse_hex_addr_any("00000000000000000000000001000000").unwrap();
        assert_eq!(result, IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
    }
}
