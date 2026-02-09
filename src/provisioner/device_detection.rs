//! Device detection and validation for USB and GPU passthrough
//!
//! This module provides functions to detect and validate hardware devices
//! for passthrough to virtual machines.

use crate::config::UsbDevice;
use crate::constants::{GPU_VENDOR_AMD, GPU_VENDOR_INTEL, GPU_VENDOR_NVIDIA, VULKAN_ICD_DIR};
use crate::error::{NetworkError, Result, UsbError};
use crate::libvirt_xml;
use crate::virsh;
use log::{debug, info, warn};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

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

// ============================================================================
// GPU render node detection
// ============================================================================

/// GPU vendor classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuVendor {
    Amd,
    Intel,
    Nvidia,
    Unknown,
}

/// A detected GPU render node on the host
#[derive(Debug, Clone)]
pub struct GpuRenderNode {
    pub vendor: GpuVendor,
    pub pci_slot: String,
    pub render_node: String,
    pub by_path: String,
}

/// Detect all GPU render nodes on the host
///
/// Scans `/sys/class/drm/renderD*` to find render nodes, resolves their
/// PCI slots, reads vendor IDs, and constructs stable `/dev/dri/by-path` paths.
pub fn detect_gpu_render_nodes() -> Vec<GpuRenderNode> {
    let mut nodes = Vec::new();

    let drm_class = Path::new("/sys/class/drm");
    let entries = match fs::read_dir(drm_class) {
        Ok(e) => e,
        Err(e) => {
            warn!("Cannot read /sys/class/drm: {}", e);
            return nodes;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("renderD") {
            continue;
        }

        let render_node = format!("/dev/dri/{}", name_str);

        // Resolve the PCI device via the 'device' symlink
        let device_link = entry.path().join("device");
        let pci_slot = match fs::read_link(&device_link) {
            Ok(target) => {
                // Target is something like ../../../0000:03:00.0
                target
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default()
            }
            Err(e) => {
                debug!("Cannot resolve device link for {}: {}", name_str, e);
                continue;
            }
        };

        if pci_slot.is_empty() {
            continue;
        }

        // Read vendor ID from sysfs
        let vendor_path = device_link.join("vendor");
        let vendor_id = match fs::read_to_string(&vendor_path) {
            Ok(v) => v.trim().to_lowercase(),
            Err(e) => {
                debug!("Cannot read vendor for {}: {}", pci_slot, e);
                continue;
            }
        };

        // Parse vendor: "0x1002" -> "1002"
        let vendor_hex = vendor_id.trim_start_matches("0x");
        let vendor = match vendor_hex {
            v if v == GPU_VENDOR_AMD => GpuVendor::Amd,
            v if v == GPU_VENDOR_INTEL => GpuVendor::Intel,
            v if v == GPU_VENDOR_NVIDIA => GpuVendor::Nvidia,
            _ => GpuVendor::Unknown,
        };

        let by_path = format!("/dev/dri/by-path/pci-{}-render", pci_slot);

        debug!(
            "Found GPU render node: {} ({:?}) at {}",
            render_node, vendor, pci_slot
        );

        nodes.push(GpuRenderNode {
            vendor,
            pci_slot,
            render_node,
            by_path,
        });
    }

    // Sort by PCI slot for deterministic ordering
    nodes.sort_by(|a, b| a.pci_slot.cmp(&b.pci_slot));
    nodes
}

/// Select the best GPU for Venus Vulkan
///
/// Preference: AMD > Intel > skip NVIDIA (no Venus support).
/// Returns `None` if no suitable GPU is found.
pub fn select_gpu_for_venus(nodes: &[GpuRenderNode]) -> Option<&GpuRenderNode> {
    // Prefer AMD first
    if let Some(node) = nodes.iter().find(|n| n.vendor == GpuVendor::Amd) {
        return Some(node);
    }
    // Then Intel
    if let Some(node) = nodes.iter().find(|n| n.vendor == GpuVendor::Intel) {
        return Some(node);
    }
    // NVIDIA has no Venus support
    None
}

/// Get the Vulkan ICD file path for a given GPU vendor
///
/// Returns `None` for NVIDIA or if the ICD file does not exist.
pub fn get_vulkan_icd_path(vendor: &GpuVendor) -> Option<String> {
    let filename = match vendor {
        GpuVendor::Amd => "radeon_icd.x86_64.json",
        GpuVendor::Intel => "intel_icd.x86_64.json",
        _ => return None,
    };

    let path = format!("{}/{}", VULKAN_ICD_DIR, filename);
    if Path::new(&path).exists() {
        Some(path)
    } else {
        warn!("Vulkan ICD file not found: {}", path);
        None
    }
}
