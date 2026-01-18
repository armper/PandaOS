//! IPv4 (Internet Protocol version 4) implementation
//!
//! This module provides IPv4 packet parsing and construction, including checksum calculation.

use alloc::vec::Vec;

/// IP protocol numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IPProtocol {
    ICMP = 1,
    TCP = 6,
    UDP = 17,
    Unknown = 0xFF,
}

impl From<u8> for IPProtocol {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::ICMP,
            6 => Self::TCP,
            17 => Self::UDP,
            _ => Self::Unknown,
        }
    }
}

impl From<IPProtocol> for u8 {
    fn from(value: IPProtocol) -> Self {
        value as u8
    }
}

/// Parsed IPv4 packet
#[derive(Debug)]
pub struct IPv4Packet<'a> {
    pub version: u8,
    pub ihl: u8,
    pub dscp: u8,
    pub ecn: u8,
    pub total_length: u16,
    pub identification: u16,
    pub flags: u8,
    pub fragment_offset: u16,
    pub ttl: u8,
    pub protocol: IPProtocol,
    pub checksum: u16,
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub payload: &'a [u8],
}

impl<'a> IPv4Packet<'a> {
    /// Minimum IPv4 header size (no options)
    const MIN_HEADER_SIZE: usize = 20;

    /// Parse an IPv4 packet from raw bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < Self::MIN_HEADER_SIZE {
            return None;
        }

        let version_ihl = data[0];
        let version = version_ihl >> 4;
        let ihl = version_ihl & 0x0F;

        if version != 4 {
            return None;
        }

        let header_len = (ihl as usize) * 4;
        if data.len() < header_len {
            return None;
        }

        let dscp_ecn = data[1];
        let dscp = dscp_ecn >> 2;
        let ecn = dscp_ecn & 0x03;

        let total_length = u16::from_be_bytes([data[2], data[3]]);
        let identification = u16::from_be_bytes([data[4], data[5]]);

        let flags_frag = u16::from_be_bytes([data[6], data[7]]);
        let flags = (flags_frag >> 13) as u8;
        let fragment_offset = flags_frag & 0x1FFF;

        let ttl = data[8];
        let protocol = IPProtocol::from(data[9]);
        let checksum = u16::from_be_bytes([data[10], data[11]]);

        let mut src_ip = [0u8; 4];
        let mut dst_ip = [0u8; 4];
        src_ip.copy_from_slice(&data[12..16]);
        dst_ip.copy_from_slice(&data[16..20]);

        // Validate checksum
        if !verify_checksum(&data[..header_len]) {
            return None;
        }

        let payload = &data[header_len..];

        Some(Self {
            version,
            ihl,
            dscp,
            ecn,
            total_length,
            identification,
            flags,
            fragment_offset,
            ttl,
            protocol,
            checksum,
            src_ip,
            dst_ip,
            payload,
        })
    }
}

/// Calculate IPv4 header checksum
fn calculate_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Sum all 16-bit words
    for i in (0..header.len()).step_by(2) {
        let word = if i + 1 < header.len() {
            u16::from_be_bytes([header[i], header[i + 1]])
        } else {
            u16::from_be_bytes([header[i], 0])
        };
        sum += u32::from(word);
    }

    // Add carry bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !sum as u16
}

/// Verify IPv4 header checksum
fn verify_checksum(header: &[u8]) -> bool {
    calculate_checksum(header) == 0
}

/// Build an IPv4 packet
pub fn build_ipv4_packet(
    buffer: &mut Vec<u8>,
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    protocol: IPProtocol,
    payload: &[u8],
) -> Result<(), &'static str> {
    let total_length = 20 + payload.len();
    if total_length > 65535 {
        return Err("Packet too large");
    }

    // Version and IHL
    buffer.push(0x45); // Version 4, IHL 5 (20 bytes)

    // DSCP and ECN
    buffer.push(0x00);

    // Total length
    buffer.extend_from_slice(&(total_length as u16).to_be_bytes());

    // Identification
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // Flags and fragment offset
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // TTL
    buffer.push(64);

    // Protocol
    buffer.push(protocol.into());

    // Checksum (placeholder)
    let checksum_pos = buffer.len();
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // Source IP
    buffer.extend_from_slice(&src_ip);

    // Destination IP
    buffer.extend_from_slice(&dst_ip);

    // Calculate and insert checksum
    let checksum = calculate_checksum(&buffer[buffer.len() - 20..]);
    buffer[checksum_pos] = (checksum >> 8) as u8;
    buffer[checksum_pos + 1] = (checksum & 0xFF) as u8;

    // Payload
    buffer.extend_from_slice(payload);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checksum() {
        // Example IPv4 header with checksum field zeroed
        let header = [
            0x45, 0x00, 0x00, 0x3c, // Version, IHL, DSCP, ECN, Total Length
            0x1c, 0x46, 0x40, 0x00, // Identification, Flags, Fragment Offset
            0x40, 0x06, 0x00, 0x00, // TTL, Protocol, Checksum (zeroed)
            0xac, 0x10, 0x0a, 0x63, // Source IP
            0xac, 0x10, 0x0a, 0x0c, // Destination IP
        ];

        let checksum = calculate_checksum(&header);
        assert_ne!(checksum, 0);

        // Verify that checksum verification works
        let mut header_with_checksum = header.to_vec();
        header_with_checksum[10] = (checksum >> 8) as u8;
        header_with_checksum[11] = (checksum & 0xFF) as u8;
        assert!(verify_checksum(&header_with_checksum));
    }

    #[test]
    fn test_build_ipv4_packet() {
        let mut buffer = Vec::new();
        let src_ip = [10, 0, 2, 15];
        let dst_ip = [10, 0, 2, 2];
        let payload = b"test";

        build_ipv4_packet(&mut buffer, src_ip, dst_ip, IPProtocol::UDP, payload).unwrap();

        assert_eq!(buffer[0], 0x45); // Version 4, IHL 5
        assert_eq!(buffer[9], 17); // UDP protocol
        assert_eq!(&buffer[12..16], &src_ip);
        assert_eq!(&buffer[16..20], &dst_ip);
        assert_eq!(&buffer[20..], payload);

        // Verify checksum
        assert!(verify_checksum(&buffer[..20]));
    }
}
