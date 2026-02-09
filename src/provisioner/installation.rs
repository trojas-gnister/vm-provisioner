//! VM installation and provisioning orchestration
//!
//! This module handles the full VM provisioning workflow using NixOS:
//! - Prerequisites checking
//! - NixOS configuration generation
//! - qcow2 image building via nixos-generators
//! - VM import via virt-install --import

use crate::config::{GraphicsBackend, NetworkMode};
use crate::error::{ProvisioningError, Result};
use crate::nixos::{config_gen, image_builder};
use crate::provisioner::device_detection::{
    detect_gpu_render_nodes, get_vulkan_icd_path, select_gpu_for_venus,
};
use crate::provisioner::network::NetworkManagement;
use crate::provisioner::usb::UsbPassthrough;
use crate::virsh;
use log::{debug, info, warn};
use std::io::Write;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Installation operations for AppVMProvisioner
pub trait Installation {
    fn provision_vm(&self) -> Result<()>;
    fn check_prerequisites(&self) -> Result<()>;
}

impl Installation for super::AppVMProvisioner {
    /// Main provisioning orchestration - creates a complete VM
    fn provision_vm(&self) -> Result<()> {
        info!("Starting Application VM provisioning...");
        debug!("System packages: {:?}", self.config.system_packages);
        debug!("Flatpak packages: {:?}", self.config.flatpak_packages);

        // Check prerequisites
        self.check_prerequisites()?;

        // Generate NixOS configuration
        info!("Generating NixOS configuration...");
        let nix_config = config_gen::generate_configuration_nix(&self.config)?;

        // Build qcow2 image
        let qcow2_path = image_builder::build_image(
            &nix_config,
            &self.config.name,
            &self.config.vm_dir,
            self.config.disk_size_gb,
        )?;

        // Import VM via virt-install
        self.import_vm(&qcow2_path.to_string_lossy())?;

        // Setup USB passthrough if devices specified (permanent mode)
        if !self.config.usb_devices.is_empty() && !self.config.usb_hotplug {
            self.setup_usb_passthrough_permanent()?;
        }

        // Remove network interface for NetworkMode::None VMs
        if matches!(self.config.network_mode, NetworkMode::None) {
            info!("Network mode is None, removing network interface...");
            self.remove_network_interface()?;
        } else {
            debug!("Network mode: {:?}", self.config.network_mode);
        }

        info!("Application VM provisioned successfully!");
        info!("VM Name: {}", self.config.name);
        debug!("System packages: {:?}", self.config.system_packages);
        debug!("Flatpak packages: {:?}", self.config.flatpak_packages);
        debug!("Graphics: {:?}", self.config.graphics_backend);

        Ok(())
    }

    /// Check that all prerequisites are installed
    fn check_prerequisites(&self) -> Result<()> {
        info!("Checking prerequisites...");

        let required_commands = [
            ("virsh", "Install libvirt for your distribution"),
            ("virt-install", "Install virt-install for your distribution"),
            ("nixos-generate", "nix-env -iA nixpkgs.nixos-generators (or use nix flakes)"),
        ];

        for (cmd, install_hint) in &required_commands {
            if Command::new("which").arg(cmd).output()?.status.success() {
                debug!("{} found", cmd);
            } else {
                return Err(ProvisioningError::MissingPrerequisite {
                    cmd: cmd.to_string(),
                    install_hint: install_hint.to_string(),
                }
                .into());
            }
        }

        // Check if libvirtd is running
        let status = Command::new("systemctl")
            .args(["is-active", "libvirtd"])
            .output()?;

        if !status.status.success() {
            warn!("Starting libvirtd...");
            Command::new("sudo")
                .args(["systemctl", "start", "libvirtd"])
                .status()?;
        }

        // Check vsock prerequisites if network disabled
        if self.config.enable_vsock {
            let modprobe = Command::new("sudo")
                .args(["modprobe", "vhost_vsock"])
                .status()?;
            if modprobe.success() {
                debug!("vhost_vsock module loaded");
            } else {
                return Err(ProvisioningError::MissingPrerequisite {
                    cmd: "vhost_vsock".to_string(),
                    install_hint: "This kernel module is required for vsock".to_string(),
                }
                .into());
            }

            if Command::new("which")
                .arg("socat")
                .output()?
                .status
                .success()
            {
                debug!("socat found");
            } else {
                return Err(ProvisioningError::MissingPrerequisite {
                    cmd: "socat".to_string(),
                    install_hint: "Install socat for your distribution".to_string(),
                }
                .into());
            }
        }

        // Check libvirt-qemu user groups for GPU access (VirtioGpu only)
        if matches!(self.config.graphics_backend, GraphicsBackend::VirtioGpu) && !self.config.headless {
            let qemu_user = Command::new("id").arg("libvirt-qemu").output();
            let id_output = match qemu_user {
                Ok(ref o) if o.status.success() => {
                    Some(String::from_utf8_lossy(&o.stdout).to_string())
                }
                _ => {
                    // Try 'qemu' as fallback (some distros use this)
                    let fallback = Command::new("id").arg("qemu").output();
                    match fallback {
                        Ok(ref o) if o.status.success() => {
                            Some(String::from_utf8_lossy(&o.stdout).to_string())
                        }
                        _ => None,
                    }
                }
            };

            if let Some(groups) = id_output {
                let has_render = groups.contains("render");
                let has_video = groups.contains("video");
                if !has_render || !has_video {
                    let mut missing = Vec::new();
                    if !has_render {
                        missing.push("render");
                    }
                    if !has_video {
                        missing.push("video");
                    }
                    warn!(
                        "libvirt-qemu user should be in '{}' group(s) for GPU access. \
                         Add with: sudo usermod -aG {} libvirt-qemu",
                        missing.join("' and '"),
                        missing.join(",")
                    );
                }
            }
        }

        Ok(())
    }
}

impl super::AppVMProvisioner {
    /// Import the built qcow2 image into libvirt via virt-install --import
    fn import_vm(&self, qcow2_path: &str) -> Result<()> {
        info!("Importing NixOS image into libvirt...");

        let memory_str = self.config.memory_mb.to_string();
        let vcpus_str = self.config.vcpus.to_string();
        let disk_arg = format!("path={},format=qcow2,bus=virtio", qcow2_path);

        // Build graphics arguments
        let graphics_args = self.build_graphics_args();

        let mut virt_install_args = vec![
            "--name",
            &self.config.name,
            "--memory",
            &memory_str,
            "--vcpus",
            &vcpus_str,
            "--disk",
            &disk_arg,
            "--import",
            "--os-variant",
            "nixos-unstable",
            "--noautoconsole",
        ];

        // Add graphics arguments
        for arg in &graphics_args {
            virt_install_args.push(arg);
        }

        // Add network configuration
        let network_arg = match &self.config.network_mode {
            NetworkMode::Bridge(bridge_name) => format!("bridge={},model=virtio", bridge_name),
            NetworkMode::Nat => "network=default,model=virtio".to_string(),
            NetworkMode::None => "none".to_string(),
        };
        virt_install_args.extend_from_slice(&["--network", &network_arg]);

        // Add vsock device for network-disabled VMs
        if self.config.enable_vsock {
            virt_install_args.extend_from_slice(&["--vsock", "cid.auto=yes"]);
        }

        // Add sound if enabled
        if self.config.enable_audio {
            let arch = std::env::consts::ARCH;
            if arch == "aarch64" {
                virt_install_args.extend_from_slice(&["--sound", "model=virtio"]);
            } else {
                virt_install_args.extend_from_slice(&["--sound", "default"]);
            }
        }

        // Add USB controller if needed
        if self.config.enable_usb_passthrough {
            virt_install_args.extend_from_slice(&["--controller", "usb,model=qemu-xhci"]);
        }

        // Add virtiofs shared folders
        let filesystem_args: Vec<String> = self
            .config
            .shared_folders
            .iter()
            .map(|folder| {
                format!(
                    "source={},target={},driver.type=virtiofs",
                    folder.host_path, folder.tag
                )
            })
            .collect();
        for fs_arg in &filesystem_args {
            virt_install_args.extend_from_slice(&["--filesystem", fs_arg]);
        }

        // Add memory backing for virtiofs
        if !self.config.shared_folders.is_empty() {
            virt_install_args
                .extend_from_slice(&["--memorybacking", "source.type=memfd,access.mode=shared"]);
        }

        let status = Command::new("sudo")
            .arg("virt-install")
            .args(&virt_install_args)
            .status()?;

        if !status.success() {
            return Err(ProvisioningError::Installation(format!(
                "virt-install --import failed with exit code: {:?}",
                status.code()
            ))
            .into());
        }

        // Stop the VM after import (user starts it explicitly)
        thread::sleep(Duration::from_secs(5));
        virsh::destroy_unchecked(&self.config.name);

        // Enable Venus Vulkan for VirtioGpu (non-headless) VMs
        if matches!(self.config.graphics_backend, GraphicsBackend::VirtioGpu) && !self.config.headless
        {
            self.enable_venus_vulkan()?;
        }

        info!("VM imported and ready.");
        Ok(())
    }

    /// Enable Venus Vulkan by post-processing the libvirt XML
    ///
    /// virt-install doesn't support the `blob=on` and `venus=on` flags needed for
    /// Venus Vulkan on virtio-gpu. This method modifies the VM's XML definition
    /// after creation to add them. Also sets VK_ICD_FILENAMES to select the
    /// correct Vulkan ICD on multi-GPU hosts.
    fn enable_venus_vulkan(&self) -> Result<()> {
        info!("Enabling Venus Vulkan for VM '{}'...", self.config.name);

        let xml = virsh::dumpxml(&self.config.name)?;

        // Add qemu namespace to domain element
        let xml = xml.replace(
            "<domain type='kvm'>",
            "<domain type='kvm' xmlns:qemu='http://libvirt.org/schemas/domain/qemu/1.0'>",
        );

        // Replace the libvirt-managed <video> block with type='none' to prevent
        // libvirt from adding a default VGA device that conflicts with our QEMU CLI device
        let xml = replace_xml_block(
            &xml,
            "<video>",
            "</video>",
            "    <video>\n      <model type='none'/>\n    </video>",
        );

        // Use KiB for memory-backend-memfd size to match libvirt's <memory> unit
        let mem_kb = self.config.memory_mb * 1024;
        let mem_size = format!("{}K", mem_kb);
        let device_arg = format!(
            "virtio-vga-gl,hostmem={},blob=true,venus=true",
            mem_size
        );
        let memfd_arg = format!(
            "memory-backend-memfd,id=mem1,size={}",
            mem_size
        );

        // Detect GPU and get ICD path for VK_ICD_FILENAMES env var
        let gpu_nodes = detect_gpu_render_nodes();
        let selected_gpu = select_gpu_for_venus(&gpu_nodes);
        let icd_env = selected_gpu
            .and_then(|gpu| get_vulkan_icd_path(&gpu.vendor))
            .map(|icd_path| {
                info!("Setting VK_ICD_FILENAMES={}", icd_path);
                format!(
                    "\n    <qemu:env name='VK_ICD_FILENAMES' value='{}'/>",
                    icd_path
                )
            })
            .unwrap_or_default();

        let qemu_block = format!(
            "  <qemu:commandline>\n    \
               <qemu:arg value='-device'/>\n    \
               <qemu:arg value='{device_arg}'/>\n    \
               <qemu:arg value='-object'/>\n    \
               <qemu:arg value='{memfd_arg}'/>\n    \
               <qemu:arg value='-machine'/>\n    \
               <qemu:arg value='memory-backend=mem1'/>\n    \
               <qemu:arg value='-vga'/>\n    \
               <qemu:arg value='none'/>{icd_env}\n  \
             </qemu:commandline>\n</domain>"
        );
        let xml = xml.replace("</domain>", &qemu_block);

        // Write modified XML to a temp file
        let mut tmp = tempfile::NamedTempFile::new().map_err(|e| {
            ProvisioningError::Installation(format!("Failed to create temp file: {}", e))
        })?;
        tmp.write_all(xml.as_bytes()).map_err(|e| {
            ProvisioningError::Installation(format!("Failed to write XML: {}", e))
        })?;
        let tmp_path = tmp.path().to_string_lossy().to_string();

        // Redefine the VM with the modified XML
        virsh::undefine(&self.config.name, false)?;
        virsh::define(&tmp_path)?;

        info!("Venus Vulkan enabled (blob=on, venus=on).");
        Ok(())
    }

    /// Build graphics arguments for virt-install
    fn build_graphics_args(&self) -> Vec<String> {
        if self.config.headless {
            return vec!["--graphics".to_string(), "none".to_string()];
        }

        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                // Detect GPU render nodes and select the best one for Venus
                let gpu_nodes = detect_gpu_render_nodes();
                let selected_gpu = select_gpu_for_venus(&gpu_nodes);

                let spice_arg = if let Some(gpu) = selected_gpu {
                    info!(
                        "Using GPU render node {} ({:?}) for SPICE",
                        gpu.by_path, gpu.vendor
                    );
                    format!(
                        "spice,gl.enable=yes,listen=none,rendernode={}",
                        gpu.by_path
                    )
                } else {
                    warn!("No suitable GPU render node found, using default SPICE GL");
                    "spice,gl.enable=yes,listen=none".to_string()
                };

                vec![
                    "--graphics".to_string(),
                    spice_arg,
                    "--video".to_string(),
                    "virtio,model.heads=1,model.acceleration.accel3d=yes".to_string(),
                ]
            }
            GraphicsBackend::VncOnly => {
                vec![
                    "--graphics".to_string(),
                    "vnc,listen=127.0.0.1,port=5900".to_string(),
                ]
            }
        }
    }
}

/// Replace an XML block (inclusive of start/end tags) with a replacement string
fn replace_xml_block(xml: &str, start_tag: &str, end_tag: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(xml.len());
    let mut skipping = false;
    let mut replaced = false;
    for line in xml.lines() {
        if !skipping && line.trim_start().starts_with(start_tag) {
            skipping = true;
        }
        if skipping {
            if line.trim_start().starts_with(end_tag) {
                skipping = false;
                if !replaced {
                    result.push_str(replacement);
                    result.push('\n');
                    replaced = true;
                }
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}
