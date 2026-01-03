use anyhow::Result;

use crate::types::PortInfo;

#[cfg(target_os = "linux")]
mod linux;

mod fallback;

pub fn get_listening_ports() -> Result<Vec<PortInfo>> {
    fallback::get_listening_ports()
}
