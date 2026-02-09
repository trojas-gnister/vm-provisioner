//! Configuration validation helpers
//!
//! This module provides validation functions for USB and shared folder
//! configurations. These validate the format and structure of device specifications
//! without checking whether devices actually exist on the system.

use crate::config::{SharedFolder, UsbDevice};
use crate::error::{ConfigError, Result};

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
