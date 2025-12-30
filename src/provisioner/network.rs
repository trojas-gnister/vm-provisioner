//! Network interface management for VMs
//!
//! This module handles network-related operations including:
//! - Removing network interfaces for airgapped VMs

use crate::error::{NetworkError, Result};
use crate::libvirt_xml;
use crate::virsh;
use log::{debug, info, warn};
use std::fs;

/// Network operations for AppVMProvisioner
pub trait NetworkManagement {
    fn remove_network_interface(&self) -> Result<()>;
}

impl NetworkManagement for super::AppVMProvisioner {
    /// Remove network interface from VM (for airgapped/network-disabled VMs)
    fn remove_network_interface(&self) -> Result<()> {
        debug!("Fetching VM XML to find network interface...");

        let xml = virsh::dumpxml(&self.config.name).map_err(|e| {
            NetworkError::InterfaceRemovalFailed(format!("Failed to get VM XML: {}", e))
        })?;

        // Find the MAC address of the interface to detach
        let mut mac_address = None;
        for line in xml.lines() {
            let line = line.trim();
            if line.contains("<mac address=") {
                debug!("Found MAC line: {}", line);
                if let Some(start) = line.find("address='") {
                    let rest = &line[start + 9..];
                    if let Some(end) = rest.find('\'') {
                        mac_address = Some(rest[..end].to_string());
                        break;
                    }
                }
            }
        }

        let mac = match mac_address {
            Some(m) => {
                info!("Found network interface with MAC: {}", m);
                m
            }
            None => {
                warn!("No network interface found to remove (already removed?)");
                return Ok(());
            }
        };

        // Create XML for the interface to detach
        let detach_xml = libvirt_xml::interface_network(&mac, "default");
        let xml_path = format!("/tmp/{}-detach-nic.xml", self.config.name);
        fs::write(&xml_path, &detach_xml)?;

        // Check if VM is running
        let is_running = virsh::is_vm_running(&self.config.name);
        debug!("VM is running: {}", is_running);

        // Detach based on VM state
        let result = if is_running {
            // For running VMs, detach live and persist to config
            virsh::detach_device(&self.config.name, &xml_path, true, true)
        } else {
            // For stopped VMs, just update config
            virsh::detach_device(&self.config.name, &xml_path, false, true)
        };

        let _ = fs::remove_file(&xml_path);

        match result {
            Ok(_) => {
                info!("Network interface removed (MAC: {})", mac);
                info!("VM is now airgapped - vsock will be used for display forwarding");

                // If VM was not running, start it now
                if !is_running {
                    debug!("Starting VM...");
                    virsh::start_if_stopped(&self.config.name);
                }
                Ok(())
            }
            Err(e) => {
                warn!("Failed to remove network interface: {}", e);
                Ok(()) // Non-fatal - VM might still work
            }
        }
    }
}
