//! PCI device passthrough management
//!
//! This module handles PCI device passthrough including:
//! - IOMMU validation
//! - Permanent device attachment
//! - Hot-plug/unplug operations
//! - vfio-pci driver binding

use crate::config::PciDevice;
use crate::constants::DEVICE_UNBIND_DELAY_MS;
use crate::error::{PciError, Result};
use crate::libvirt_xml;
use crate::virsh;
use log::{debug, info, warn};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// PCI passthrough operations for AppVMProvisioner
pub trait PciPassthrough {
    fn validate_pci_passthrough(&self) -> Result<()>;
    fn get_iommu_group_devices(&self, group: u32) -> Result<Vec<String>>;
    fn generate_pci_device_xml(&self, device: &PciDevice) -> Result<String>;
    fn setup_pci_passthrough_permanent(&self) -> Result<()>;
    fn attach_pci_devices_hotplug(&self) -> Result<()>;
    fn detach_pci_devices_hotplug(&self) -> Result<()>;
    fn unbind_device(&self, address: &str) -> Result<()>;
    fn bind_to_vfio(&self, device: &PciDevice) -> Result<()>;
    fn rebind_to_driver(&self, address: &str, driver: &str) -> Result<()>;
}

impl PciPassthrough for super::AppVMProvisioner {
    /// Validate PCI passthrough prerequisites (IOMMU, vfio-pci)
    fn validate_pci_passthrough(&self) -> Result<()> {
        info!("Validating PCI passthrough setup...");

        // Check IOMMU enabled
        if !check_iommu_enabled() {
            return Err(PciError::IommuNotEnabled.into());
        }
        debug!("IOMMU enabled");

        // Check vfio-pci module available
        let modprobe = Command::new("modprobe").arg("vfio-pci").status();

        if modprobe.is_err() {
            return Err(PciError::VfioNotAvailable.into());
        }
        debug!("vfio-pci module available");

        // Validate each device and check IOMMU groups
        for device in &self.config.pci_devices {
            if let Some(group) = device.iommu_group {
                let group_devices = self.get_iommu_group_devices(group)?;
                if group_devices.len() > 1 {
                    warn!(
                        "IOMMU group {} contains {} devices:",
                        group,
                        group_devices.len()
                    );
                    for dev in &group_devices {
                        debug!("  {}", dev);
                    }
                    warn!("All devices in the group will be isolated from the host.");
                }
            }
        }

        Ok(())
    }

    /// Get all devices in an IOMMU group
    fn get_iommu_group_devices(&self, group: u32) -> Result<Vec<String>> {
        let group_path = format!("/sys/kernel/iommu_groups/{}/devices", group);
        let mut devices = Vec::new();

        if let Ok(entries) = fs::read_dir(&group_path) {
            for entry in entries.flatten() {
                if let Some(device_name) = entry.file_name().to_str() {
                    let lspci = Command::new("lspci").args(["-s", device_name]).output();

                    if let Ok(output) = lspci {
                        let desc = String::from_utf8_lossy(&output.stdout);
                        devices.push(desc.trim().to_string());
                    } else {
                        devices.push(device_name.to_string());
                    }
                }
            }
        }

        Ok(devices)
    }

    /// Generate libvirt XML for PCI device passthrough
    fn generate_pci_device_xml(&self, device: &PciDevice) -> Result<String> {
        libvirt_xml::hostdev_pci(device)
    }

    /// Setup permanent PCI passthrough (devices attached to VM config)
    fn setup_pci_passthrough_permanent(&self) -> Result<()> {
        info!("Setting up permanent PCI passthrough...");

        for device in &self.config.pci_devices {
            debug!("Adding {} to VM XML", device.address);

            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!(
                "/tmp/{}-pci-{}.xml",
                self.config.name,
                device.address.replace(':', "-")
            );
            fs::write(&xml_path, &xml)?;

            match virsh::attach_device(&self.config.name, &xml_path, false, true) {
                Ok(_) => info!("{} attached to VM (permanent)", device.address),
                Err(e) => warn!("Failed to attach {} to VM XML: {}", device.address, e),
            }

            let _ = fs::remove_file(&xml_path);
        }

        Ok(())
    }

    /// Hot-attach PCI devices to running VM
    fn attach_pci_devices_hotplug(&self) -> Result<()> {
        info!("Hot-attaching PCI devices...");

        for device in &self.config.pci_devices {
            debug!("Attaching {} ({})", device.address, device.description);

            // Unbind from current driver
            if device.original_driver.is_some() {
                self.unbind_device(&device.address)?;
                thread::sleep(Duration::from_millis(DEVICE_UNBIND_DELAY_MS));
            }

            // Bind to vfio-pci
            self.bind_to_vfio(device)?;
            thread::sleep(Duration::from_millis(DEVICE_UNBIND_DELAY_MS));

            // Generate device XML
            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!(
                "/tmp/{}-pci-{}.xml",
                self.config.name,
                device.address.replace(':', "-")
            );
            fs::write(&xml_path, &xml)?;

            // Hot-attach to running VM
            match virsh::attach_device(&self.config.name, &xml_path, true, false) {
                Ok(_) => info!("{} attached successfully", device.address),
                Err(e) => warn!("Failed to attach {}: {}", device.address, e),
            }

            let _ = fs::remove_file(&xml_path);
        }

        Ok(())
    }

    /// Hot-detach PCI devices from running VM
    fn detach_pci_devices_hotplug(&self) -> Result<()> {
        info!("Hot-detaching PCI devices...");

        for device in &self.config.pci_devices {
            debug!("Detaching {} ({})", device.address, device.description);

            // Generate XML for detach
            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!(
                "/tmp/{}-pci-{}.xml",
                self.config.name,
                device.address.replace(':', "-")
            );
            fs::write(&xml_path, &xml)?;

            // Detach from VM
            match virsh::detach_device(&self.config.name, &xml_path, true, false) {
                Ok(_) => info!("{} detached from VM", device.address),
                Err(e) => debug!("Detach {} failed: {}", device.address, e),
            }

            let _ = fs::remove_file(&xml_path);

            thread::sleep(Duration::from_millis(DEVICE_UNBIND_DELAY_MS));

            // Unbind from vfio-pci
            self.unbind_device(&device.address)?;
            thread::sleep(Duration::from_millis(DEVICE_UNBIND_DELAY_MS));

            // Rebind to original driver
            if let Some(ref driver) = device.original_driver {
                debug!("Restoring driver: {}", driver);
                self.rebind_to_driver(&device.address, driver)?;
                info!("{} restored to {}", device.address, driver);
            }
        }

        Ok(())
    }

    /// Unbind a PCI device from its current driver
    fn unbind_device(&self, address: &str) -> Result<()> {
        let unbind_path = format!("/sys/bus/pci/devices/{}/driver/unbind", address);

        if Path::new(&unbind_path).exists() {
            let result = Command::new("sudo")
                .args([
                    "bash",
                    "-c",
                    &format!("echo '{}' > {}", address, unbind_path),
                ])
                .status();

            match result {
                Ok(status) if status.success() => {
                    debug!("Device {} unbound successfully", address);
                }
                Ok(_) | Err(_) => {
                    // Device may already be unbound, not a fatal error
                    debug!("Device {} may already be unbound", address);
                }
            }
        }

        Ok(())
    }

    /// Bind a PCI device to vfio-pci driver
    fn bind_to_vfio(&self, device: &PciDevice) -> Result<()> {
        // Ensure vfio-pci module loaded
        Command::new("sudo")
            .args(["modprobe", "vfio-pci"])
            .status()?;

        // Bind device to vfio-pci using new_id
        let new_id = format!("{} {}", device.vendor_id, device.device_id);
        let new_id_path = "/sys/bus/pci/drivers/vfio-pci/new_id";

        let result = Command::new("sudo")
            .args(["bash", "-c", &format!("echo '{}' > {}", new_id, new_id_path)])
            .status();

        let needs_manual_bind = match result {
            Ok(status) if status.success() => false,
            _ => true, // May already be bound, try manual bind
        };

        if needs_manual_bind {
            let bind_path = "/sys/bus/pci/drivers/vfio-pci/bind";
            Command::new("sudo")
                .args([
                    "bash",
                    "-c",
                    &format!("echo '{}' > {}", device.address, bind_path),
                ])
                .status()?;
        }

        Ok(())
    }

    /// Rebind a PCI device to its original driver
    fn rebind_to_driver(&self, address: &str, driver: &str) -> Result<()> {
        let bind_path = format!("/sys/bus/pci/drivers/{}/bind", driver);

        if Path::new(&bind_path).exists() {
            Command::new("sudo")
                .args(["bash", "-c", &format!("echo '{}' > {}", address, bind_path)])
                .status()?;
        }

        Ok(())
    }
}

// ============================================================================
// Standalone IOMMU helper functions (for library consumers)
// ============================================================================

// These functions are library-only exports for external consumers building
// tools like virt-isolation-usb. The CLI binary doesn't use them directly.

/// Check if IOMMU is enabled on the system.
///
/// Checks for the existence and population of IOMMU groups in sysfs.
/// This is more reliable than checking dmesg which may be truncated.
///
/// # Returns
/// - `true` if IOMMU is enabled and groups exist
/// - `false` if IOMMU is not enabled
pub fn check_iommu_enabled() -> bool {
    let iommu_groups = Path::new("/sys/kernel/iommu_groups");
    if !iommu_groups.exists() {
        return false;
    }
    match fs::read_dir(iommu_groups) {
        Ok(entries) => entries.count() > 0,
        Err(_) => false,
    }
}

/// Get the IOMMU group number for a PCI device.
///
/// # Arguments
/// * `address` - PCI address in format "0000:00:14.0"
///
/// # Returns
/// - `Some(group)` if the device has an IOMMU group
/// - `None` if IOMMU is not enabled or device not found
pub fn get_iommu_group(address: &str) -> Option<u32> {
    let path = format!("/sys/bus/pci/devices/{}/iommu_group", address);
    fs::read_link(&path)
        .ok()
        .and_then(|p| p.file_name()?.to_str()?.parse().ok())
}

/// List all PCI device addresses in an IOMMU group.
///
/// # Arguments
/// * `group` - IOMMU group number
///
/// # Returns
/// Vector of PCI addresses (e.g., ["0000:00:14.0", "0000:00:14.2"])
pub fn list_iommu_group_devices(group: u32) -> Result<Vec<String>> {
    let group_path = format!("/sys/kernel/iommu_groups/{}/devices", group);
    let mut devices = Vec::new();

    if let Ok(entries) = fs::read_dir(&group_path) {
        for entry in entries.flatten() {
            if let Some(device_name) = entry.file_name().to_str() {
                devices.push(device_name.to_string());
            }
        }
    }

    Ok(devices)
}

/// Check if an IOMMU group contains only one device.
///
/// A "clean" IOMMU group with a single device is ideal for PCI passthrough
/// as it can be passed through without affecting other devices.
///
/// # Arguments
/// * `group` - IOMMU group number
///
/// # Returns
/// - `Ok(true)` if the group contains exactly one device
/// - `Ok(false)` if the group contains multiple devices
pub fn is_clean_iommu_group(group: u32) -> Result<bool> {
    let devices = list_iommu_group_devices(group)?;
    Ok(devices.len() == 1)
}

/// Get the current driver bound to a PCI device.
///
/// # Arguments
/// * `address` - PCI address in format "0000:01:00.0"
///
/// # Returns
/// - `Some(driver_name)` if device is bound to a driver
/// - `None` if device has no driver or doesn't exist
pub fn get_pci_driver(address: &str) -> Option<String> {
    let driver_path = format!("/sys/bus/pci/devices/{}/driver", address);
    fs::read_link(&driver_path)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}

/// Check if a PCI device is bound to the vfio-pci driver.
///
/// # Arguments
/// * `address` - PCI address in format "0000:01:00.0"
///
/// # Returns
/// - `true` if device is bound to vfio-pci
/// - `false` otherwise
pub fn is_device_bound_to_vfio(address: &str) -> bool {
    matches!(get_pci_driver(address), Some(driver) if driver == "vfio-pci")
}

/// Check passthrough prerequisites, returning reason if not met.
///
/// This function checks whether PCI passthrough can proceed:
/// 1. IOMMU must be enabled
/// 2. Each device must be bound to vfio-pci driver
///
/// # Arguments
/// * `pci_devices` - Slice of PCI devices to check
///
/// # Returns
/// - `None` if all prerequisites are met
/// - `Some(reason)` if prerequisites are not met, with explanation
///
/// This allows callers to defer provisioning rather than fail immediately.
pub fn check_passthrough_prerequisites(pci_devices: &[PciDevice]) -> Option<String> {
    if pci_devices.is_empty() {
        return None;
    }

    if !check_iommu_enabled() {
        return Some("IOMMU not enabled (reboot required after enabling in BIOS)".into());
    }

    for pci in pci_devices {
        if !is_device_bound_to_vfio(&pci.address) {
            let driver = get_pci_driver(&pci.address).unwrap_or_else(|| "none".to_string());
            return Some(format!(
                "Device {} not bound to vfio-pci (current: {})",
                pci.address, driver
            ));
        }
    }
    None
}
