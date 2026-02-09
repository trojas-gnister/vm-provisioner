//! Libvirt XML generation utilities
//!
//! This module provides functions to generate libvirt XML fragments for
//! various device types (USB, network interfaces).

use crate::config::UsbDevice;

// ============================================================================
// USB Device XML
// ============================================================================

/// Generate libvirt XML for USB device passthrough
///
/// # Arguments
/// * `device` - USB device information
///
/// # Returns
/// XML string for virsh attach-device/detach-device
pub fn hostdev_usb(device: &UsbDevice) -> String {
    format!(
        r#"<hostdev mode='subsystem' type='usb' managed='yes'>
  <source>
    <vendor id='0x{}'/>
    <product id='0x{}'/>
  </source>
</hostdev>"#,
        device.vendor_id, device.product_id
    )
}

/// Generate libvirt XML for USB device passthrough from vendor:product IDs
///
/// # Arguments
/// * `vendor_id` - USB vendor ID (hex string, e.g., "046d")
/// * `product_id` - USB product ID (hex string, e.g., "c52b")
///
/// # Returns
/// XML string for virsh attach-device/detach-device
pub fn hostdev_usb_from_ids(vendor_id: &str, product_id: &str) -> String {
    format!(
        r#"<hostdev mode='subsystem' type='usb' managed='yes'>
  <source>
    <vendor id='0x{}'/>
    <product id='0x{}'/>
  </source>
</hostdev>"#,
        vendor_id, product_id
    )
}

// ============================================================================
// Network Interface XML
// ============================================================================

/// Generate libvirt XML for a network interface
///
/// # Arguments
/// * `mac` - MAC address of the interface
/// * `network` - Network name (e.g., "default")
///
/// # Returns
/// XML string for virsh attach-device/detach-device
pub fn interface_network(mac: &str, network: &str) -> String {
    format!(
        r#"<interface type='network'>
  <mac address='{}'/>
  <source network='{}'/>
</interface>"#,
        mac, network
    )
}

/// Generate libvirt XML for a bridged network interface
///
/// # Arguments
/// * `mac` - MAC address of the interface
/// * `bridge` - Bridge interface name (e.g., "br0")
///
/// # Returns
/// XML string for virsh attach-device/detach-device
pub fn interface_bridge(mac: &str, bridge: &str) -> String {
    format!(
        r#"<interface type='bridge'>
  <mac address='{}'/>
  <source bridge='{}'/>
  <model type='virtio'/>
</interface>"#,
        mac, bridge
    )
}

// ============================================================================
// Tests
