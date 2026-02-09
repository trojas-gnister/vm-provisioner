//! VM lifecycle management (start, stop, destroy)
//!
//! This module handles runtime VM operations:
//! - Starting VMs (with optional hot-plug)
//! - Stopping VMs (with graceful shutdown)
//! - Destroying VMs (cleanup)

use crate::config::GraphicsBackend;
use crate::constants::{DEFAULT_SPICE_PORT, VM_BOOT_WAIT_SECS};
use crate::error::Result;
use crate::provisioner::usb::UsbPassthrough;
use crate::virsh;
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

        virsh::start(&self.config.name)?;

        // Wait for VM to boot
        thread::sleep(Duration::from_secs(VM_BOOT_WAIT_SECS));

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
            GraphicsBackend::VirtioGpu => {
                info!("Launching SPICE viewer...");
                let vm_name = self.config.name.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(VM_BOOT_WAIT_SECS));

                    // Get the actual SPICE port from virsh
                    if let Some(display) = virsh::get_display(&vm_name) {
                        let _ = Command::new("remote-viewer").arg(&display).spawn();
                        return;
                    }

                    // Fallback to default port
                    let _ = Command::new("remote-viewer")
                        .arg(format!("spice://127.0.0.1:{}", DEFAULT_SPICE_PORT))
                        .spawn();
                });
                debug!("SPICE viewer will launch automatically");
                debug!(
                    "Or get connection info with: virsh domdisplay {}",
                    self.config.name
                );
            }
            GraphicsBackend::VncOnly => {
                info!("Connect with: vncviewer localhost:{}", DEFAULT_SPICE_PORT);
            }
        }

        info!("VM started successfully!");

        Ok(())
    }

    /// Stop the VM gracefully
    fn stop_vm(&self) -> Result<()> {
        info!("Stopping VM: {}", self.config.name);

        // Hot-detach USB devices if in hot-plug mode (before shutdown)
        if self.config.usb_hotplug && !self.config.usb_devices.is_empty() {
            self.detach_usb_devices_hotplug()?;
        }

        virsh::shutdown(&self.config.name)?;

        Ok(())
    }

    /// Destroy the VM and clean up resources
    fn destroy_vm(&self) -> Result<()> {
        info!("Destroying VM: {}", self.config.name);

        // Get VM IP before destroying (for SSH known_hosts cleanup)
        let vm_ip = virsh::get_vm_ip(&self.config.name);

        // Check if VM exists first
        if !virsh::domain_exists(&self.config.name) {
            debug!("VM {} not found", self.config.name);
        } else {
            // Force stop if running
            debug!("Force stopping VM...");
            if virsh::destroy_unchecked(&self.config.name) {
                debug!("VM stopped successfully");
            } else {
                debug!("VM stop failed or already stopped");
            }

            thread::sleep(Duration::from_secs(3));

            // Undefine VM (remove from libvirt)
            debug!("Removing VM definition...");
            if virsh::undefine(&self.config.name, true).is_ok() {
                debug!("VM definition removed with storage");
            } else {
                debug!("Undefine with storage failed, trying without storage flags...");
                if virsh::undefine(&self.config.name, false).is_ok() {
                    debug!("VM definition removed (without storage)");
                } else {
                    error!("Failed to undefine VM");
                }
            }
        }

        // Remove disk manually
        let disk_path = Path::new(&self.config.vm_dir).join(format!("{}.qcow2", self.config.name));
        if disk_path.exists() {
            debug!("Removing disk image: {}", disk_path.display());
            match fs::remove_file(&disk_path) {
                Ok(_) => info!("Disk removed successfully"),
                Err(e) => {
                    debug!("Permission denied ({}), trying with sudo...", e);
                    let sudo_result = Command::new("sudo")
                        .args(["rm", "-f", disk_path.to_str().unwrap_or_default()])
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
            debug!("Disk image not found at: {}", disk_path.display());
        }

        // Final verification
        if virsh::domain_exists(&self.config.name) {
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
