pub mod proc_fd;
pub mod proc_parser;

use std::fs;
use std::net::IpAddr;

use anyhow::Result;

use crate::types::{PortInfo, Protocol};
use proc_fd::build_inode_to_process_map;
use proc_parser::{parse_proc_net_file, RawSocket, SocketState};

#[derive(Clone, Copy)]
enum FilterMode {
    Listening,
    Established,
    All,
}

pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    get_ports(FilterMode::Listening)
}

pub fn get_all_connections() -> Result<Vec<PortInfo>> {
    get_ports(FilterMode::All)
}

pub fn get_established_connections() -> Result<Vec<PortInfo>> {
    get_ports(FilterMode::Established)
}

fn is_remote_zero(socket: &RawSocket) -> bool {
    socket.remote_port == 0
        && match socket.remote_addr {
            IpAddr::V4(addr) => addr.is_unspecified(),
            IpAddr::V6(addr) => addr.is_unspecified(),
        }
}

fn should_include(socket: &RawSocket, mode: FilterMode, is_udp: bool) -> bool {
    match mode {
        FilterMode::All => true,
        FilterMode::Listening => {
            if is_udp {
                is_remote_zero(socket)
            } else {
                socket.state == SocketState::Listen
            }
        }
        FilterMode::Established => {
            if is_udp {
                !is_remote_zero(socket)
            } else {
                socket.state == SocketState::Established
            }
        }
    }
}

fn get_ports(mode: FilterMode) -> Result<Vec<PortInfo>> {
    let inode_map = build_inode_to_process_map()?;
    let mut ports = Vec::new();

    for (path, protocol) in [
        ("/proc/net/tcp", Protocol::Tcp),
        ("/proc/net/tcp6", Protocol::Tcp),
        ("/proc/net/udp", Protocol::Udp),
        ("/proc/net/udp6", Protocol::Udp),
    ] {
        let is_udp = protocol == Protocol::Udp;

        if let Ok(content) = fs::read_to_string(path) {
            let sockets = parse_proc_net_file(&content);

            for socket in sockets {
                if !should_include(&socket, mode, is_udp) {
                    continue;
                }

                if let Some(process_info) = inode_map.get(&socket.inode) {
                    ports.push(PortInfo {
                        port: socket.local_port,
                        protocol,
                        pid: process_info.pid,
                        process_name: process_info.name.clone(),
                        address: format!("{}:{}", socket.local_addr, socket.local_port),
                    });
                }
            }
        }
    }

    Ok(ports)
}
