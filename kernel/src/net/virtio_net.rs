//! VirtIO-Net driver for QEMU
//!
//! This module provides a minimal virtio-net driver for network I/O in QEMU.
//! It uses polling for RX (no IRQ initially) and supports TX.
//!
//! ## Safety
//!
//! This module contains unsafe code for:
//! - PCI configuration space access
//! - MMIO to device registers
//! - DMA buffer management
//!
//! All unsafe code is documented with SAFETY comments.

use alloc::vec;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// VirtIO vendor ID
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;

/// VirtIO network device ID (legacy)
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

/// VirtIO status bits
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FAILED: u8 = 128;

/// Virtqueue descriptor flags
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Virtqueue descriptor
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// Virtqueue available ring
#[repr(C, align(2))]
#[derive(Debug)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
}

/// Virtqueue used element
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// Virtqueue used ring
#[repr(C, align(4))]
#[derive(Debug)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
}

/// Virtio-net device structure
struct VirtioNetDevice {
    /// PCI I/O base address
    io_base: u16,
    /// MAC address
    mac_addr: [u8; 6],
    /// RX virtqueue
    rx_queue: VirtQueue,
    /// TX virtqueue
    tx_queue: VirtQueue,
}

/// Simplified virtqueue structure
struct VirtQueue {
    /// Queue size
    size: u16,
    /// Last seen used index
    last_seen_used: u16,
    /// Next available descriptor
    next_desc: u16,
}

impl VirtQueue {
    /// Create a new virtqueue
    fn new(size: u16) -> Self {
        Self { size, last_seen_used: 0, next_desc: 0 }
    }
}

/// Global virtio-net device
static VIRTIO_NET: Mutex<Option<VirtioNetDevice>> = Mutex::new(None);

/// RX buffer pool (simplified)
static RX_BUFFERS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Initialize the virtio-net driver
pub fn init() -> Result<(), &'static str> {
    // Scan PCI bus for virtio-net device
    let (bus, device, _function, io_base) = scan_pci_for_virtio_net()?;

    serial_println!("[VIRTIO-NET] Found device at bus={}, device={}", bus, device);

    // Reset device
    write_port_u8(io_base + 18, 0); // Write to status register

    // Set ACKNOWLEDGE status
    write_port_u8(io_base + 18, VIRTIO_STATUS_ACKNOWLEDGE);

    // Set DRIVER status
    write_port_u8(io_base + 18, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

    // Read MAC address from config space
    let mut mac_addr = [0u8; 6];
    for i in 0..6 {
        mac_addr[i] = read_port_u8(io_base + 20 + i as u16);
    }

    serial_println!(
        "[VIRTIO-NET] MAC address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac_addr[0],
        mac_addr[1],
        mac_addr[2],
        mac_addr[3],
        mac_addr[4],
        mac_addr[5]
    );

    // Negotiate features (accept defaults for now)
    let _features = read_port_u32(io_base + 0);
    write_port_u32(io_base + 4, 0); // No special features

    // Set FEATURES_OK
    write_port_u8(
        io_base + 18,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
    );

    // Create virtqueues
    let rx_queue = VirtQueue::new(256);
    let tx_queue = VirtQueue::new(256);

    // Initialize RX buffers
    let mut rx_buffers = RX_BUFFERS.lock();
    for _ in 0..32 {
        rx_buffers.push(vec![0u8; 2048]);
    }
    drop(rx_buffers);

    // Set DRIVER_OK
    write_port_u8(
        io_base + 18,
        VIRTIO_STATUS_ACKNOWLEDGE
            | VIRTIO_STATUS_DRIVER
            | VIRTIO_STATUS_FEATURES_OK
            | VIRTIO_STATUS_DRIVER_OK,
    );

    let device = VirtioNetDevice { io_base, mac_addr, rx_queue, tx_queue };

    *VIRTIO_NET.lock() = Some(device);

    serial_println!("[VIRTIO-NET] Initialization complete");

    Ok(())
}

/// Get MAC address from driver
pub fn get_mac_address() -> Result<[u8; 6], &'static str> {
    let device = VIRTIO_NET.lock();
    device.as_ref().map(|d| d.mac_addr).ok_or("Device not initialized")
}

/// Send a packet
pub fn send_packet(data: &[u8]) -> Result<(), &'static str> {
    let mut device = VIRTIO_NET.lock();
    let device = device.as_mut().ok_or("Device not initialized")?;

    // For simplicity, we'll just log that we're sending
    // A real implementation would use DMA and virtqueues
    serial_println!("[VIRTIO-NET] TX packet: {} bytes", data.len());

    // Simplified TX: would normally set up descriptor, add to avail ring, kick device
    // For now, this is a stub

    Ok(())
}

/// Poll for received packets
pub fn poll_rx() {
    // Simplified RX polling
    // Would normally check used ring for completed RX descriptors
    // For now, this is a stub
}

/// Scan PCI bus for virtio-net device
fn scan_pci_for_virtio_net() -> Result<(u8, u8, u8, u16), &'static str> {
    // Simplified PCI scan - check a few common locations
    for bus in 0..1 {
        for device in 0..32 {
            let vendor_id = read_pci_config(bus, device, 0, 0) as u16;
            if vendor_id == VIRTIO_VENDOR_ID {
                let device_id = (read_pci_config(bus, device, 0, 0) >> 16) as u16;
                if device_id == VIRTIO_NET_DEVICE_ID {
                    // Found it! Get I/O base from BAR0
                    let bar0 = read_pci_config(bus, device, 0, 0x10);
                    let io_base = (bar0 & 0xFFFC) as u16;
                    return Ok((bus, device, 0, io_base));
                }
            }
        }
    }
    Err("VirtIO-Net device not found")
}

/// Read PCI configuration space
fn read_pci_config(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address: u32 = 0x8000_0000
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xFC);

    // SAFETY: Writing to PCI configuration address port
    unsafe {
        let mut addr_port = Port::<u32>::new(0xCF8);
        let mut data_port = Port::<u32>::new(0xCFC);
        addr_port.write(address);
        data_port.read()
    }
}

/// Read a byte from I/O port
fn read_port_u8(port: u16) -> u8 {
    // SAFETY: Reading from device I/O port
    unsafe {
        let mut p = Port::<u8>::new(port);
        p.read()
    }
}

/// Write a byte to I/O port
fn write_port_u8(port: u16, value: u8) {
    // SAFETY: Writing to device I/O port
    unsafe {
        let mut p = Port::<u8>::new(port);
        p.write(value);
    }
}

/// Read a u32 from I/O port
fn read_port_u32(port: u16) -> u32 {
    // SAFETY: Reading from device I/O port
    unsafe {
        let mut p = Port::<u32>::new(port);
        p.read()
    }
}

/// Write a u32 to I/O port
fn write_port_u32(port: u16, value: u32) {
    // SAFETY: Writing to device I/O port
    unsafe {
        let mut p = Port::<u32>::new(port);
        p.write(value);
    }
}
