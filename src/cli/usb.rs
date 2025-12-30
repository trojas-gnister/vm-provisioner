//! USB device passthrough operations
//!
//! This module handles attaching and detaching USB devices to/from running VMs.

use log::{debug, error, info};

use vm_provisioner::config::AppVMConfig;
use vm_provisioner::error::Result;
use vm_provisioner::provisioner::usb::UsbPassthrough;
use vm_provisioner::provisioner::{detect_usb_device, AppVMProvisioner};

use super::get_vm_status;

/// Attach a USB device to a running VM
pub fn usb_attach(name: String, device: String) -> Result<()> {
    info!("Attaching USB device to VM: {}", name);

    // Check VM is running
    if get_vm_status(&name) != "running" {
        error!(
            "VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    // Detect USB device
    let usb_device = detect_usb_device(&device)?;
    debug!(
        "Found: {} ({}:{})",
        usb_device.description, usb_device.vendor_id, usb_device.product_id
    );

    // Load config to create provisioner
    let config = AppVMConfig::load(&name)?;
    let provisioner = AppVMProvisioner::new(config);

    // Attach the device
    provisioner.attach_usb_device(&usb_device)?;
    Ok(())
}

/// Detach a USB device from a running VM
pub fn usb_detach(name: String, device: String) -> Result<()> {
    info!("Detaching USB device from VM: {}", name);

    // Check VM is running
    if get_vm_status(&name) != "running" {
        error!("VM is not running. Cannot detach device from stopped VM.");
        std::process::exit(1);
    }

    // Detect USB device
    let usb_device = detect_usb_device(&device)?;
    debug!(
        "Found: {} ({}:{})",
        usb_device.description, usb_device.vendor_id, usb_device.product_id
    );

    // Load config to create provisioner
    let config = AppVMConfig::load(&name)?;
    let provisioner = AppVMProvisioner::new(config);

    // Detach the device
    provisioner.detach_usb_device(&usb_device)?;
    Ok(())
}
