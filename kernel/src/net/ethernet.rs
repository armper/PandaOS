//! Ethernet frame handling
//!
//! This module provides parsing and construction of Ethernet II frames.

use alloc::vec::Vec;

/// Ethernet frame type (EtherType)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EtherType {
    /// IPv4
    IPv4 = 0x0800,
    /// ARP
    ARP = 0x0806,
    /// IPv6
    IPv6 = 0x86DD,
    /// Unknown
    Unknown = 0xFFFF,
}

impl From<u16> for EtherType {
    fn from(value: u16) -> Self {
        match value {
            0x0800 => Self::IPv4,
            0x0806 => Self::ARP,
            0x86DD => Self::IPv6,
            _ => Self::Unknown,
        }
    }
}

impl From<EtherType> for u16 {
    fn from(value: EtherType) -> Self {
        value as u16
    }
}

/// Parsed Ethernet frame
#[derive(Debug)]
pub struct EthernetFrame<'a> {
    /// Destination MAC address
    pub dst_mac: [u8; 6],
    /// Source MAC address
    pub src_mac: [u8; 6],
    /// EtherType
    pub ethertype: EtherType,
    /// Payload
    pub payload: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    /// Minimum Ethernet frame size (header only)
    const MIN_SIZE: usize = 14;

    /// Parse an Ethernet frame from raw bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < Self::MIN_SIZE {
            return None;
        }

        let mut dst_mac = [0u8; 6];
        let mut src_mac = [0u8; 6];

        dst_mac.copy_from_slice(&data[0..6]);
        src_mac.copy_from_slice(&data[6..12]);

        let ethertype_raw = u16::from_be_bytes([data[12], data[13]]);
        let ethertype = EtherType::from(ethertype_raw);

        Some(Self { dst_mac, src_mac, ethertype, payload: &data[14..] })
    }
}

/// Build an Ethernet frame
pub fn build_ethernet_frame(
    buffer: &mut Vec<u8>,
    src_mac: [u8; 6],
    dst_mac: [u8; 6],
    ethertype: EtherType,
    payload: &[u8],
) -> Result<(), &'static str> {
    // Ethernet header
    buffer.extend_from_slice(&dst_mac);
    buffer.extend_from_slice(&src_mac);
    buffer.extend_from_slice(&u16::to_be_bytes(ethertype.into()));

    // Payload
    buffer.extend_from_slice(payload);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ethernet_frame() {
        let frame = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // dst MAC (broadcast)
            0x52, 0x54, 0x00, 0x12, 0x34, 0x56, // src MAC
            0x08, 0x00, // EtherType (IPv4)
            0x45, 0x00, // IPv4 header start
        ];

        let parsed = EthernetFrame::parse(&frame).unwrap();
        assert_eq!(parsed.dst_mac, [0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert_eq!(parsed.src_mac, [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
        assert_eq!(parsed.ethertype, EtherType::IPv4);
        assert_eq!(parsed.payload, &[0x45, 0x00]);
    }

    #[test]
    fn test_build_ethernet_frame() {
        let mut buffer = Vec::new();
        let src_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        let dst_mac = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        let payload = [0x45, 0x00];

        build_ethernet_frame(&mut buffer, src_mac, dst_mac, EtherType::IPv4, &payload).unwrap();

        assert_eq!(buffer[0..6], dst_mac);
        assert_eq!(buffer[6..12], src_mac);
        assert_eq!(u16::from_be_bytes([buffer[12], buffer[13]]), 0x0800);
        assert_eq!(&buffer[14..], &payload);
    }
}
