//! UDP (User Datagram Protocol) implementation
//!
//! This module provides UDP packet parsing and construction, including optional checksum.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

/// Parsed UDP packet
#[derive(Debug)]
pub struct UdpPacket<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
    pub payload: &'a [u8],
}

impl<'a> UdpPacket<'a> {
    /// UDP header size
    const HEADER_SIZE: usize = 8;

    /// Parse a UDP packet from raw bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < Self::HEADER_SIZE {
            return None;
        }

        let src_port = u16::from_be_bytes([data[0], data[1]]);
        let dst_port = u16::from_be_bytes([data[2], data[3]]);
        let length = u16::from_be_bytes([data[4], data[5]]);
        let checksum = u16::from_be_bytes([data[6], data[7]]);

        if data.len() < length as usize {
            return None;
        }

        Some(Self {
            src_port,
            dst_port,
            length,
            checksum,
            payload: &data[Self::HEADER_SIZE..length as usize],
        })
    }
}

/// Build a UDP packet
pub fn build_udp_packet(
    buffer: &mut Vec<u8>,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Result<(), &'static str> {
    let length = 8 + payload.len();
    if length > 65535 {
        return Err("Packet too large");
    }

    // Source port
    buffer.extend_from_slice(&src_port.to_be_bytes());

    // Destination port
    buffer.extend_from_slice(&dst_port.to_be_bytes());

    // Length
    buffer.extend_from_slice(&(length as u16).to_be_bytes());

    // Checksum (0 = disabled for IPv4)
    buffer.extend_from_slice(&0u16.to_be_bytes());

    // Payload
    buffer.extend_from_slice(payload);

    Ok(())
}

/// UDP socket binding: maps port to receive queue
#[derive(Debug)]
struct UdpSocket {
    port: u16,
    rx_queue: Vec<(Vec<u8>, [u8; 4], u16)>, // (data, src_ip, src_port)
}

impl UdpSocket {
    fn new(port: u16) -> Self {
        Self { port, rx_queue: Vec::new() }
    }

    fn enqueue(&mut self, data: Vec<u8>, src_ip: [u8; 4], src_port: u16) {
        // Limit queue size to prevent memory exhaustion
        if self.rx_queue.len() < 100 {
            self.rx_queue.push((data, src_ip, src_port));
        }
    }

    fn dequeue(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
        if self.rx_queue.is_empty() {
            None
        } else {
            Some(self.rx_queue.remove(0))
        }
    }
}

/// Global UDP socket table
static UDP_SOCKETS: Mutex<BTreeMap<u16, UdpSocket>> = Mutex::new(BTreeMap::new());

/// Next ephemeral port
static NEXT_EPHEMERAL_PORT: Mutex<u16> = Mutex::new(49152);

/// Bind a UDP socket to a port
pub fn bind(port: u16) -> Result<u16, &'static str> {
    let mut sockets = UDP_SOCKETS.lock();

    let actual_port = if port == 0 {
        // Allocate ephemeral port
        let mut ephemeral = NEXT_EPHEMERAL_PORT.lock();
        let allocated = *ephemeral;
        *ephemeral = if allocated == 65535 { 49152 } else { allocated + 1 };
        allocated
    } else {
        port
    };

    if sockets.contains_key(&actual_port) {
        return Err("Port already in use");
    }

    sockets.insert(actual_port, UdpSocket::new(actual_port));
    Ok(actual_port)
}

/// Unbind a UDP socket
pub fn unbind(port: u16) {
    UDP_SOCKETS.lock().remove(&port);
}

/// Handle an incoming UDP packet
pub fn handle_udp_packet(src_ip: [u8; 4], data: &[u8]) {
    let packet = match UdpPacket::parse(data) {
        Some(p) => p,
        None => return,
    };

    // Find socket bound to destination port
    let mut sockets = UDP_SOCKETS.lock();
    if let Some(socket) = sockets.get_mut(&packet.dst_port) {
        socket.enqueue(packet.payload.to_vec(), src_ip, packet.src_port);
    }
}

/// Send a UDP packet
pub fn send_udp_packet(
    src_port: u16,
    dst_ip: [u8; 4],
    dst_port: u16,
    payload: &[u8],
) -> Result<(), &'static str> {
    // Build UDP packet
    let mut udp_buf = Vec::new();
    build_udp_packet(&mut udp_buf, src_port, dst_port, payload)?;

    // Send via IP layer
    crate::net::send_ipv4_packet(dst_ip, crate::net::ipv4::IPProtocol::UDP, &udp_buf)?;

    Ok(())
}

/// Receive a UDP packet (non-blocking)
pub fn recv_udp_packet(port: u16) -> Result<Option<(Vec<u8>, [u8; 4], u16)>, &'static str> {
    let mut sockets = UDP_SOCKETS.lock();
    let socket = sockets.get_mut(&port).ok_or("Socket not bound")?;
    Ok(socket.dequeue())
}

/// Receive a UDP packet with polling (blocking with timeout)
pub fn recv_udp_packet_blocking(
    port: u16,
    max_attempts: usize,
) -> Result<(Vec<u8>, [u8; 4], u16), &'static str> {
    for _ in 0..max_attempts {
        // Poll network
        crate::net::poll_rx();

        // Check if packet is available
        if let Some(packet) = recv_udp_packet(port)? {
            return Ok(packet);
        }

        // Small delay
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }

    Err("Receive timeout")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_udp_packet() {
        let data = [
            0x04, 0xd2, // Source port (1234)
            0x00, 0x35, // Destination port (53, DNS)
            0x00, 0x10, // Length (16)
            0x00, 0x00, // Checksum (disabled)
            0x74, 0x65, 0x73, 0x74, // Payload "test"
            0x00, 0x00, 0x00, 0x00, // More payload
        ];

        let packet = UdpPacket::parse(&data).unwrap();
        assert_eq!(packet.src_port, 1234);
        assert_eq!(packet.dst_port, 53);
        assert_eq!(packet.length, 16);
        assert_eq!(packet.payload, b"test\0\0\0\0");
    }

    #[test]
    fn test_build_udp_packet() {
        let mut buffer = Vec::new();
        let payload = b"test";

        build_udp_packet(&mut buffer, 1234, 53, payload).unwrap();

        assert_eq!(u16::from_be_bytes([buffer[0], buffer[1]]), 1234);
        assert_eq!(u16::from_be_bytes([buffer[2], buffer[3]]), 53);
        assert_eq!(u16::from_be_bytes([buffer[4], buffer[5]]), 12); // 8 + 4
        assert_eq!(&buffer[8..], payload);
    }
}
