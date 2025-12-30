//! USB device passthrough management
//!
//! This module handles USB device passthrough including:
//! - Permanent device attachment
//! - Hot-plug/unplug operations

use crate::config::UsbDevice;
use crate::error::{Result, UsbError};
use crate::libvirt_xml;
use crate::virsh;
use log::{debug, info, warn};
use std::fs;

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

            let xml = libvirt_xml::hostdev_usb(device);
            let xml_path = format!(
                "/tmp/{}-usb-{}-{}.xml",
                self.config.name, device.vendor_id, device.product_id
            );
            fs::write(&xml_path, &xml)?;

            match virsh::attach_device(&self.config.name, &xml_path, false, true) {
                Ok(_) => info!("USB device attached permanently"),
                Err(e) => warn!(
                    "Failed to attach USB device {}:{}: {}",
                    device.vendor_id, device.product_id, e
                ),
            }

            let _ = fs::remove_file(&xml_path);
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

        let xml = libvirt_xml::hostdev_usb(device);
        let xml_path = format!(
            "/tmp/{}-usb-{}-{}.xml",
            self.config.name, device.vendor_id, device.product_id
        );
        fs::write(&xml_path, &xml)?;

        let result = virsh::attach_device(&self.config.name, &xml_path, true, false);
        let _ = fs::remove_file(&xml_path);

        match result {
            Ok(_) => {
                info!("USB device attached successfully");
                Ok(())
            }
            Err(e) => Err(UsbError::AttachFailed {
                vendor_id: device.vendor_id.clone(),
                product_id: device.product_id.clone(),
                reason: e.to_string(),
            }
            .into()),
        }
    }

    /// Detach a single USB device from the running VM
    fn detach_usb_device(&self, device: &UsbDevice) -> Result<()> {
        debug!(
            "Detaching {} ({}:{})",
            device.description, device.vendor_id, device.product_id
        );

        let xml = libvirt_xml::hostdev_usb(device);
        let xml_path = format!(
            "/tmp/{}-usb-{}-{}.xml",
            self.config.name, device.vendor_id, device.product_id
        );
        fs::write(&xml_path, &xml)?;

        let result = virsh::detach_device(&self.config.name, &xml_path, true, false);
        let _ = fs::remove_file(&xml_path);

        match result {
            Ok(_) => {
                info!("USB device detached successfully");
                Ok(())
            }
            Err(e) => Err(UsbError::DetachFailed {
                vendor_id: device.vendor_id.clone(),
                product_id: device.product_id.clone(),
                reason: e.to_string(),
            }
            .into()),
        }
    }
}
