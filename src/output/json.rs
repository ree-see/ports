use crate::types::PortInfo;

pub fn print_ports(ports: &[PortInfo]) {
    let json = serde_json::to_string_pretty(ports).expect("Failed to serialize to JSON");
    println!("{json}");
}
