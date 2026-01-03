pub mod proc_fd;
pub mod proc_parser;

use std::fs;

use anyhow::{Context, Result};

use crate::types::{PortInfo, Protocol};
use proc_fd::build_inode_to_process_map;
use proc_parser::{parse_proc_net_tcp, TcpState};

pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    get_ports_by_state(|state| state == TcpState::Listen)
}

pub fn get_all_connections() -> Result<Vec<PortInfo>> {
    get_ports_by_state(|_| true)
}

pub fn get_established_connections() -> Result<Vec<PortInfo>> {
    get_ports_by_state(|state| state == TcpState::Established)
}

fn get_ports_by_state<F>(state_filter: F) -> Result<Vec<PortInfo>>
where
    F: Fn(TcpState) -> bool,
{
    let tcp_content =
        fs::read_to_string("/proc/net/tcp").context("Failed to read /proc/net/tcp")?;

    let sockets = parse_proc_net_tcp(&tcp_content);
    let inode_map = build_inode_to_process_map()?;

    let ports: Vec<PortInfo> = sockets
        .into_iter()
        .filter(|s| state_filter(s.state))
        .filter_map(|socket| {
            let process_info = inode_map.get(&socket.inode)?;

            Some(PortInfo {
                port: socket.local_port,
                protocol: Protocol::Tcp,
                pid: process_info.pid,
                process_name: process_info.name.clone(),
                address: format!("{}:{}", socket.local_addr, socket.local_port),
            })
        })
        .collect();

    Ok(ports)
}
