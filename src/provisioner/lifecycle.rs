//! VM lifecycle management (start, stop, destroy)
//!
//! This module handles runtime VM operations:
//! - Starting VMs (with optional hot-plug)
//! - Stopping VMs (with graceful shutdown)
//! - Destroying VMs (cleanup)

use crate::config::GraphicsBackend;
use crate::error::Result;
use crate::provisioner::pci::PciPassthrough;
use crate::provisioner::usb::UsbPassthrough;
use log::{debug, error, info, warn};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// VM lifecycle operations for AppVMProvisioner
pub trait Lifecycle {
    fn start_vm(&self) -> Result<()>;
    fn stop_vm(&self) -> Result<()>;
    fn destroy_vm(&self) -> Result<()>;
}

impl Lifecycle for super::AppVMProvisioner {
    /// Start the VM
    fn start_vm(&self) -> Result<()> {
        info!("Starting VM: {}", self.config.name);

        Command::new("virsh")
            .args(["-c", "qemu:///system", "start", &self.config.name])
            .status()?;

        // Wait for VM to boot
        thread::sleep(Duration::from_secs(5));

        // Hot-attach PCI devices if in hot-plug mode
        if self.config.pci_hotplug && !self.config.pci_devices.is_empty() {
            self.attach_pci_devices_hotplug()?;
        }

        // Hot-attach USB devices if in hot-plug mode
        if self.config.usb_hotplug && !self.config.usb_devices.is_empty() {
            self.attach_usb_devices_hotplug()?;
        }

        // Handle display based on headless mode
        if self.config.headless {
            info!("Headless VM started - use serial console to connect");
            info!("Connect with: virsh console {}", self.config.name);
            return Ok(());
        }

        // Launch SPICE viewer for immediate functionality
        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu | GraphicsBackend::QxlSpice => {
                info!("Launching SPICE viewer...");
                let vm_name = self.config.name.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(5));

                    // Get the actual SPICE port from virsh
                    if let Ok(output) = Command::new("virsh")
                        .args(["-c", "qemu:///system", "domdisplay", &vm_name])
                        .output()
                    {
                        if let Ok(display) = String::from_utf8(output.stdout) {
                            let display = display.trim();
                            if !display.is_empty() {
                                let _ = Command::new("remote-viewer").arg(display).spawn();
                                return;
                            }
                        }
                    }

                    // Fallback to default port
                    let _ = Command::new("remote-viewer")
                        .arg("spice://127.0.0.1:5900")
                        .spawn();
                });
                debug!("SPICE viewer will launch automatically");
                debug!(
                    "Or get connection info with: virsh domdisplay {}",
                    self.config.name
                );
            }
            GraphicsBackend::VncOnly => {
                info!("Connect with: vncviewer localhost:5900");
            }
        }

        info!("VM started successfully!");

        Ok(())
    }

    /// Stop the VM gracefully
    fn stop_vm(&self) -> Result<()> {
        info!("Stopping VM: {}", self.config.name);

        // Hot-detach PCI devices if in hot-plug mode (before shutdown)
        if self.config.pci_hotplug && !self.config.pci_devices.is_empty() {
            thread::sleep(Duration::from_secs(2));
            self.detach_pci_devices_hotplug()?;
        }

        // Hot-detach USB devices if in hot-plug mode (before shutdown)
        if self.config.usb_hotplug && !self.config.usb_devices.is_empty() {
            self.detach_usb_devices_hotplug()?;
        }

        Command::new("virsh")
            .args(["-c", "qemu:///system", "shutdown", &self.config.name])
            .status()?;

        Ok(())
    }

    /// Destroy the VM and clean up resources
    fn destroy_vm(&self) -> Result<()> {
        info!("Destroying VM: {}", self.config.name);

        // Get VM IP before destroying (for SSH known_hosts cleanup)
        let vm_ip = {
            let output = Command::new("sudo")
                .args([
                    "virsh",
                    "-c",
                    "qemu:///system",
                    "domifaddr",
                    &self.config.name,
                ])
                .output();
            match output {
                Ok(out) if out.status.success() => {
                    let output_str = String::from_utf8_lossy(&out.stdout);
                    output_str
                        .lines()
                        .find(|line| line.contains("ipv4"))
                        .and_then(|line| line.split_whitespace().nth(3))
                        .and_then(|ip_part| ip_part.split('/').next())
                        .map(|s| s.to_string())
                }
                _ => None,
            }
        };

        // Check if VM exists first
        let list_output = Command::new("virsh")
            .args(["-c", "qemu:///system", "list", "--all"])
            .output()?;

        if !String::from_utf8_lossy(&list_output.stdout).contains(&self.config.name) {
            debug!("VM {} not found", self.config.name);
        } else {
            // Force stop if running
            debug!("Force stopping VM...");
            let destroy_output = Command::new("virsh")
                .args(["-c", "qemu:///system", "destroy", &self.config.name])
                .output();

            match destroy_output {
                Ok(output) => {
                    if output.status.success() {
                        debug!("VM stopped successfully");
                    } else {
                        debug!(
                            "VM stop failed or already stopped: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
                Err(e) => debug!("Error stopping VM: {}", e),
            }

            thread::sleep(Duration::from_secs(3));

            // Undefine VM (remove from libvirt)
            debug!("Removing VM definition...");
            let undefine_output = Command::new("virsh")
                .args([
                    "-c",
                    "qemu:///system",
                    "undefine",
                    &self.config.name,
                    "--remove-all-storage",
                    "--nvram",
                ])
                .output();

            match undefine_output {
                Ok(output) => {
                    if output.status.success() {
                        debug!("VM definition removed with storage");
                    } else {
                        debug!(
                            "Undefine with storage failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        debug!("Trying without storage flags...");

                        let simple_undefine = Command::new("virsh")
                            .args(["-c", "qemu:///system", "undefine", &self.config.name])
                            .output()?;

                        if simple_undefine.status.success() {
                            debug!("VM definition removed (without storage)");
                        } else {
                            error!(
                                "Simple undefine also failed: {}",
                                String::from_utf8_lossy(&simple_undefine.stderr)
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("Error running undefine: {}", e);
                }
            }
        }

        // Remove disk manually
        let disk_path = format!("{}/{}.qcow2", self.config.vm_dir, self.config.name);
        if Path::new(&disk_path).exists() {
            debug!("Removing disk image: {}", disk_path);
            match fs::remove_file(&disk_path) {
                Ok(_) => info!("Disk removed successfully"),
                Err(e) => {
                    debug!("Permission denied ({}), trying with sudo...", e);
                    let sudo_result = Command::new("sudo")
                        .args(["rm", "-f", &disk_path])
                        .output();

                    match sudo_result {
                        Ok(output) => {
                            if output.status.success() {
                                info!("Disk removed with sudo");
                            } else {
                                error!(
                                    "Failed to remove disk even with sudo: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
                            }
                        }
                        Err(e) => error!("Sudo command failed: {}", e),
                    }
                }
            }
        } else {
            debug!("Disk image not found at: {}", disk_path);
        }

        // Final verification
        let final_check = Command::new("virsh")
            .args(["-c", "qemu:///system", "list", "--all"])
            .output()?;

        if String::from_utf8_lossy(&final_check.stdout).contains(&self.config.name) {
            warn!("VM still appears in virsh list");
            warn!(
                "You may need to manually run: virsh undefine {}",
                self.config.name
            );
        } else {
            info!("VM successfully removed from libvirt");
        }

        // Clean up SSH known_hosts entry
        if let Some(ip) = vm_ip {
            debug!("Cleaning up SSH known_hosts for {}...", ip);
            let cleanup_result = Command::new("ssh-keygen").args(["-R", &ip]).output();
            match cleanup_result {
                Ok(output) if output.status.success() => {
                    info!("SSH known_hosts entry removed");
                }
                _ => {
                    debug!("No SSH entry found or cleanup skipped");
                }
            }
        }

        info!("VM destruction completed");

        Ok(())
    }
}
