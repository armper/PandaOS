//! Network subsystem for PandaOS
//!
//! This module provides a minimal but functional networking stack with:
//! - Ethernet frame handling
//! - ARP (Address Resolution Protocol)
//! - IPv4 (Internet Protocol version 4)
//! - UDP (User Datagram Protocol)
//! - DNS client
//! - VirtIO-Net driver for QEMU
//!
//! ## Design
//!
//! The network stack is designed for simplicity and deterministic behavior:
//! - Static IP configuration (no DHCP initially)
//! - Polling-based RX (no IRQ required initially)
//! - Minimal allocations using fixed buffers
//! - Single-threaded access (protected by spinlocks)
//!
//! ## Invariants
//!
//! - Network must be initialized before use
//! - All unsafe code is confined to driver/PCI modules
//! - All checksums are validated on receive
//! - All packet sizes are bounds-checked

pub mod arp;
pub mod dns;
pub mod ethernet;
pub mod ipv4;
pub mod udp;
pub mod virtio_net;

use alloc::vec::Vec;
use spin::Mutex;

/// Network configuration
#[derive(Debug, Clone, Copy)]
pub struct NetConfig {
    /// Our IPv4 address
    pub ip_addr: [u8; 4],
    /// Network mask
    pub netmask: [u8; 4],
    /// Gateway IPv4 address
    pub gateway: [u8; 4],
    /// DNS server IPv4 address
    pub dns_server: [u8; 4],
    /// Our MAC address
    pub mac_addr: [u8; 6],
}

impl NetConfig {
    /// Default configuration for QEMU user-mode networking
    pub const fn default_qemu() -> Self {
        Self {
            ip_addr: [10, 0, 2, 15],
            netmask: [255, 255, 255, 0],
            gateway: [10, 0, 2, 2],
            dns_server: [10, 0, 2, 3],
            mac_addr: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56], // Will be overwritten by virtio-net
        }
    }
}

/// Global network configuration
static NET_CONFIG: Mutex<Option<NetConfig>> = Mutex::new(None);

/// Initialize the network subsystem
///
/// This must be called during kernel boot, after heap initialization.
pub fn init() -> Result<(), &'static str> {
    // Initialize virtio-net driver
    virtio_net::init()?;

    // Get MAC address from driver
    let mac = virtio_net::get_mac_address()?;

    // Set up configuration
    let mut config = NetConfig::default_qemu();
    config.mac_addr = mac;

    *NET_CONFIG.lock() = Some(config);

    serial_println!("[NET] Network initialized");
    serial_println!(
        "[NET] IP: {}.{}.{}.{}",
        config.ip_addr[0],
        config.ip_addr[1],
        config.ip_addr[2],
        config.ip_addr[3]
    );
    serial_println!(
        "[NET] Gateway: {}.{}.{}.{}",
        config.gateway[0],
        config.gateway[1],
        config.gateway[2],
        config.gateway[3]
    );
    serial_println!(
        "[NET] DNS: {}.{}.{}.{}",
        config.dns_server[0],
        config.dns_server[1],
        config.dns_server[2],
        config.dns_server[3]
    );
    serial_println!(
        "[NET] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );

    Ok(())
}

/// Get current network configuration
pub fn get_config() -> Option<NetConfig> {
    *NET_CONFIG.lock()
}

/// Process an incoming ethernet frame
pub fn handle_rx_frame(frame: &[u8]) {
    if let Some(eth_frame) = ethernet::EthernetFrame::parse(frame) {
        match eth_frame.ethertype {
            ethernet::EtherType::ARP => {
                arp::handle_arp_packet(eth_frame.payload);
            }
            ethernet::EtherType::IPv4 => {
                if let Some(ipv4_packet) = ipv4::IPv4Packet::parse(eth_frame.payload) {
                    handle_ipv4_packet(&ipv4_packet);
                }
            }
            _ => {
                // Unknown protocol, ignore
            }
        }
    }
}

/// Handle an IPv4 packet
fn handle_ipv4_packet(packet: &ipv4::IPv4Packet) {
    // Check if packet is for us
    let config = match get_config() {
        Some(c) => c,
        None => return,
    };

    if packet.dst_ip != config.ip_addr {
        // Not for us, ignore
        return;
    }

    // Dispatch based on protocol
    match packet.protocol {
        ipv4::IPProtocol::UDP => {
            udp::handle_udp_packet(packet.src_ip, packet.payload);
        }
        ipv4::IPProtocol::ICMP => {
            // ICMP not implemented yet
        }
        ipv4::IPProtocol::TCP => {
            // TCP not implemented yet
        }
        _ => {
            // Unknown protocol
        }
    }
}

/// Send an IPv4 packet
pub fn send_ipv4_packet(
    dst_ip: [u8; 4],
    protocol: ipv4::IPProtocol,
    payload: &[u8],
) -> Result<(), &'static str> {
    let config = get_config().ok_or("Network not initialized")?;

    // Resolve destination MAC via ARP
    let dst_mac = arp::resolve(dst_ip)?;

    // Build IPv4 packet
    let mut packet_buf = Vec::new();
    ipv4::build_ipv4_packet(&mut packet_buf, config.ip_addr, dst_ip, protocol, payload)?;

    // Build Ethernet frame
    let mut frame_buf = Vec::new();
    ethernet::build_ethernet_frame(
        &mut frame_buf,
        config.mac_addr,
        dst_mac,
        ethernet::EtherType::IPv4,
        &packet_buf,
    )?;

    // Send via driver
    virtio_net::send_packet(&frame_buf)?;

    Ok(())
}

/// Poll for received packets
///
/// This should be called periodically to process incoming packets.
/// In a polling-based system, this can be called from the scheduler.
pub fn poll_rx() {
    virtio_net::poll_rx();
}
