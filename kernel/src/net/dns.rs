//! DNS (Domain Name System) client implementation
//!
//! This module provides a minimal DNS client for A record (IPv4 address) queries.

use crate::net::udp;
use alloc::string::String;
use alloc::vec::Vec;

/// DNS query type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DnsType {
    A = 1,     // IPv4 address
    AAAA = 28, // IPv6 address
}

/// DNS class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DnsClass {
    IN = 1, // Internet
}

/// DNS response code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DnsRCode {
    NoError = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
    NotImplemented = 4,
    Refused = 5,
}

impl From<u8> for DnsRCode {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::NoError,
            1 => Self::FormatError,
            2 => Self::ServerFailure,
            3 => Self::NameError,
            4 => Self::NotImplemented,
            5 => Self::Refused,
            _ => Self::ServerFailure,
        }
    }
}

/// Encode a domain name in DNS format
fn encode_domain_name(name: &str, buffer: &mut Vec<u8>) {
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        buffer.push(label.len() as u8);
        buffer.extend_from_slice(label.as_bytes());
    }
    buffer.push(0); // Null terminator
}

/// Decode a domain name from DNS format
fn decode_domain_name(data: &[u8], offset: &mut usize) -> Option<String> {
    let mut name = String::new();
    let mut jumped = false;
    let mut jump_offset = *offset;
    let mut first = true;

    loop {
        if jump_offset >= data.len() {
            return None;
        }

        let len = data[jump_offset];
        if len == 0 {
            jump_offset += 1;
            if !jumped {
                *offset = jump_offset;
            }
            break;
        }

        // Check for compression pointer
        if (len & 0xC0) == 0xC0 {
            if jump_offset + 1 >= data.len() {
                return None;
            }
            let ptr = u16::from_be_bytes([len & 0x3F, data[jump_offset + 1]]) as usize;
            if !jumped {
                *offset = jump_offset + 2;
            }
            jump_offset = ptr;
            jumped = true;
            continue;
        }

        jump_offset += 1;
        if jump_offset + len as usize > data.len() {
            return None;
        }

        if !first {
            name.push('.');
        }
        first = false;

        let label = &data[jump_offset..jump_offset + len as usize];
        if let Ok(s) = core::str::from_utf8(label) {
            name.push_str(s);
        } else {
            return None;
        }

        jump_offset += len as usize;
    }

    Some(name)
}

/// Build a DNS query packet
fn build_dns_query(hostname: &str, query_type: DnsType) -> Vec<u8> {
    let mut buffer = Vec::new();

    // Transaction ID
    let txid: u16 = 0x1234; // Simple fixed ID for now
    buffer.extend_from_slice(&txid.to_be_bytes());

    // Flags: Standard query, recursion desired
    buffer.extend_from_slice(&0x0100u16.to_be_bytes());

    // Questions: 1
    buffer.extend_from_slice(&1u16.to_be_bytes());

    // Answers: 0
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // Authority RRs: 0
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // Additional RRs: 0
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // Question section
    encode_domain_name(hostname, &mut buffer);

    // Type (A record)
    buffer.extend_from_slice(&(query_type as u16).to_be_bytes());

    // Class (IN)
    buffer.extend_from_slice(&(DnsClass::IN as u16).to_be_bytes());

    buffer
}

/// Parse a DNS response
fn parse_dns_response(data: &[u8]) -> Option<Vec<[u8; 4]>> {
    if data.len() < 12 {
        return None;
    }

    // Parse header
    let _txid = u16::from_be_bytes([data[0], data[1]]);
    let flags = u16::from_be_bytes([data[2], data[3]]);
    let rcode = DnsRCode::from((flags & 0x0F) as u8);

    if rcode != DnsRCode::NoError {
        return None;
    }

    let qdcount = u16::from_be_bytes([data[4], data[5]]);
    let ancount = u16::from_be_bytes([data[6], data[7]]);

    let mut offset = 12;

    // Skip question section
    for _ in 0..qdcount {
        // Skip name
        decode_domain_name(data, &mut offset)?;
        // Skip type and class
        offset += 4;
    }

    // Parse answer section
    let mut addresses = Vec::new();
    for _ in 0..ancount {
        // Skip name
        decode_domain_name(data, &mut offset)?;

        if offset + 10 > data.len() {
            return None;
        }

        let rtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let _rclass = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        let _ttl = u32::from_be_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]);
        offset += 10;

        if offset + rdlength as usize > data.len() {
            return None;
        }

        // Check if it's an A record
        if rtype == DnsType::A as u16 && rdlength == 4 {
            let mut ip = [0u8; 4];
            ip.copy_from_slice(&data[offset..offset + 4]);
            addresses.push(ip);
        }

        offset += rdlength as usize;
    }

    Some(addresses)
}

/// Perform a DNS lookup for a hostname
pub fn lookup(hostname: &str) -> Result<[u8; 4], &'static str> {
    let config = crate::net::get_config().ok_or("Network not initialized")?;

    // Bind ephemeral port for DNS query
    let src_port = udp::bind(0)?;

    // Build DNS query
    let query = build_dns_query(hostname, DnsType::A);

    // Send query to DNS server
    udp::send_udp_packet(src_port, config.dns_server, 53, &query)?;

    // Wait for response
    let (response, _src_ip, _src_port) = udp::recv_udp_packet_blocking(src_port, 1000)?;

    // Unbind port
    udp::unbind(src_port);

    // Parse response
    let addresses = parse_dns_response(&response).ok_or("Invalid DNS response")?;

    if addresses.is_empty() {
        return Err("No addresses found");
    }

    Ok(addresses[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_domain_name() {
        let mut buffer = Vec::new();
        encode_domain_name("example.com", &mut buffer);
        assert_eq!(
            buffer,
            vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0]
        );
    }

    #[test]
    fn test_build_dns_query() {
        let query = build_dns_query("example.com", DnsType::A);
        assert!(query.len() > 12); // At least header + question
        assert_eq!(u16::from_be_bytes([query[0], query[1]]), 0x1234); // Transaction ID
        assert_eq!(u16::from_be_bytes([query[2], query[3]]), 0x0100); // Flags
        assert_eq!(u16::from_be_bytes([query[4], query[5]]), 1); // 1 question
    }
}
