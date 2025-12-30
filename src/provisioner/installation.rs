//! VM installation and provisioning orchestration
//!
//! This module handles the full VM provisioning workflow:
//! - Prerequisites checking
//! - ISO download
//! - Disk creation
//! - Kickstart-based installation
//! - SSH key acceptance

use crate::config::{GraphicsBackend, NetworkMode};
use crate::constants::{
    MIN_INSTALL_MEMORY_MB, POST_INSTALL_WAIT_SECS, SHUTDOWN_WAIT_SECS, SSH_RETRY_COUNT,
    SSH_RETRY_DELAY_SECS, VM_BOOT_RETRY_COUNT, VM_BOOT_RETRY_DELAY_SECS, VM_BOOT_WAIT_SECS,
};
use crate::error::{ProvisioningError, Result};
use crate::provisioner::kickstart::KickstartGeneration;
use crate::provisioner::network::NetworkManagement;
use crate::provisioner::pci::PciPassthrough;
use crate::provisioner::usb::UsbPassthrough;
use crate::virsh;
use log::{debug, error, info, warn};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Installation operations for AppVMProvisioner
pub trait Installation {
    fn provision_vm(&self) -> Result<()>;
    fn check_prerequisites(&self) -> Result<()>;
    fn download_fedora_iso(&self) -> Result<String>;
    fn create_vm_disk(&self) -> Result<String>;
    fn start_installation(&self, iso_path: &str, disk_path: &str, kickstart_path: &str) -> Result<()>;
    fn accept_ssh_host_key(&self) -> Result<()>;
    fn setup_window_management(&self) -> Result<()>;
}

impl Installation for super::AppVMProvisioner {
    /// Main provisioning orchestration - creates a complete VM
    fn provision_vm(&self) -> Result<()> {
        info!("Starting Application VM provisioning...");
        debug!("System packages: {:?}", self.config.system_packages);
        debug!("Flatpak packages: {:?}", self.config.flatpak_packages);

        // Check prerequisites
        self.check_prerequisites()?;

        // Validate PCI passthrough if devices specified
        if !self.config.pci_devices.is_empty() {
            self.validate_pci_passthrough()?;
        }

        // Download Fedora ISO (reused across VMs)
        let iso_path = self.download_fedora_iso()?;

        // Create VM disk
        let disk_path = self.create_vm_disk()?;

        // Generate kickstart configuration
        let kickstart_path = self.generate_kickstart_config()?;

        // Start automated installation
        self.start_installation(&iso_path, &disk_path, &kickstart_path)?;

        // Configure window management integration
        self.setup_window_management()?;

        // Setup PCI passthrough if devices specified (permanent mode)
        if !self.config.pci_devices.is_empty() && !self.config.pci_hotplug {
            self.setup_pci_passthrough_permanent()?;
        }

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
        debug!(
            "Clipboard: {}",
            if self.config.enable_clipboard {
                "Enabled"
            } else {
                "Disabled"
            }
        );

        Ok(())
    }

    /// Check that all prerequisites are installed
    fn check_prerequisites(&self) -> Result<()> {
        info!("Checking prerequisites...");

        let required_commands = [
            ("virsh", "libvirt"),
            ("virt-install", "virt-install"),
            ("qemu-img", "qemu-img"),
        ];

        for (cmd, package) in &required_commands {
            if Command::new("which").arg(cmd).output()?.status.success() {
                debug!("{} found", cmd);
            } else {
                return Err(ProvisioningError::MissingPrerequisite {
                    cmd: cmd.to_string(),
                    install_hint: format!("sudo dnf install {}", package),
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
                    install_hint: "sudo dnf install socat".to_string(),
                }
                .into());
            }
        }

        Ok(())
    }

    /// Download Fedora ISO if not already cached
    fn download_fedora_iso(&self) -> Result<String> {
        let arch = std::env::consts::ARCH;
        let iso_name = format!("Fedora-Server-dvd-{}.iso", arch);
        let iso_path = Path::new(&self.config.vm_dir)
            .join(&iso_name)
            .to_string_lossy()
            .to_string();

        // Ensure the VM directory exists
        let mkdir_status = Command::new("sudo")
            .args(["mkdir", "-p", &self.config.vm_dir])
            .status()?;
        if !mkdir_status.success() {
            return Err(ProvisioningError::IsoDownload("Failed to create VM directory".to_string()).into());
        }

        if Path::new(&iso_path).exists() {
            info!("Using existing Fedora ISO");
            return Ok(iso_path);
        }

        info!("Downloading Fedora Server ISO (~2GB)...");
        info!("This is a one-time download and will be reused for future VMs");

        let fedora_version = &self.config.fedora_version;
        let download_url = match arch {
            "x86_64" => format!(
                "https://download.fedoraproject.org/pub/fedora/linux/releases/{}/Server/x86_64/iso/Fedora-Server-dvd-x86_64-{}-1.2.iso",
                fedora_version, fedora_version
            ),
            "aarch64" => format!(
                "https://download.fedoraproject.org/pub/fedora/linux/releases/{}/Server/aarch64/iso/Fedora-Server-dvd-aarch64-{}-1.2.iso",
                fedora_version, fedora_version
            ),
            _ => return Err(ProvisioningError::UnsupportedArch(arch.to_string()).into()),
        };

        let status = Command::new("sudo")
            .args(["curl", "-L", "-o", &iso_path, "--progress-bar", &download_url])
            .status()?;

        if !status.success() {
            return Err(ProvisioningError::IsoDownload("curl failed".to_string()).into());
        }

        info!("Download complete");
        Ok(iso_path)
    }

    /// Create the VM disk image
    fn create_vm_disk(&self) -> Result<String> {
        let disk_path = Path::new(&self.config.vm_dir)
            .join(format!("{}.qcow2", self.config.name))
            .to_string_lossy()
            .to_string();

        // Remove existing disk if it exists
        Command::new("sudo").args(["rm", "-f", &disk_path]).status()?;

        info!("Creating VM disk ({} GB)...", self.config.disk_size_gb);

        Command::new("sudo")
            .args([
                "qemu-img",
                "create",
                "-f",
                "qcow2",
                &disk_path,
                &format!("{}G", self.config.disk_size_gb),
            ])
            .status()?;

        Ok(disk_path)
    }

    /// Start the automated installation process
    fn start_installation(
        &self,
        _iso_path: &str,
        disk_path: &str,
        kickstart_path: &str,
    ) -> Result<()> {
        info!("Starting VM installation...");

        // Use more RAM during installation if needed
        let install_memory = if self.config.memory_mb < MIN_INSTALL_MEMORY_MB {
            warn!(
                "Using {}MB RAM for installation (VM will use {}MB after first boot)",
                MIN_INSTALL_MEMORY_MB, self.config.memory_mb
            );
            MIN_INSTALL_MEMORY_MB
        } else {
            self.config.memory_mb
        };

        let arch = std::env::consts::ARCH;
        let fedora_version = &self.config.fedora_version;
        let install_location = match arch {
            "x86_64" => format!(
                "https://dl.fedoraproject.org/pub/fedora/linux/releases/{}/Server/x86_64/os/",
                fedora_version
            ),
            "aarch64" => format!(
                "https://dl.fedoraproject.org/pub/fedora/linux/releases/{}/Everything/aarch64/os/",
                fedora_version
            ),
            _ => return Err(ProvisioningError::UnsupportedArch(arch.to_string()).into()),
        };

        let memory_str = install_memory.to_string();
        let vcpus_str = self.config.vcpus.to_string();
        let disk_arg = format!(
            "path={},size={},format=qcow2,bus=virtio",
            disk_path, self.config.disk_size_gb
        );
        let osinfo = format!("fedora{}", fedora_version);

        // Configure graphics
        let graphics_args = self.build_graphics_args(arch);

        let mut virt_install_args = vec![
            "--name",
            &self.config.name,
            "--memory",
            &memory_str,
            "--vcpus",
            &vcpus_str,
            "--disk",
            &disk_arg,
            "--location",
            &install_location,
            "--initrd-inject",
            kickstart_path,
            "--extra-args",
            "inst.ks=file:/kickstart.cfg console=tty0 console=ttyS0,115200n8",
            "--osinfo",
            &osinfo,
            "--noautoconsole",
            "--wait",
            "-1",
        ];

        // Add graphics arguments
        for arg in &graphics_args {
            virt_install_args.push(arg);
        }

        // Add network configuration
        let network_arg = match &self.config.network_mode {
            NetworkMode::Bridge(bridge_name) => format!("bridge={},model=virtio", bridge_name),
            NetworkMode::Nat => "network=default,model=virtio".to_string(),
            NetworkMode::None => "network=default,model=virtio".to_string(), // Temporary for install
        };
        virt_install_args.extend_from_slice(&["--network", &network_arg]);

        // Add vsock device for network-disabled VMs
        if self.config.enable_vsock {
            virt_install_args.extend_from_slice(&["--vsock", "cid.auto=yes"]);
        }

        // Add sound if enabled
        if self.config.enable_audio {
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

        info!("Running automated installation (15-20 minutes)...");

        let status = Command::new("sudo")
            .arg("virt-install")
            .args(&virt_install_args)
            .status()?;

        if !status.success() {
            return Err(ProvisioningError::Installation(format!(
                "virt-install failed with exit code: {:?}",
                status.code()
            ))
            .into());
        }

        // Validate installation
        self.validate_installation(disk_path)?;

        // Reduce memory if we increased it
        if install_memory > self.config.memory_mb {
            self.reduce_memory_after_install()?;
        }

        // Accept SSH host key
        self.accept_ssh_host_key()?;

        // Stop the VM
        virsh::destroy_unchecked(&self.config.name);

        info!("Installation completed and validated!");

        Ok(())
    }

    /// Accept the VM's SSH host key for seamless Xpra connections
    fn accept_ssh_host_key(&self) -> Result<()> {
        info!("Adding VM SSH host key...");
        debug!("Waiting for VM networking to be ready...");

        // Retry getting VM IP address
        let mut vm_ip = None;
        for attempt in 1..=SSH_RETRY_COUNT {
            if let Some(ip) = virsh::get_vm_ip(&self.config.name) {
                vm_ip = Some(ip);
                break;
            }

            if attempt < SSH_RETRY_COUNT {
                thread::sleep(Duration::from_secs(SSH_RETRY_DELAY_SECS));
            }
        }

        let vm_ip = vm_ip.ok_or_else(|| {
            ProvisioningError::SshKeyAcceptance(format!(
                "Could not determine VM IP address after {} seconds",
                SSH_RETRY_COUNT * SSH_RETRY_DELAY_SECS as u32
            ))
        })?;
        debug!("VM IP: {}", vm_ip);

        // Get user's home directory
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            std::env::var("SUDO_USER")
                .ok()
                .and_then(|user| {
                    Command::new("getent")
                        .args(["passwd", &user])
                        .output()
                        .ok()
                        .and_then(|o| {
                            String::from_utf8(o.stdout)
                                .ok()
                                .and_then(|s| s.split(':').nth(5).map(|s| s.to_string()))
                        })
                })
                .unwrap_or_else(|| "/root".to_string())
        });

        let known_hosts = format!("{}/.ssh/known_hosts", home);

        // Use ssh-keyscan to get the host key
        debug!("Waiting for SSH server to be ready...");
        let mut scan_output = None;
        let ssh_scan_retries = SSH_RETRY_COUNT * 2; // Double retries for SSH scan
        for attempt in 1..=ssh_scan_retries {
            let output = Command::new("ssh-keyscan").args(["-H", &vm_ip]).output()?;

            if output.status.success() && !output.stdout.is_empty() {
                scan_output = Some(output);
                break;
            }

            if attempt < ssh_scan_retries {
                thread::sleep(Duration::from_secs(SSH_RETRY_DELAY_SECS));
            }
        }

        let output = scan_output.ok_or_else(|| {
            ProvisioningError::SshKeyAcceptance("Failed to scan SSH host key after 2 minutes".to_string())
        })?;

        // Append to known_hosts
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&known_hosts)?;

        file.write_all(&output.stdout)?;

        info!("SSH host key added to {}", known_hosts);

        Ok(())
    }

    /// Setup window management integration
    fn setup_window_management(&self) -> Result<()> {
        info!("Setting up window management integration...");

        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                debug!("Configured for VirtIO-GPU acceleration");
            }
            GraphicsBackend::QxlSpice => {
                debug!("Configured for SPICE protocol");
                debug!("Connect with: remote-viewer spice://localhost:5900");
            }
            GraphicsBackend::VncOnly => {
                debug!("VNC fallback mode");
                debug!("Connect with: vncviewer localhost:5900");
            }
        }

        if self.config.enable_clipboard {
            debug!("Clipboard sharing enabled (requires host agent)");
        }

        Ok(())
    }
}

impl super::AppVMProvisioner {
    /// Build graphics arguments for virt-install
    fn build_graphics_args(&self, arch: &str) -> Vec<&'static str> {
        if self.config.headless {
            return vec!["--graphics", "none"];
        }

        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                if arch == "aarch64" {
                    vec![
                        "--graphics",
                        "spice",
                        "--video",
                        "virtio",
                        "--channel",
                        "spicevmc,target_type=virtio,name=com.redhat.spice.0",
                    ]
                } else {
                    vec![
                        "--graphics",
                        "spice,listen=127.0.0.1",
                        "--video",
                        "qxl",
                        "--channel",
                        "spicevmc,target_type=virtio,name=com.redhat.spice.0",
                    ]
                }
            }
            GraphicsBackend::QxlSpice => {
                if arch == "aarch64" {
                    vec![
                        "--graphics",
                        "spice",
                        "--video",
                        "virtio",
                        "--channel",
                        "spicevmc,target_type=virtio,name=com.redhat.spice.0",
                    ]
                } else {
                    vec![
                        "--graphics",
                        "spice,listen=127.0.0.1",
                        "--video",
                        "qxl",
                        "--channel",
                        "spicevmc,target_type=virtio,name=com.redhat.spice.0",
                    ]
                }
            }
            GraphicsBackend::VncOnly => {
                vec!["--graphics", "vnc,listen=127.0.0.1,port=5900"]
            }
        }
    }

    /// Validate that installation succeeded
    fn validate_installation(&self, disk_path: &str) -> Result<()> {
        use crate::constants::MIN_DISK_SIZE_MB;

        info!("Validating installation...");

        // Check disk size
        if let Ok(metadata) = fs::metadata(disk_path) {
            let disk_size_mb = metadata.len() / (1024 * 1024);
            debug!("Disk size: {} MB", disk_size_mb);

            if disk_size_mb < MIN_DISK_SIZE_MB {
                error!(
                    "Installation failed: Disk size is only {} MB (expected at least {} MB)",
                    disk_size_mb, MIN_DISK_SIZE_MB
                );
                error!("This usually means the installer ran out of memory or disk space.");
                error!("Try increasing RAM with --memory 3072 or --memory 4096");
                return Err(ProvisioningError::Validation("disk too small".to_string()).into());
            }
        } else {
            warn!("Could not check disk size");
        }

        // Check VM state - retry a few times to handle post-install reboot
        debug!("Checking VM status (waiting for post-install reboot to complete)...");

        // Give the VM time to stabilize after installation reboot
        thread::sleep(Duration::from_secs(POST_INSTALL_WAIT_SECS));

        let mut vm_running = false;

        for attempt in 1..=VM_BOOT_RETRY_COUNT {
            let vm_state = virsh::get_vm_state(&self.config.name).unwrap_or_default();

            debug!(
                "Attempt {}/{}: VM state is '{}'",
                attempt, VM_BOOT_RETRY_COUNT, vm_state
            );

            if vm_state == "running" {
                debug!("VM is running");
                vm_running = true;
                break;
            }

            // If shut off, try to start it
            if vm_state == "shut off" {
                debug!("VM is shut off, attempting to start...");
                if virsh::start_if_stopped(&self.config.name) {
                    debug!("VM started successfully");
                    vm_running = true;
                    break;
                }
                // Check if already running (race condition)
                if virsh::is_vm_running(&self.config.name) {
                    debug!("VM is already active");
                    vm_running = true;
                    break;
                }
                debug!("Start failed, will retry...");
            }

            // Wait before retry (VM might be mid-reboot)
            if attempt < VM_BOOT_RETRY_COUNT {
                debug!("Waiting {}s before retry...", VM_BOOT_RETRY_DELAY_SECS);
                thread::sleep(Duration::from_secs(VM_BOOT_RETRY_DELAY_SECS));
            }
        }

        if !vm_running {
            // One final check - maybe it came up while we were in the loop
            let final_state = virsh::get_vm_state(&self.config.name).unwrap_or_default();

            if final_state != "running" {
                error!(
                    "Installation failed: VM will not start after {} attempts",
                    VM_BOOT_RETRY_COUNT
                );
                error!("Final state: {}", final_state);
                return Err(ProvisioningError::Validation("VM won't boot".to_string()).into());
            }
        }

        // Give the VM a moment to fully boot
        thread::sleep(Duration::from_secs(VM_BOOT_WAIT_SECS));

        Ok(())
    }

    /// Reduce memory after installation if it was temporarily increased
    fn reduce_memory_after_install(&self) -> Result<()> {
        info!("Reducing VM memory to {}MB...", self.config.memory_mb);

        // Stop the VM
        virsh::shutdown_unchecked(&self.config.name);

        // Wait for shutdown
        for _ in 0..SHUTDOWN_WAIT_SECS {
            thread::sleep(Duration::from_secs(1));
            if let Some(state) = virsh::get_vm_state(&self.config.name) {
                if state == "shut off" {
                    break;
                }
            }
        }

        // Update memory configuration
        virsh::set_memory(&self.config.name, self.config.memory_mb, true)?; // setmaxmem
        virsh::set_memory(&self.config.name, self.config.memory_mb, false)?; // setmem

        debug!("Memory reduced to {}MB", self.config.memory_mb);

        // Start the VM again
        debug!("Starting VM with new memory configuration...");
        virsh::start_if_stopped(&self.config.name);

        thread::sleep(Duration::from_secs(VM_BOOT_WAIT_SECS));

        Ok(())
    }
}
