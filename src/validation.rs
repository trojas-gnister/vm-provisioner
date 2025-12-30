//! Configuration validation helpers
//!
//! This module provides validation functions for PCI, USB, and shared folder
//! configurations. These validate the format and structure of device specifications
//! without checking whether devices actually exist on the system.

use crate::config::{PciDevice, SharedFolder, UsbDevice};
use crate::error::{ConfigError, Result};

/// Validate PCI device configuration.
///
/// Checks:
/// - Address format contains colons (e.g., "0000:01:00.0")
/// - vendor_id is exactly 4 hex digits
/// - device_id is exactly 4 hex digits
///
/// # Example
/// ```rust,ignore
/// use vm_provisioner::{PciDevice, validate_pci_device};
///
/// let pci = PciDevice {
///     address: "0000:01:00.0".into(),
///     vendor_id: "10de".into(),
///     device_id: "1c03".into(),
///     description: "NVIDIA GPU".into(),
///     original_driver: None,
///     iommu_group: None,
/// };
/// validate_pci_device(&pci)?;
/// ```
pub fn validate_pci_device(pci: &PciDevice) -> Result<()> {
    // Validate address format (0000:01:00.0)
    if !pci.address.contains(':') {
        return Err(ConfigError::Invalid(format!(
            "Invalid PCI address '{}'. Expected format: '0000:01:00.0'",
            pci.address
        ))
        .into());
    }

    // Validate vendor_id (4 hex digits)
    if !is_valid_hex_id(&pci.vendor_id) {
        return Err(ConfigError::Invalid(format!(
            "Invalid vendor_id '{}'. Expected 4 hex digits (e.g., '10de')",
            pci.vendor_id
        ))
        .into());
    }

    // Validate device_id (4 hex digits)
    if !is_valid_hex_id(&pci.device_id) {
        return Err(ConfigError::Invalid(format!(
            "Invalid device_id '{}'. Expected 4 hex digits (e.g., '1c03')",
            pci.device_id
        ))
        .into());
    }

    Ok(())
}

/// Validate USB device configuration.
///
/// Checks:
/// - vendor_id is exactly 4 hex digits
/// - product_id is exactly 4 hex digits
///
/// # Example
/// ```rust,ignore
/// use vm_provisioner::{UsbDevice, validate_usb_device};
///
/// let usb = UsbDevice {
///     vendor_id: "046d".into(),
///     product_id: "c52b".into(),
///     description: "Logitech Receiver".into(),
///     bus: None,
///     device: None,
/// };
/// validate_usb_device(&usb)?;
/// ```
pub fn validate_usb_device(usb: &UsbDevice) -> Result<()> {
    if !is_valid_hex_id(&usb.vendor_id) {
        return Err(ConfigError::Invalid(format!(
            "Invalid USB vendor_id '{}'. Expected 4 hex digits (e.g., '046d')",
            usb.vendor_id
        ))
        .into());
    }

    if !is_valid_hex_id(&usb.product_id) {
        return Err(ConfigError::Invalid(format!(
            "Invalid USB product_id '{}'. Expected 4 hex digits (e.g., 'c52b')",
            usb.product_id
        ))
        .into());
    }

    Ok(())
}

/// Validate shared folder configuration.
///
/// Checks:
/// - host_path is an absolute path (starts with '/')
/// - guest_path is an absolute path (starts with '/')
/// - Neither path contains '..' (path traversal prevention)
///
/// # Example
/// ```rust,ignore
/// use vm_provisioner::{SharedFolder, validate_shared_folder};
///
/// let folder = SharedFolder {
///     host_path: "/home/user/shared".into(),
///     guest_path: "/mnt/shared".into(),
///     tag: "shared0".into(),
///     readonly: false,
/// };
/// validate_shared_folder(&folder)?;
/// ```
pub fn validate_shared_folder(folder: &SharedFolder) -> Result<()> {
    if !folder.host_path.starts_with('/') {
        return Err(ConfigError::Invalid(format!(
            "host_path '{}' must be an absolute path (start with '/')",
            folder.host_path
        ))
        .into());
    }

    if !folder.guest_path.starts_with('/') {
        return Err(ConfigError::Invalid(format!(
            "guest_path '{}' must be an absolute path (start with '/')",
            folder.guest_path
        ))
        .into());
    }

    // Security: prevent path traversal
    if folder.host_path.contains("..") || folder.guest_path.contains("..") {
        return Err(
            ConfigError::Invalid("Paths cannot contain '..' (path traversal)".into()).into(),
        );
    }

    Ok(())
}

/// Check if a string is exactly 4 hex digits.
fn is_valid_hex_id(id: &str) -> bool {
    id.len() == 4 && id.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pci(address: &str, vendor: &str, device: &str) -> PciDevice {
        PciDevice {
            address: address.into(),
            vendor_id: vendor.into(),
            device_id: device.into(),
            description: "Test".into(),
            original_driver: None,
            iommu_group: None,
        }
    }

    fn make_usb(vendor: &str, product: &str) -> UsbDevice {
        UsbDevice {
            vendor_id: vendor.into(),
            product_id: product.into(),
            description: "Test".into(),
            bus: None,
            device: None,
        }
    }

    fn make_folder(host: &str, guest: &str) -> SharedFolder {
        SharedFolder {
            host_path: host.into(),
            guest_path: guest.into(),
            tag: "test".into(),
            readonly: false,
        }
    }

    #[test]
    fn test_valid_pci_device() {
        let pci = make_pci("0000:01:00.0", "10de", "1c03");
        assert!(validate_pci_device(&pci).is_ok());
    }

    #[test]
    fn test_invalid_pci_address() {
        let pci = make_pci("invalid", "10de", "1c03");
        assert!(validate_pci_device(&pci).is_err());
    }

    #[test]
    fn test_invalid_pci_vendor_id() {
        let pci = make_pci("0000:01:00.0", "xyz", "1c03");
        assert!(validate_pci_device(&pci).is_err());
    }

    #[test]
    fn test_invalid_pci_device_id() {
        let pci = make_pci("0000:01:00.0", "10de", "toolong");
        assert!(validate_pci_device(&pci).is_err());
    }

    #[test]
    fn test_valid_usb_device() {
        let usb = make_usb("046d", "c52b");
        assert!(validate_usb_device(&usb).is_ok());
    }

    #[test]
    fn test_invalid_usb_vendor() {
        let usb = make_usb("46d", "c52b"); // Only 3 chars
        assert!(validate_usb_device(&usb).is_err());
    }

    #[test]
    fn test_invalid_usb_product() {
        let usb = make_usb("046d", "gggg"); // Not hex
        assert!(validate_usb_device(&usb).is_err());
    }

    #[test]
    fn test_valid_shared_folder() {
        let folder = make_folder("/home/user/shared", "/mnt/shared");
        assert!(validate_shared_folder(&folder).is_ok());
    }

    #[test]
    fn test_relative_host_path() {
        let folder = make_folder("relative/path", "/mnt/shared");
        assert!(validate_shared_folder(&folder).is_err());
    }

    #[test]
    fn test_relative_guest_path() {
        let folder = make_folder("/home/user", "mnt/shared");
        assert!(validate_shared_folder(&folder).is_err());
    }

    #[test]
    fn test_path_traversal_host() {
        let folder = make_folder("/home/../etc/passwd", "/mnt/shared");
        assert!(validate_shared_folder(&folder).is_err());
    }

    #[test]
    fn test_path_traversal_guest() {
        let folder = make_folder("/home/user", "/mnt/../etc");
        assert!(validate_shared_folder(&folder).is_err());
    }

    #[test]
    fn test_is_valid_hex_id() {
        assert!(is_valid_hex_id("abcd"));
        assert!(is_valid_hex_id("1234"));
        assert!(is_valid_hex_id("ABCD"));
        assert!(is_valid_hex_id("a1B2"));
        assert!(!is_valid_hex_id("abc"));    // Too short
        assert!(!is_valid_hex_id("abcde"));  // Too long
        assert!(!is_valid_hex_id("ghij"));   // Not hex
        assert!(!is_valid_hex_id(""));       // Empty
    }
}
