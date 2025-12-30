//! Libvirt XML generation utilities
//!
//! This module provides functions to generate libvirt XML fragments for
//! various device types (PCI, USB, network interfaces).

use crate::config::{PciDevice, UsbDevice};
use crate::error::{PciError, Result};

// ============================================================================
// PCI Device XML
// ============================================================================

/// Parsed PCI address components
pub struct PciAddress {
    pub domain: String,
    pub bus: String,
    pub slot: String,
    pub function: String,
}

impl PciAddress {
    /// Parse a PCI address string (e.g., "0000:01:00.0") into components
    pub fn parse(address: &str) -> Result<Self> {
        let parts: Vec<&str> = address.split(&[':', '.']).collect();

        if parts.len() != 4 {
            return Err(PciError::InvalidAddress(address.to_string()).into());
        }

        Ok(Self {
            domain: parts[0].to_string(),
            bus: parts[1].to_string(),
            slot: parts[2].to_string(),
            function: parts[3].to_string(),
        })
    }
}

/// Generate libvirt XML for PCI device passthrough
///
/// # Arguments
/// * `device` - PCI device information
///
/// # Returns
/// XML string for virsh attach-device/detach-device
pub fn hostdev_pci(device: &PciDevice) -> Result<String> {
    let addr = PciAddress::parse(&device.address)?;

    Ok(format!(
        r#"<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
  </source>
</hostdev>"#,
        addr.domain, addr.bus, addr.slot, addr.function
    ))
}

/// Generate libvirt XML for PCI device passthrough from address string
///
/// # Arguments
/// * `address` - PCI address in format "0000:01:00.0"
///
/// # Returns
/// XML string for virsh attach-device/detach-device
pub fn hostdev_pci_from_address(address: &str) -> Result<String> {
    let addr = PciAddress::parse(address)?;

    Ok(format!(
        r#"<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
  </source>
</hostdev>"#,
        addr.domain, addr.bus, addr.slot, addr.function
    ))
}

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
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pci_address_parse() {
        let addr = PciAddress::parse("0000:01:00.0").unwrap();
        assert_eq!(addr.domain, "0000");
        assert_eq!(addr.bus, "01");
        assert_eq!(addr.slot, "00");
        assert_eq!(addr.function, "0");
    }

    #[test]
    fn test_pci_address_parse_invalid() {
        assert!(PciAddress::parse("invalid").is_err());
        assert!(PciAddress::parse("0000:01:00").is_err()); // missing function
        assert!(PciAddress::parse("").is_err());
    }

    #[test]
    fn test_hostdev_pci_from_address() {
        let xml = hostdev_pci_from_address("0000:01:00.0").unwrap();
        assert!(xml.contains("domain='0x0000'"));
        assert!(xml.contains("bus='0x01'"));
        assert!(xml.contains("slot='0x00'"));
        assert!(xml.contains("function='0x0'"));
        assert!(xml.contains("type='pci'"));
        assert!(xml.contains("managed='yes'"));
    }

    #[test]
    fn test_hostdev_usb_from_ids() {
        let xml = hostdev_usb_from_ids("046d", "c52b");
        assert!(xml.contains("vendor id='0x046d'"));
        assert!(xml.contains("product id='0xc52b'"));
        assert!(xml.contains("type='usb'"));
        assert!(xml.contains("managed='yes'"));
    }

    #[test]
    fn test_interface_network() {
        let xml = interface_network("52:54:00:ab:cd:ef", "default");
        assert!(xml.contains("mac address='52:54:00:ab:cd:ef'"));
        assert!(xml.contains("source network='default'"));
        assert!(xml.contains("type='network'"));
    }

    #[test]
    fn test_interface_bridge() {
        let xml = interface_bridge("52:54:00:ab:cd:ef", "br0");
        assert!(xml.contains("mac address='52:54:00:ab:cd:ef'"));
        assert!(xml.contains("source bridge='br0'"));
        assert!(xml.contains("type='bridge'"));
        assert!(xml.contains("model type='virtio'"));
    }
}
