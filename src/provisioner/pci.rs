//! PCI device passthrough management
//!
//! This module handles PCI device passthrough including:
//! - IOMMU validation
//! - Permanent device attachment
//! - Hot-plug/unplug operations
//! - vfio-pci driver binding

use crate::config::PciDevice;
use crate::error::{PciError, Result};
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

        // Check IOMMU enabled by looking for IOMMU groups
        // This is more reliable than dmesg which requires root privileges
        let iommu_groups = Path::new("/sys/kernel/iommu_groups");
        let has_iommu_groups = iommu_groups.exists()
            && fs::read_dir(iommu_groups)
                .map(|entries| entries.count() > 0)
                .unwrap_or(false);

        if !has_iommu_groups {
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
        // Parse address: 0000:01:00.0 -> domain:0000, bus:01, slot:00, function:0
        let parts: Vec<&str> = device.address.split(&[':', '.']).collect();

        if parts.len() != 4 {
            return Err(PciError::InvalidAddress(device.address.clone()).into());
        }

        let xml = format!(
            r#"<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
  </source>
</hostdev>"#,
            parts[0], parts[1], parts[2], parts[3]
        );

        Ok(xml)
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
            fs::write(&xml_path, xml)?;

            let result = Command::new("virsh")
                .args([
                    "-c",
                    "qemu:///system",
                    "attach-device",
                    &self.config.name,
                    &xml_path,
                    "--config",
                ])
                .status();

            fs::remove_file(&xml_path)?;

            match result {
                Ok(status) if status.success() => {
                    info!("{} attached to VM (permanent)", device.address);
                }
                Ok(status) => {
                    warn!(
                        "Failed to attach {} to VM XML (exit code: {:?})",
                        device.address,
                        status.code()
                    );
                }
                Err(e) => {
                    warn!("Failed to attach {} to VM XML: {}", device.address, e);
                }
            }
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
                thread::sleep(Duration::from_millis(500));
            }

            // Bind to vfio-pci
            self.bind_to_vfio(device)?;
            thread::sleep(Duration::from_millis(500));

            // Generate device XML
            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!(
                "/tmp/{}-pci-{}.xml",
                self.config.name,
                device.address.replace(':', "-")
            );
            fs::write(&xml_path, &xml)?;

            // Hot-attach to running VM
            let result = Command::new("virsh")
                .args([
                    "-c",
                    "qemu:///system",
                    "attach-device",
                    &self.config.name,
                    &xml_path,
                    "--live",
                ])
                .status();

            fs::remove_file(&xml_path)?;

            match result {
                Ok(status) if status.success() => {
                    info!("{} attached successfully", device.address);
                }
                Ok(status) => {
                    warn!(
                        "Failed to attach {} (exit code: {:?})",
                        device.address,
                        status.code()
                    );
                }
                Err(e) => {
                    warn!("Failed to attach {}: {}", device.address, e);
                }
            }
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
            let result = Command::new("virsh")
                .args([
                    "-c",
                    "qemu:///system",
                    "detach-device",
                    &self.config.name,
                    &xml_path,
                    "--live",
                ])
                .status();

            fs::remove_file(&xml_path)?;

            match result {
                Ok(status) if status.success() => {
                    info!("{} detached from VM", device.address);
                }
                Ok(status) => {
                    debug!(
                        "Detach {} returned exit code: {:?}",
                        device.address,
                        status.code()
                    );
                }
                Err(e) => {
                    debug!("Detach {} failed: {}", device.address, e);
                }
            }

            thread::sleep(Duration::from_millis(500));

            // Unbind from vfio-pci
            self.unbind_device(&device.address)?;
            thread::sleep(Duration::from_millis(500));

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
/// This checks kernel messages for IOMMU or DMAR (Intel VT-d) indicators.
/// Note: This may require root privileges to read dmesg.
///
/// # Returns
/// - `Ok(true)` if IOMMU appears to be enabled
/// - `Ok(false)` if IOMMU does not appear to be enabled
/// - `Err` if unable to check (e.g., dmesg not available)
#[allow(dead_code)] // Library-only export
pub fn check_iommu_enabled() -> Result<bool> {
    let dmesg = Command::new("dmesg").output()?;
    let dmesg_str = String::from_utf8_lossy(&dmesg.stdout);
    Ok(dmesg_str.contains("IOMMU") || dmesg_str.contains("DMAR"))
}

/// Get the IOMMU group number for a PCI device.
///
/// # Arguments
/// * `address` - PCI address in format "0000:00:14.0"
///
/// # Returns
/// - `Some(group)` if the device has an IOMMU group
/// - `None` if IOMMU is not enabled or device not found
#[allow(dead_code)] // Library-only export
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
#[allow(dead_code)] // Library-only export
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
#[allow(dead_code)] // Library-only export
pub fn is_clean_iommu_group(group: u32) -> Result<bool> {
    let devices = list_iommu_group_devices(group)?;
    Ok(devices.len() == 1)
}
