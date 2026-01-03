use std::net::Ipv4Addr;

use anyhow::Result;

pub fn parse_hex_addr(hex: &str) -> Result<Ipv4Addr> {
    let num = u32::from_str_radix(hex, 16)?;
    Ok(Ipv4Addr::new(
        (num & 0xFF) as u8,
        ((num >> 8) & 0xFF) as u8,
        ((num >> 16) & 0xFF) as u8,
        ((num >> 24) & 0xFF) as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_addr_localhost_returns_127_0_0_1() {
        let result = parse_hex_addr("0100007F").unwrap();
        assert_eq!(result, Ipv4Addr::new(127, 0, 0, 1));
    }

    #[test]
    fn test_parse_hex_addr_any_returns_0_0_0_0() {
        let result = parse_hex_addr("00000000").unwrap();
        assert_eq!(result, Ipv4Addr::new(0, 0, 0, 0));
    }
}
