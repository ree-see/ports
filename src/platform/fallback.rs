#[cfg(not(target_os = "linux"))]
pub fn get_listening_ports() -> anyhow::Result<Vec<crate::types::PortInfo>> {
    use anyhow::anyhow;
    use crate::types::{PortInfo, Protocol};

    let listeners =
        listeners::get_all().map_err(|e| anyhow!("Failed to get listening ports: {}", e))?;

    let ports: Vec<PortInfo> = listeners
        .into_iter()
        .map(|l: listeners::Listener| PortInfo {
            port: l.socket.port(),
            protocol: match l.protocol {
                listeners::Protocol::TCP => Protocol::Tcp,
                listeners::Protocol::UDP => Protocol::Udp,
            },
            pid: l.process.pid,
            process_name: l.process.name.clone(),
            address: l.socket.to_string(),
            remote_address: None,
            container: None,
            service_name: None,
        })
        .collect();

    Ok(ports)
}
