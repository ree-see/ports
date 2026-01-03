use std::net::Ipv4Addr;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSocket {
    pub local_addr: Ipv4Addr,
    pub local_port: u16,
    pub remote_addr: Ipv4Addr,
    pub remote_port: u16,
    pub state: TcpState,
    pub inode: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
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

impl TcpState {
    fn from_hex(hex: &str) -> Result<Self> {
        let num = u8::from_str_radix(hex, 16).context("Invalid state hex")?;
        Ok(match num {
            1 => TcpState::Established,
            2 => TcpState::SynSent,
            3 => TcpState::SynRecv,
            4 => TcpState::FinWait1,
            5 => TcpState::FinWait2,
            6 => TcpState::TimeWait,
            7 => TcpState::Close,
            8 => TcpState::CloseWait,
            9 => TcpState::LastAck,
            10 => TcpState::Listen,
            11 => TcpState::Closing,
            n => TcpState::Unknown(n),
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

pub fn parse_tcp_line(line: &str) -> Result<RawSocket> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 10 {
        bail!("Invalid tcp line: not enough fields");
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
        local_addr: parse_hex_addr(local_addr_hex)?,
        local_port: parse_hex_port(local_port_hex)?,
        remote_addr: parse_hex_addr(remote_addr_hex)?,
        remote_port: parse_hex_port(remote_port_hex)?,
        state: TcpState::from_hex(state_hex)?,
        inode: inode_str.parse().context("Invalid inode")?,
    })
}

pub fn parse_proc_net_tcp(content: &str) -> Vec<RawSocket> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| parse_tcp_line(line).ok())
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
    fn test_tcp_state_listen() {
        let result = TcpState::from_hex("0A").unwrap();
        assert_eq!(result, TcpState::Listen);
    }

    #[test]
    fn test_tcp_state_established() {
        let result = TcpState::from_hex("01").unwrap();
        assert_eq!(result, TcpState::Established);
    }

    #[test]
    fn test_parse_tcp_line_listening_socket() {
        let line = "   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12345 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_tcp_line(line).unwrap();
        
        assert_eq!(result.local_addr, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(result.local_port, 8080);
        assert_eq!(result.remote_addr, Ipv4Addr::new(0, 0, 0, 0));
        assert_eq!(result.remote_port, 0);
        assert_eq!(result.state, TcpState::Listen);
        assert_eq!(result.inode, 12345);
    }

    #[test]
    fn test_parse_tcp_line_established_connection() {
        let line = "   1: 0100007F:1F90 0501A8C0:D431 01 00000000:00000000 00:00000000 00000000   500        0 12346 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_tcp_line(line).unwrap();
        
        assert_eq!(result.local_addr, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(result.local_port, 8080);
        assert_eq!(result.remote_addr, Ipv4Addr::new(192, 168, 1, 5));
        assert_eq!(result.remote_port, 54321);
        assert_eq!(result.state, TcpState::Established);
        assert_eq!(result.inode, 12346);
    }

    #[test]
    fn test_parse_proc_net_tcp_skips_header() {
        let content = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12345 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_proc_net_tcp(content);
        
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local_port, 8080);
    }

    #[test]
    fn test_parse_proc_net_tcp_multiple_lines() {
        let content = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   500        0 12345 1 0000000000000000 100 0 0 10 0
   1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12346 1 0000000000000000 100 0 0 10 0";
        
        let result = parse_proc_net_tcp(content);
        
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].local_port, 8080);
        assert_eq!(result[1].local_port, 80);
    }
}
