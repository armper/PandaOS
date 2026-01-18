//! ARP (Address Resolution Protocol) implementation
//!
//! This module handles ARP requests and maintains an ARP cache for IP-to-MAC resolution.

use crate::net::ethernet::{self, EtherType};
use crate::net::virtio_net;
use crate::net::NetConfig;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

/// ARP operation codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ArpOp {
    Request = 1,
    Reply = 2,
}

impl From<u16> for ArpOp {
    fn from(value: u16) -> Self {
        match value {
            1 => Self::Request,
            2 => Self::Reply,
            _ => Self::Request, // Default to request
        }
    }
}

/// ARP packet structure
#[derive(Debug)]
pub struct ArpPacket {
    pub hardware_type: u16,
    pub protocol_type: u16,
    pub hardware_len: u8,
    pub protocol_len: u8,
    pub operation: ArpOp,
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

impl ArpPacket {
    /// ARP packet size
    const SIZE: usize = 28;

    /// Parse an ARP packet from raw bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        let hardware_type = u16::from_be_bytes([data[0], data[1]]);
        let protocol_type = u16::from_be_bytes([data[2], data[3]]);
        let hardware_len = data[4];
        let protocol_len = data[5];
        let operation = ArpOp::from(u16::from_be_bytes([data[6], data[7]]));

        let mut sender_mac = [0u8; 6];
        let mut sender_ip = [0u8; 4];
        let mut target_mac = [0u8; 6];
        let mut target_ip = [0u8; 4];

        sender_mac.copy_from_slice(&data[8..14]);
        sender_ip.copy_from_slice(&data[14..18]);
        target_mac.copy_from_slice(&data[18..24]);
        target_ip.copy_from_slice(&data[24..28]);

        Some(Self {
            hardware_type,
            protocol_type,
            hardware_len,
            protocol_len,
            operation,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }

    /// Serialize ARP packet to bytes
    pub fn to_bytes(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.hardware_type.to_be_bytes());
        buffer.extend_from_slice(&self.protocol_type.to_be_bytes());
        buffer.push(self.hardware_len);
        buffer.push(self.protocol_len);
        buffer.extend_from_slice(&(self.operation as u16).to_be_bytes());
        buffer.extend_from_slice(&self.sender_mac);
        buffer.extend_from_slice(&self.sender_ip);
        buffer.extend_from_slice(&self.target_mac);
        buffer.extend_from_slice(&self.target_ip);
    }
}

/// ARP cache: maps IP addresses to MAC addresses
static ARP_CACHE: Mutex<BTreeMap<[u8; 4], [u8; 6]>> = Mutex::new(BTreeMap::new());

/// Handle an incoming ARP packet
pub fn handle_arp_packet(data: &[u8]) {
    let packet = match ArpPacket::parse(data) {
        Some(p) => p,
        None => return,
    };

    // Verify it's Ethernet + IPv4
    if packet.hardware_type != 1 || packet.protocol_type != 0x0800 {
        return;
    }

    // Update ARP cache with sender info
    ARP_CACHE.lock().insert(packet.sender_ip, packet.sender_mac);

    // Check if it's a request for us
    if packet.operation == ArpOp::Request {
        let config = match crate::net::get_config() {
            Some(c) => c,
            None => return,
        };

        if packet.target_ip == config.ip_addr {
            // Send ARP reply
            if let Err(e) = send_arp_reply(&packet, &config) {
                serial_println!("[ARP] Failed to send reply: {}", e);
            }
        }
    }
}

/// Send an ARP reply
fn send_arp_reply(request: &ArpPacket, config: &NetConfig) -> Result<(), &'static str> {
    let reply = ArpPacket {
        hardware_type: 1,      // Ethernet
        protocol_type: 0x0800, // IPv4
        hardware_len: 6,
        protocol_len: 4,
        operation: ArpOp::Reply,
        sender_mac: config.mac_addr,
        sender_ip: config.ip_addr,
        target_mac: request.sender_mac,
        target_ip: request.sender_ip,
    };

    // Serialize ARP packet
    let mut arp_buf = Vec::new();
    reply.to_bytes(&mut arp_buf);

    // Build Ethernet frame
    let mut frame_buf = Vec::new();
    ethernet::build_ethernet_frame(
        &mut frame_buf,
        config.mac_addr,
        request.sender_mac,
        EtherType::ARP,
        &arp_buf,
    )?;

    // Send packet
    virtio_net::send_packet(&frame_buf)?;

    Ok(())
}

/// Send an ARP request to resolve an IP address
pub fn send_arp_request(target_ip: [u8; 4]) -> Result<(), &'static str> {
    let config = crate::net::get_config().ok_or("Network not initialized")?;

    let request = ArpPacket {
        hardware_type: 1,      // Ethernet
        protocol_type: 0x0800, // IPv4
        hardware_len: 6,
        protocol_len: 4,
        operation: ArpOp::Request,
        sender_mac: config.mac_addr,
        sender_ip: config.ip_addr,
        target_mac: [0; 6], // Unknown
        target_ip,
    };

    // Serialize ARP packet
    let mut arp_buf = Vec::new();
    request.to_bytes(&mut arp_buf);

    // Build Ethernet frame (broadcast)
    let mut frame_buf = Vec::new();
    ethernet::build_ethernet_frame(
        &mut frame_buf,
        config.mac_addr,
        [0xff, 0xff, 0xff, 0xff, 0xff, 0xff], // Broadcast
        EtherType::ARP,
        &arp_buf,
    )?;

    // Send packet
    virtio_net::send_packet(&frame_buf)?;

    Ok(())
}

/// Resolve an IP address to a MAC address
///
/// This will check the ARP cache first, and if not found, send an ARP request
/// and poll for a response.
pub fn resolve(ip: [u8; 4]) -> Result<[u8; 6], &'static str> {
    // Check cache first
    if let Some(mac) = ARP_CACHE.lock().get(&ip) {
        return Ok(*mac);
    }

    // Send ARP request
    send_arp_request(ip)?;

    // Poll for response (with timeout)
    for _ in 0..100 {
        // Poll network
        crate::net::poll_rx();

        // Check cache again
        if let Some(mac) = ARP_CACHE.lock().get(&ip) {
            return Ok(*mac);
        }

        // Small delay
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }

    Err("ARP resolution timeout")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arp_packet() {
        let data = [
            0x00, 0x01, // Hardware type (Ethernet)
            0x08, 0x00, // Protocol type (IPv4)
            0x06, // Hardware length
            0x04, // Protocol length
            0x00, 0x01, // Operation (Request)
            0x52, 0x54, 0x00, 0x12, 0x34, 0x56, // Sender MAC
            0x0a, 0x00, 0x02, 0x0f, // Sender IP (10.0.2.15)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Target MAC
            0x0a, 0x00, 0x02, 0x02, // Target IP (10.0.2.2)
        ];

        let packet = ArpPacket::parse(&data).unwrap();
        assert_eq!(packet.hardware_type, 1);
        assert_eq!(packet.protocol_type, 0x0800);
        assert_eq!(packet.operation, ArpOp::Request);
        assert_eq!(packet.sender_ip, [10, 0, 2, 15]);
        assert_eq!(packet.target_ip, [10, 0, 2, 2]);
    }
}
