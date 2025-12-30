//! Device detection and validation for PCI and USB passthrough
//!
//! This module provides functions to detect and validate hardware devices
//! for passthrough to virtual machines.

use crate::config::{PciDevice, UsbDevice};
use crate::error::{NetworkError, PciError, Result, UsbError};
use crate::libvirt_xml;
use crate::virsh;
use log::{debug, info};
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Detect and validate a PCI device for passthrough
///
/// # Arguments
/// * `address` - PCI address in format "0000:01:00.0"
///
/// # Returns
/// * `Result<PciDevice>` - Device information if found
pub fn detect_pci_device(address: &str) -> Result<PciDevice> {
    let lspci_output = Command::new("lspci")
        .args(["-s", address, "-nn", "-k"])
        .output()?;

    if !lspci_output.status.success() || lspci_output.stdout.is_empty() {
        return Err(PciError::DeviceNotFound(address.to_string()).into());
    }

    let output_str = String::from_utf8_lossy(&lspci_output.stdout);
    let first_line = output_str.lines().next().unwrap_or("");

    let (vendor_id, device_id) = parse_vendor_device_ids(&output_str)?;

    let description = if let Some(desc_start) = first_line.find(": ") {
        let desc = &first_line[desc_start + 2..];
        if let Some(bracket_pos) = desc.find(" [") {
            desc[..bracket_pos].to_string()
        } else {
            desc.to_string()
        }
    } else {
        "Unknown device".to_string()
    };

    let original_driver = get_current_driver(address);
    let iommu_group = get_iommu_group(address);

    debug!(
        "Detected PCI device: {} ({}) vendor={} device={} driver={:?} iommu_group={:?}",
        address, description, vendor_id, device_id, original_driver, iommu_group
    );

    Ok(PciDevice {
        address: address.to_string(),
        vendor_id,
        device_id,
        description,
        original_driver,
        iommu_group,
    })
}

/// Parse vendor and device IDs from lspci output
fn parse_vendor_device_ids(lspci_output: &str) -> Result<(String, String)> {
    // Look for pattern like [10de:1c03]
    if let Some(start) = lspci_output.find('[') {
        if let Some(end) = lspci_output[start..].find(']') {
            let ids = &lspci_output[start + 1..start + end];
            if let Some(colon_pos) = ids.find(':') {
                let vendor = ids[..colon_pos].to_string();
                let device = ids[colon_pos + 1..].to_string();
                return Ok((vendor, device));
            }
        }
    }
    Err(PciError::ParseError.into())
}

/// Get the current driver bound to a PCI device
fn get_current_driver(address: &str) -> Option<String> {
    let driver_path = format!("/sys/bus/pci/devices/{}/driver", address);
    fs::read_link(&driver_path)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}

/// Get the IOMMU group for a PCI device
fn get_iommu_group(address: &str) -> Option<u32> {
    let iommu_path = format!("/sys/bus/pci/devices/{}/iommu_group", address);
    fs::read_link(&iommu_path).ok().and_then(|p| {
        p.file_name()
            .and_then(|n| n.to_string_lossy().parse::<u32>().ok())
    })
}

/// Detect and validate a USB device for passthrough
///
/// # Arguments
/// * `address` - USB address in format "vendor:product" (e.g., "046d:c52b")
///
/// # Returns
/// * `Result<UsbDevice>` - Device information if found
pub fn detect_usb_device(address: &str) -> Result<UsbDevice> {
    let parts: Vec<&str> = address.split(':').collect();
    if parts.len() != 2 {
        return Err(UsbError::InvalidFormat(address.to_string()).into());
    }

    let vendor_id = parts[0].to_lowercase();
    let product_id = parts[1].to_lowercase();

    // Validate hex format
    if vendor_id.len() != 4 || product_id.len() != 4 {
        return Err(UsbError::InvalidIds(address.to_string()).into());
    }

    let lsusb_output = Command::new("lsusb")
        .args(["-d", address])
        .output()?;

    if !lsusb_output.status.success() {
        return Err(UsbError::DeviceNotFound(format!("{}:{}", vendor_id, product_id)).into());
    }

    let output_str = String::from_utf8_lossy(&lsusb_output.stdout);
    let first_line = output_str.lines().next().unwrap_or("");

    if first_line.is_empty() {
        return Err(UsbError::DeviceNotFound(format!("{}:{}", vendor_id, product_id)).into());
    }

    let (bus, device) = parse_usb_bus_device(first_line);
    let description = parse_usb_description(first_line);

    info!(
        "Found USB device: {} (Bus {:03} Device {:03})",
        description,
        bus.unwrap_or(0),
        device.unwrap_or(0)
    );

    Ok(UsbDevice {
        vendor_id,
        product_id,
        description,
        bus,
        device,
    })
}

/// Parse bus and device numbers from lsusb output line
fn parse_usb_bus_device(lsusb_line: &str) -> (Option<u8>, Option<u8>) {
    // Format: "Bus 001 Device 003: ID 046d:c52b Logitech, Inc. Unifying Receiver"
    let mut bus: Option<u8> = None;
    let mut device: Option<u8> = None;

    let parts: Vec<&str> = lsusb_line.split_whitespace().collect();
    for i in 0..parts.len() {
        if parts[i] == "Bus" && i + 1 < parts.len() {
            bus = parts[i + 1].parse().ok();
        }
        if parts[i] == "Device" && i + 1 < parts.len() {
            let dev_str = parts[i + 1].trim_end_matches(':');
            device = dev_str.parse().ok();
        }
    }

    (bus, device)
}

/// Parse description from lsusb output line
fn parse_usb_description(lsusb_line: &str) -> String {
    // Format: "Bus 001 Device 003: ID 046d:c52b Logitech, Inc. Unifying Receiver"
    if let Some(id_pos) = lsusb_line.find("ID ") {
        let after_id = &lsusb_line[id_pos + 3..];
        if let Some(space_pos) = after_id.find(' ') {
            return after_id[space_pos + 1..].trim().to_string();
        }
    }
    "Unknown USB device".to_string()
}

/// Generate libvirt XML for USB hostdev passthrough
///
/// This is a convenience wrapper around libvirt_xml::hostdev_usb for backwards compatibility.
pub fn generate_usb_device_xml(device: &UsbDevice) -> String {
    libvirt_xml::hostdev_usb(device)
}

/// Retrieve auto-assigned vsock CID from libvirt domain XML
///
/// Note: CID is only assigned when VM is running, so this may start/stop the VM briefly
pub fn get_vsock_cid(vm_name: &str) -> Result<u32> {
    // First, try to get CID from current XML
    if let Ok(cid) = get_vsock_cid_from_xml(vm_name) {
        return Ok(cid);
    }

    // CID not found - VM might be shut off. Start it briefly to get CID assigned.
    info!("Starting VM briefly to retrieve vsock CID...");

    virsh::start_if_stopped(vm_name);

    // Wait a moment for CID assignment
    thread::sleep(Duration::from_secs(3));

    // Try again
    let result = get_vsock_cid_from_xml(vm_name);

    // Shut down the VM
    virsh::destroy_unchecked(vm_name);

    result
}

/// Parse vsock CID from VM XML
fn get_vsock_cid_from_xml(vm_name: &str) -> Result<u32> {
    let xml = virsh::dumpxml(vm_name).map_err(|e| {
        NetworkError::VsockCidRetrievalFailed(format!("Failed to get VM XML for {}: {}", vm_name, e))
    })?;

    // Parse: <cid auto='yes' address='3'/>
    for line in xml.lines() {
        let line = line.trim();
        if line.contains("<cid") && line.contains("address=") {
            // Try single quotes
            if let Some(start) = line.find("address='") {
                let rest = &line[start + 9..];
                if let Some(end) = rest.find('\'') {
                    if let Ok(cid) = rest[..end].parse::<u32>() {
                        return Ok(cid);
                    }
                }
            }
            // Try double quotes
            if let Some(start) = line.find("address=\"") {
                let rest = &line[start + 9..];
                if let Some(end) = rest.find('"') {
                    if let Ok(cid) = rest[..end].parse::<u32>() {
                        return Ok(cid);
                    }
                }
            }
        }
    }

    Err(NetworkError::VsockCidRetrievalFailed(format!(
        "Could not find vsock CID in VM XML for {}. Is vsock enabled?",
        vm_name
    ))
    .into())
}
