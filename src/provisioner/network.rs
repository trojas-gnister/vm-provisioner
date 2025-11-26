//! Network interface management for VMs
//!
//! This module handles network-related operations including:
//! - Removing network interfaces for airgapped VMs

use crate::error::{NetworkError, Result};
use log::{debug, info, warn};
use std::fs;
use std::process::Command;

/// Network operations for AppVMProvisioner
pub trait NetworkManagement {
    fn remove_network_interface(&self) -> Result<()>;
}

impl NetworkManagement for super::AppVMProvisioner {
    /// Remove network interface from VM (for airgapped/network-disabled VMs)
    fn remove_network_interface(&self) -> Result<()> {
        debug!("Fetching VM XML to find network interface...");

        let output = Command::new("virsh")
            .args(["-c", "qemu:///system", "dumpxml", &self.config.name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NetworkError::InterfaceRemovalFailed(format!(
                "Failed to get VM XML: {}",
                stderr
            ))
            .into());
        }

        let xml = String::from_utf8_lossy(&output.stdout);

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
        let detach_xml = format!(
            r#"<interface type='network'>
  <mac address='{}'/>
  <source network='default'/>
</interface>"#,
            mac
        );

        let xml_path = format!("/tmp/{}-detach-nic.xml", self.config.name);
        fs::write(&xml_path, &detach_xml)?;

        // Check if VM is running
        let state_output = Command::new("virsh")
            .args(["-c", "qemu:///system", "domstate", &self.config.name])
            .output()?;
        let vm_state = String::from_utf8_lossy(&state_output.stdout)
            .trim()
            .to_string();
        debug!("VM state: {}", vm_state);

        let is_running = vm_state == "running";

        // Build detach args based on VM state
        let mut detach_args = vec![
            "virsh",
            "-c",
            "qemu:///system",
            "detach-device",
            &self.config.name,
            &xml_path,
        ];

        if is_running {
            detach_args.push("--persistent");
            detach_args.push("--live");
        } else {
            detach_args.push("--config");
        }

        debug!("Running: sudo {}", detach_args.join(" "));

        let result = Command::new("sudo")
            .args(&detach_args[1..])
            .output();

        let _ = fs::remove_file(&xml_path);

        match result {
            Ok(output) if output.status.success() => {
                info!("Network interface removed (MAC: {})", mac);
                info!("VM is now airgapped - vsock will be used for display forwarding");

                // If VM was not running, start it now
                if !is_running {
                    debug!("Starting VM...");
                    let _ = Command::new("sudo")
                        .args(["virsh", "-c", "qemu:///system", "start", &self.config.name])
                        .output();
                }
                Ok(())
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to remove network interface: {}", stderr.trim());
                Ok(()) // Non-fatal - VM might still work
            }
            Err(e) => {
                warn!("Failed to remove network interface: {}", e);
                Ok(()) // Non-fatal
            }
        }
    }
}
