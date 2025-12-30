//! VM creation command handler
//!
//! This module handles the `create` subcommand for provisioning new VMs.

use dialoguer::Confirm;
use log::{debug, error, info, warn};
use std::path::Path;

use vm_provisioner::config::{AppVMConfig, NetworkMode, SharedFolder};
use vm_provisioner::error::{ConfigError, NetworkError, Result};
use vm_provisioner::passwords::VMPasswords;
use vm_provisioner::provisioner::{self, AppVMProvisioner, Installation};

/// Options for VM creation
pub struct CreateOptions {
    pub name: Option<String>,
    pub system_packages: Vec<String>,
    pub flatpak_packages: Vec<String>,
    pub skip_confirm: bool,
    pub config_path: Option<String>,
    pub memory: u64,
    pub vcpus: u32,
    pub disk: u64,
    pub headless: bool,
    pub pci_addresses: Vec<String>,
    pub pci_hotplug: bool,
    pub web_port: Option<u16>,
    pub usb_addresses: Vec<String>,
    pub usb_hotplug: bool,
    pub share_paths: Vec<String>,
    pub share_readonly: bool,
    pub network_bridge: Option<String>,
    pub grant_device_access: bool,
    pub no_network: bool,
}

/// Validate create options for mutual exclusivity and prerequisites
pub fn validate_create_options(opts: &CreateOptions) -> Result<()> {
    // Validate --no-network and --web-port are mutually exclusive
    if opts.no_network && opts.web_port.is_some() {
        return Err(NetworkError::ConflictingOptions(
            "Cannot use --no-network with --web-port. Selkies web streaming requires networking."
                .to_string(),
        )
        .into());
    }

    // Validate --no-network and --network-bridge are mutually exclusive
    if opts.no_network && opts.network_bridge.is_some() {
        return Err(NetworkError::ConflictingOptions(
            "Cannot use --no-network with --network-bridge. These options are mutually exclusive."
                .to_string(),
        )
        .into());
    }

    if opts.no_network {
        info!("Network-disabled mode: VM will use vsock for display forwarding");
    }

    // Validate bridge interface exists if specified
    if let Some(ref bridge) = opts.network_bridge {
        let bridge_path = format!("/sys/class/net/{}", bridge);
        if !Path::new(&bridge_path).exists() {
            error!("Bridge interface '{}' not found.", bridge);
            println!("\nTo create a bridge (one-time setup):");
            println!("  1. Find your network interface: nmcli device status");
            println!("  2. Create bridge:");
            println!(
                "     sudo nmcli connection add type bridge ifname {} con-name {}",
                bridge, bridge
            );
            println!(
                "     sudo nmcli connection add type bridge-slave ifname <your-interface> master {}",
                bridge
            );
            println!("     sudo nmcli connection up {}", bridge);
            return Err(NetworkError::BridgeNotFound(bridge.clone()).into());
        }
        info!("Using bridged networking via '{}'", bridge);
    }

    Ok(())
}

/// Build VM config from options, detecting devices as needed
pub fn build_config(opts: CreateOptions) -> Result<AppVMConfig> {
    if let Some(path) = opts.config_path {
        return Ok(toml::from_str::<AppVMConfig>(&std::fs::read_to_string(
            path,
        )?)?);
    }

    let vm_name = opts.name.unwrap_or_else(|| {
        if !opts.flatpak_packages.is_empty() {
            format!("{}-vm", opts.flatpak_packages[0].replace('.', "-"))
        } else if !opts.system_packages.is_empty() {
            format!("{}-vm", opts.system_packages[0])
        } else {
            format!(
                "app-vm-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System clock is set before UNIX epoch")
                    .as_secs()
            )
        }
    });

    // Validate VM name
    AppVMConfig::validate_vm_name(&vm_name)?;

    let pci_devices = if !opts.pci_addresses.is_empty() {
        info!("Detecting PCI devices...");
        opts.pci_addresses
            .iter()
            .map(|addr| provisioner::detect_pci_device(addr))
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    let usb_devices = if !opts.usb_addresses.is_empty() {
        info!("Detecting USB devices...");
        opts.usb_addresses
            .iter()
            .map(|addr| provisioner::detect_usb_device(addr))
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    let shared_folders = if !opts.share_paths.is_empty() {
        info!("Configuring shared folders...");
        opts.share_paths
            .iter()
            .map(|path| parse_share_path(path, opts.share_readonly))
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    Ok(AppVMConfig::new(
        vm_name,
        opts.memory,
        opts.vcpus,
        opts.disk,
        opts.system_packages,
        opts.flatpak_packages,
        opts.headless,
        pci_devices,
        opts.pci_hotplug,
        opts.web_port,
        usb_devices,
        opts.usb_hotplug,
        shared_folders,
        opts.network_bridge,
        opts.grant_device_access,
        opts.no_network,
    ))
}

/// Display VM configuration summary
pub fn display_config_summary(config: &AppVMConfig) {
    println!("\n📋 VM Configuration:");
    println!("   Name: {}", config.name);
    println!(
        "   Mode: {}",
        if config.headless {
            "Headless (CLI only)"
        } else {
            "GUI (Xpra)"
        }
    );
    if let Some(port) = config.web_port {
        println!(
            "   Web Streaming: http://<vm-ip>:{}/ (Selkies WebRTC)",
            port
        );
    }
    println!("   System Packages: {:?}", config.system_packages);
    println!("   Flatpak Packages: {:?}", config.flatpak_packages);
    println!("   Memory: {} MB", config.memory_mb);
    println!("   vCPUs: {}", config.vcpus);
    println!("   Disk: {} GB", config.disk_size_gb);
    if !config.usb_devices.is_empty() {
        println!(
            "   USB Devices: {} device(s){}",
            config.usb_devices.len(),
            if config.usb_hotplug {
                " (hot-plug)"
            } else {
                " (permanent)"
            }
        );
        for usb in &config.usb_devices {
            println!(
                "     - {} ({}:{})",
                usb.description, usb.vendor_id, usb.product_id
            );
        }
    }
    if !config.shared_folders.is_empty() {
        println!(
            "   Shared Folders: {} folder(s)",
            config.shared_folders.len()
        );
        for folder in &config.shared_folders {
            println!(
                "     - {} -> {} ({})",
                folder.host_path,
                folder.guest_path,
                if folder.readonly {
                    "read-only"
                } else {
                    "read-write"
                }
            );
        }
    }
    match &config.network_mode {
        NetworkMode::None => println!("   Network: Disabled (vsock for display)"),
        NetworkMode::Nat => println!("   Network: NAT (192.168.122.x)"),
        NetworkMode::Bridge(br) => println!("   Network: Bridged ({})", br),
    }
}

/// Save VM config and password to disk
pub fn save_config_and_passwords(config: &AppVMConfig) -> Result<String> {
    let config_dir = AppVMConfig::config_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    let config_file = AppVMConfig::config_path(&config.name)?;
    std::fs::write(&config_file, toml::to_string_pretty(config)?)?;
    info!("Configuration saved to: {}", config_file);

    let mut passwords = VMPasswords::load_or_create(&config_dir)?;
    passwords.add_vm(&config.name, &config.user_password);
    passwords.save(&config_dir)?;

    Ok(config_file)
}

/// Handle post-provisioning tasks like vsock CID retrieval
pub fn handle_post_provisioning(config: &mut AppVMConfig, config_file: &str) -> Result<()> {
    if config.enable_vsock {
        match provisioner::get_vsock_cid(&config.name) {
            Ok(cid) => {
                info!("Vsock CID assigned: {}", cid);
                config.vsock_cid = Some(cid);
                // Re-save config with CID
                std::fs::write(config_file, toml::to_string_pretty(config)?)?;
            }
            Err(e) => {
                warn!("Could not retrieve vsock CID: {}", e);
                warn!(
                    "Display forwarding may not work. Check 'virsh dumpxml {}'",
                    config.name
                );
            }
        }
    }
    Ok(())
}

/// Display success message after VM creation
pub fn display_success_message(config: &AppVMConfig) {
    println!("\n✅ VM created successfully!");
    println!("   VM Name: {}", config.name);
    println!("   Username: user");
    println!("   Password: {}", config.user_password);
    if config.enable_vsock {
        println!("   Vsock CID: {}", config.vsock_cid.unwrap_or(0));
    }
    println!("   Start with: vm-provisioner start {}", config.name);
}

/// Parse share path argument into SharedFolder struct
pub fn parse_share_path(path: &str, readonly: bool) -> Result<SharedFolder> {
    let parts: Vec<&str> = path.split(':').collect();
    if parts.len() != 2 {
        return Err(ConfigError::Invalid(format!(
            "Invalid share format '{}'. Expected format: '/host/path:/guest/path'",
            path
        ))
        .into());
    }

    let host_path = parts[0];
    let guest_path = parts[1];

    // Validate host path exists
    if !Path::new(host_path).exists() {
        return Err(
            ConfigError::Invalid(format!("Host path '{}' does not exist", host_path)).into(),
        );
    }

    // Validate host path is a directory
    if !Path::new(host_path).is_dir() {
        return Err(ConfigError::Invalid(format!(
            "Host path '{}' is not a directory",
            host_path
        ))
        .into());
    }

    // Validate guest path is absolute
    if !guest_path.starts_with('/') {
        return Err(ConfigError::Invalid(format!(
            "Guest path '{}' must be absolute (start with /)",
            guest_path
        ))
        .into());
    }

    // Generate a tag from guest path (replace / with _ and remove leading _)
    let tag = guest_path
        .trim_start_matches('/')
        .replace('/', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();

    if tag.is_empty() {
        return Err(ConfigError::Invalid(
            "Guest path must contain alphanumeric characters".to_string(),
        )
        .into());
    }

    debug!(
        "{} -> {} ({})",
        host_path,
        guest_path,
        if readonly { "read-only" } else { "read-write" }
    );

    Ok(SharedFolder {
        host_path: host_path.to_string(),
        guest_path: guest_path.to_string(),
        tag,
        readonly,
    })
}

/// Main entry point for the create command
pub fn create_vm(opts: CreateOptions) -> Result<()> {
    info!("VM Provisioner - Dynamic Package Installer");
    println!("==============================================");

    // Validate options
    validate_create_options(&opts)?;

    // Build configuration
    let skip_confirm = opts.skip_confirm;
    let config = build_config(opts)?;

    // Display summary
    display_config_summary(&config);

    // Confirm with user
    if !skip_confirm
        && !Confirm::new()
            .with_prompt("Proceed with VM creation?")
            .default(true)
            .interact()?
    {
        warn!("VM creation cancelled");
        return Ok(());
    }

    // Save config and passwords
    let config_file = save_config_and_passwords(&config)?;

    // Provision VM
    let mut config = config;
    AppVMProvisioner::new(config.clone()).provision_vm()?;

    // Handle post-provisioning tasks
    handle_post_provisioning(&mut config, &config_file)?;

    // Display success message
    display_success_message(&config);

    Ok(())
}
