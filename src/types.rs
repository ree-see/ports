use std::fmt;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub port: u16,
    pub protocol: Protocol,
    pub pid: u32,
    pub process_name: String,
    pub address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "tcp"),
            Protocol::Udp => write!(f, "udp"),
        }
    }
}
