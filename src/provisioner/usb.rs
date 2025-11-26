//! USB device passthrough management
//!
//! This module handles USB device passthrough including:
//! - Permanent device attachment
//! - Hot-plug/unplug operations

use crate::config::UsbDevice;
use crate::error::{UsbError, Result};
use crate::provisioner::device_detection::generate_usb_device_xml;
use log::{debug, info, warn};
use std::fs;
use std::process::Command;

/// USB passthrough operations for AppVMProvisioner
pub trait UsbPassthrough {
    fn setup_usb_passthrough_permanent(&self) -> Result<()>;
    fn attach_usb_devices_hotplug(&self) -> Result<()>;
    fn detach_usb_devices_hotplug(&self) -> Result<()>;
    fn attach_usb_device(&self, device: &UsbDevice) -> Result<()>;
    fn detach_usb_device(&self, device: &UsbDevice) -> Result<()>;
}

impl UsbPassthrough for super::AppVMProvisioner {
    /// Setup permanent USB passthrough (devices attached to VM config)
    fn setup_usb_passthrough_permanent(&self) -> Result<()> {
        info!("Setting up USB passthrough (permanent mode)...");

        for device in &self.config.usb_devices {
            debug!(
                "Attaching {} ({}:{})",
                device.description, device.vendor_id, device.product_id
            );

            let xml = generate_usb_device_xml(device);
            let xml_path = format!(
                "/tmp/{}-usb-{}-{}.xml",
                self.config.name, device.vendor_id, device.product_id
            );
            fs::write(&xml_path, &xml)?;

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

            if result.is_err() || !result?.success() {
                warn!(
                    "Failed to attach USB device {}:{}",
                    device.vendor_id, device.product_id
                );
            } else {
                info!("USB device attached permanently");
            }
        }

        Ok(())
    }

    /// Hot-attach all configured USB devices
    fn attach_usb_devices_hotplug(&self) -> Result<()> {
        info!("Hot-attaching USB devices...");
        for device in &self.config.usb_devices {
            self.attach_usb_device(device)?;
        }
        Ok(())
    }

    /// Hot-detach all configured USB devices
    fn detach_usb_devices_hotplug(&self) -> Result<()> {
        info!("Hot-detaching USB devices...");
        for device in &self.config.usb_devices {
            // Ignore errors on detach - device may already be detached
            let _ = self.detach_usb_device(device);
        }
        Ok(())
    }

    /// Attach a single USB device to the running VM
    fn attach_usb_device(&self, device: &UsbDevice) -> Result<()> {
        debug!(
            "Attaching {} ({}:{})",
            device.description, device.vendor_id, device.product_id
        );

        let xml = generate_usb_device_xml(device);
        let xml_path = format!(
            "/tmp/{}-usb-{}-{}.xml",
            self.config.name, device.vendor_id, device.product_id
        );
        fs::write(&xml_path, &xml)?;

        let result = Command::new("virsh")
            .args([
                "-c",
                "qemu:///system",
                "attach-device",
                &self.config.name,
                &xml_path,
                "--live",
            ])
            .output()?;

        fs::remove_file(&xml_path)?;

        if result.status.success() {
            info!("USB device attached successfully");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            Err(UsbError::AttachFailed {
                vendor_id: device.vendor_id.clone(),
                product_id: device.product_id.clone(),
                reason: stderr.to_string(),
            }
            .into())
        }
    }

    /// Detach a single USB device from the running VM
    fn detach_usb_device(&self, device: &UsbDevice) -> Result<()> {
        debug!(
            "Detaching {} ({}:{})",
            device.description, device.vendor_id, device.product_id
        );

        let xml = generate_usb_device_xml(device);
        let xml_path = format!(
            "/tmp/{}-usb-{}-{}.xml",
            self.config.name, device.vendor_id, device.product_id
        );
        fs::write(&xml_path, &xml)?;

        let result = Command::new("virsh")
            .args([
                "-c",
                "qemu:///system",
                "detach-device",
                &self.config.name,
                &xml_path,
                "--live",
            ])
            .output()?;

        fs::remove_file(&xml_path)?;

        if result.status.success() {
            info!("USB device detached successfully");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            Err(UsbError::DetachFailed {
                vendor_id: device.vendor_id.clone(),
                product_id: device.product_id.clone(),
                reason: stderr.to_string(),
            }
            .into())
        }
    }
}
