mod config;
mod display_bridge;
mod error;
mod provisioner;
mod templates;
mod xpra_manager;

use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use config::{AppVMConfig, SharedFolder};
use display_bridge::DisplayBridge;
use error::{ConfigError, NetworkError, Result};
use provisioner::{detect_usb_device, AppVMProvisioner, Installation, Lifecycle};
use provisioner::usb::UsbPassthrough;
use xpra_manager::XpraManager;

#[derive(Debug, Serialize, Deserialize)]
struct VMPasswords {
    vms: HashMap<String, String>,
}

impl VMPasswords {
    fn new() -> Self {
        Self {
            vms: HashMap::new(),
        }
    }

    fn load_or_create(config_dir: &str) -> Result<Self> {
        let password_file = format!("{}/vm-passwords.toml", config_dir);

        if Path::new(&password_file).exists() {
            let content = std::fs::read_to_string(&password_file)?;
            Ok(toml::from_str(&content).unwrap_or_else(|_| Self::new()))
        } else {
            Ok(Self::new())
        }
    }

    fn save(&self, config_dir: &str) -> Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let password_file = format!("{}/vm-passwords.toml", config_dir);
        std::fs::write(&password_file, toml::to_string_pretty(self)?)?;
        info!("Passwords saved to: {}", password_file);
        Ok(())
    }

    fn add_vm(&mut self, vm_name: &str, password: &str) {
        self.vms.insert(vm_name.to_string(), password.to_string());
    }
}

#[derive(Parser)]
#[command(name = "vm-provisioner")]
#[command(about = "Lightweight VM isolation system with seamless windowing", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Create {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long, action = clap::ArgAction::Append)]
        system: Vec<String>,
        #[arg(long, action = clap::ArgAction::Append)]
        flatpak: Vec<String>,
        #[arg(short = 'y', long)]
        yes: bool,
        #[arg(short, long)]
        config: Option<String>,
        #[arg(long, default_value = "2048")]
        memory: u64,
        #[arg(long, default_value = "2")]
        vcpus: u32,
        #[arg(long, default_value = "20")]
        disk: u64,
        #[arg(long)]
        headless: bool,
        #[arg(long, action = clap::ArgAction::Append)]
        pci: Vec<String>,
        #[arg(long)]
        pci_hotplug: bool,
        /// Enable web-based streaming on this port (Selkies-GStreamer WebRTC)
        #[arg(long)]
        web_port: Option<u16>,
        /// USB device passthrough (format: "vendor:product" e.g. "046d:c52b")
        #[arg(long, action = clap::ArgAction::Append)]
        usb: Vec<String>,
        /// Hot-attach USB devices only while VM runs (restore to host on stop)
        #[arg(long)]
        usb_hotplug: bool,
        /// Share host folder with VM (format: "/host/path:/guest/mount/path")
        #[arg(long, action = clap::ArgAction::Append)]
        share: Vec<String>,
        /// Mount shared folders as read-only
        #[arg(long)]
        share_readonly: bool,
        /// Use bridged networking (VM gets LAN IP). Requires bridge interface to exist.
        #[arg(long)]
        network_bridge: Option<String>,
        /// Grant flatpak apps access to all devices (webcams, audio, etc.)
        #[arg(long)]
        grant_device_access: bool,
        /// Disable networking entirely (uses vsock for display forwarding)
        #[arg(long)]
        no_network: bool,
    },
    Start {
        name: String,
    },
    Stop {
        name: String,
    },
    List,
    Passwords,
    Destroy {
        name: String,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    Console {
        name: String,
    },
    GenerateShortcuts {
        name: String,
    },
    Launch {
        name: String,
        app: String,
    },
    Apps {
        name: String,
    },
    /// Attach a USB device to a running VM
    UsbAttach {
        /// VM name
        name: String,
        /// USB device in vendor:product format (e.g., "046d:c52b")
        device: String,
    },
    /// Detach a USB device from a running VM
    UsbDetach {
        /// VM name
        name: String,
        /// USB device in vendor:product format (e.g., "046d:c52b")
        device: String,
    },
}

fn get_display_bridge(config: &AppVMConfig) -> Result<Box<dyn DisplayBridge>> {
    // Xpra is the only supported display protocol
    Ok(Box::new(XpraManager::new(config)?))
}

/// Options for VM creation
struct CreateOptions {
    name: Option<String>,
    system_packages: Vec<String>,
    flatpak_packages: Vec<String>,
    skip_confirm: bool,
    config_path: Option<String>,
    memory: u64,
    vcpus: u32,
    disk: u64,
    headless: bool,
    pci_addresses: Vec<String>,
    pci_hotplug: bool,
    web_port: Option<u16>,
    usb_addresses: Vec<String>,
    usb_hotplug: bool,
    share_paths: Vec<String>,
    share_readonly: bool,
    network_bridge: Option<String>,
    grant_device_access: bool,
    no_network: bool,
}

/// Validate create options for mutual exclusivity and prerequisites
fn validate_create_options(opts: &CreateOptions) -> Result<()> {
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
fn build_config(opts: CreateOptions) -> Result<AppVMConfig> {
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
fn display_config_summary(config: &AppVMConfig) {
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
        config::NetworkMode::None => println!("   Network: Disabled (vsock for display)"),
        config::NetworkMode::Nat => println!("   Network: NAT (192.168.122.x)"),
        config::NetworkMode::Bridge(br) => println!("   Network: Bridged ({})", br),
    }
}

/// Save VM config and password to disk
fn save_config_and_passwords(config: &AppVMConfig) -> Result<String> {
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
fn handle_post_provisioning(config: &mut AppVMConfig, config_file: &str) -> Result<()> {
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
fn display_success_message(config: &AppVMConfig) {
    println!("\n✅ VM created successfully!");
    println!("   VM Name: {}", config.name);
    println!("   Username: user");
    println!("   Password: {}", config.user_password);
    if config.enable_vsock {
        println!("   Vsock CID: {}", config.vsock_cid.unwrap_or(0));
    }
    println!("   Start with: vm-provisioner start {}", config.name);
}

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            use std::io::Write;
            let level_icon = match record.level() {
                log::Level::Error => "❌",
                log::Level::Warn => "⚠️ ",
                log::Level::Info => "ℹ️ ",
                log::Level::Debug => "🔍",
                log::Level::Trace => "📋",
            };
            writeln!(buf, "{} {}", level_icon, record.args())
        })
        .init();
}

fn main() -> Result<()> {
    init_logger();

    let cli = Cli::parse();
    match cli.command {
        Commands::Create {
            name,
            system,
            flatpak,
            yes,
            config,
            memory,
            vcpus,
            disk,
            headless,
            pci,
            pci_hotplug,
            web_port,
            usb,
            usb_hotplug,
            share,
            share_readonly,
            network_bridge,
            grant_device_access,
            no_network,
        } => {
            create_vm(
                name,
                system,
                flatpak,
                yes,
                config,
                memory,
                vcpus,
                disk,
                headless,
                pci,
                pci_hotplug,
                web_port,
                usb,
                usb_hotplug,
                share,
                share_readonly,
                network_bridge,
                grant_device_access,
                no_network,
            )?;
        }
        Commands::Start { name } => start_vm(name)?,
        Commands::Stop { name } => stop_vm(name)?,
        Commands::List => list_vms()?,
        Commands::Passwords => show_passwords()?,
        Commands::Destroy { name, yes } => destroy_vm(name, yes)?,
        Commands::Console { name } => connect_console(name)?,
        Commands::GenerateShortcuts { name } => generate_shortcuts(name)?,
        Commands::Launch { name, app } => launch_app(name, app)?,
        Commands::Apps { name } => list_apps(name)?,
        Commands::UsbAttach { name, device } => usb_attach(name, device)?,
        Commands::UsbDetach { name, device } => usb_detach(name, device)?,
    }
    Ok(())
}

fn create_vm(
    name: Option<String>,
    system_packages: Vec<String>,
    flatpak_packages: Vec<String>,
    skip_confirm: bool,
    config_path: Option<String>,
    memory: u64,
    vcpus: u32,
    disk: u64,
    headless: bool,
    pci_addresses: Vec<String>,
    pci_hotplug: bool,
    web_port: Option<u16>,
    usb_addresses: Vec<String>,
    usb_hotplug: bool,
    share_paths: Vec<String>,
    share_readonly: bool,
    network_bridge: Option<String>,
    grant_device_access: bool,
    no_network: bool,
) -> Result<()> {
    info!("VM Provisioner - Dynamic Package Installer");
    println!("==============================================");

    let opts = CreateOptions {
        name,
        system_packages,
        flatpak_packages,
        skip_confirm,
        config_path,
        memory,
        vcpus,
        disk,
        headless,
        pci_addresses,
        pci_hotplug,
        web_port,
        usb_addresses,
        usb_hotplug,
        share_paths,
        share_readonly,
        network_bridge,
        grant_device_access,
        no_network,
    };

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

fn start_vm(name: String) -> Result<()> {
    info!("Starting VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    AppVMProvisioner::new(config.clone()).start_vm()?;

    if config.headless {
        println!("\n💡 Headless VM - connect via console: virsh console {}", name);
    } else {
        println!("\n🪟 Seamless window integration enabled via Xpra");
        println!(
            "   Use `vm-provisioner generate-shortcuts {}` to create .desktop files.",
            name
        );
        if let Some(port) = config.web_port {
            println!(
                "   Or access via browser: http://<vm-ip>:{}/ (Selkies WebRTC)",
                port
            );
        }
    }
    Ok(())
}

fn stop_vm(name: String) -> Result<()> {
    info!("Stopping VM: {}", name);
    let config = AppVMConfig::load(&name)?;
    AppVMProvisioner::new(config).stop_vm()?;
    info!("VM stopped");
    Ok(())
}

fn list_vms() -> Result<()> {
    println!("📋 Available VMs:");
    let config_dir = AppVMConfig::config_dir()?;
    if !Path::new(&config_dir).exists() {
        println!("No VMs configured yet.");
        return Ok(());
    }
    for entry in std::fs::read_dir(&config_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            // Skip the password file
            if path.file_stem().and_then(|s| s.to_str()) == Some("vm-passwords") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str::<AppVMConfig>(&content) {
                    println!("  {} [{}]", config.name, get_vm_status(&config.name));
                }
            }
        }
    }
    Ok(())
}

fn destroy_vm(name: String, skip_confirm: bool) -> Result<()> {
    if !skip_confirm
        && !Confirm::new()
            .with_prompt(format!(
                "Permanently delete VM '{}' and all its data?",
                name
            ))
            .default(false)
            .interact()?
    {
        warn!("Destruction cancelled");
        return Ok(());
    }

    let config_file = AppVMConfig::config_path(&name)?;
    let config = AppVMConfig::load(&name)?;

    let bridge = get_display_bridge(&config)?;
    bridge.remove_desktop_files()?;

    // Clean up SSH known_hosts entry for this VM
    let ip_output = std::process::Command::new("virsh")
        .args(["-c", "qemu:///system", "domifaddr", &name])
        .output();

    if let Ok(output) = ip_output {
        let ip_str = String::from_utf8_lossy(&output.stdout);
        if let Some(ip_line) = ip_str.lines().find(|l| l.contains("ipv4")) {
            if let Some(ip) = ip_line.split_whitespace().nth(3) {
                let ip = ip.trim_end_matches('/').split('/').next().unwrap_or("");
                if !ip.is_empty() {
                    debug!("Cleaning up SSH key for {}", ip);
                    let _ = std::process::Command::new("ssh-keygen")
                        .args(["-R", ip])
                        .output();
                }
            }
        }
    }

    AppVMProvisioner::new(config).destroy_vm()?;
    std::fs::remove_file(&config_file)?;
    info!("VM destroyed");
    Ok(())
}

fn connect_console(name: String) -> Result<()> {
    info!("Connecting to VM console: {}", name);
    std::process::Command::new("virsh")
        .args(["-c", "qemu:///system", "console", &name])
        .status()?;
    Ok(())
}

fn get_vm_status(name: &str) -> String {
    match std::process::Command::new("virsh")
        .args(["-c", "qemu:///system", "domstate", name])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "not created".to_string(),
    }
}

fn parse_share_path(path: &str, readonly: bool) -> Result<SharedFolder> {
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

fn show_passwords() -> Result<()> {
    let config_dir = AppVMConfig::config_dir()?;
    let passwords = VMPasswords::load_or_create(&config_dir)?;
    if passwords.vms.is_empty() {
        println!("ℹ️  No VM passwords stored yet");
        return Ok(());
    }
    println!("🔑 VM Login Credentials:");
    for (vm_name, password) in &passwords.vms {
        println!("   {} | user:{}", vm_name, password);
    }
    Ok(())
}

fn generate_shortcuts(name: String) -> Result<()> {
    info!("Generating application shortcuts for VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    if get_vm_status(&name) != "running" {
        error!(
            "VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    debug!("Waiting for VM to be fully ready...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let bridge = get_display_bridge(&config)?;
    bridge.generate_desktop_files()?;

    println!("\n✅ Application shortcuts created!");
    Ok(())
}

fn launch_app(name: String, app: String) -> Result<()> {
    info!("Launching application in VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    if get_vm_status(&name) != "running" {
        error!(
            "VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    let bridge = get_display_bridge(&config)?;
    bridge.launch_app(&app)?;
    Ok(())
}

fn list_apps(name: String) -> Result<()> {
    println!("📱 Applications available in VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    let bridge = get_display_bridge(&config)?;
    let apps = bridge.list_applications();

    if apps.is_empty() {
        println!("   No applications found.");
    } else {
        for app in apps {
            println!("   - {}", app);
        }
    }
    Ok(())
}

fn usb_attach(name: String, device: String) -> Result<()> {
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

fn usb_detach(name: String, device: String) -> Result<()> {
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
