mod config;
mod display_bridge;
mod provisioner;
mod xpra_manager;

use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio;

use config::{AppVMConfig, SharedFolder};
use display_bridge::DisplayBridge;
use provisioner::AppVMProvisioner;
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

    fn load_or_create(config_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let password_file = format!("{}/vm-passwords.toml", config_dir);

        if Path::new(&password_file).exists() {
            let content = std::fs::read_to_string(&password_file)?;
            Ok(toml::from_str(&content).unwrap_or_else(|_| Self::new()))
        } else {
            Ok(Self::new())
        }
    }

    fn save(&self, config_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(config_dir)?;
        let password_file = format!("{}/vm-passwords.toml", config_dir);
        std::fs::write(&password_file, toml::to_string_pretty(self)?)?;
        println!("💾 Passwords saved to: {}", password_file);
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
        /// Enable Xpra HTML5 web access on this port (access from any browser)
        #[arg(long)]
        xpra_html_port: Option<u16>,
        /// Allow Xpra HTML5 access from LAN (binds to 0.0.0.0 instead of localhost)
        #[arg(long)]
        xpra_html_lan: bool,
        /// Enable microphone input from host to VM
        #[arg(long)]
        microphone: bool,
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
}

fn get_display_bridge(
    config: &AppVMConfig,
) -> Result<Box<dyn DisplayBridge>, Box<dyn std::error::Error>> {
    // Xpra is the only supported display protocol
    Ok(Box::new(XpraManager::new(config)?))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
            xpra_html_port,
            xpra_html_lan,
            microphone,
            usb,
            usb_hotplug,
            share,
            share_readonly,
            network_bridge,
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
                xpra_html_port,
                xpra_html_lan,
                microphone,
                usb,
                usb_hotplug,
                share,
                share_readonly,
                network_bridge,
            )
            .await?;
        }
        Commands::Start { name } => start_vm(name).await?,
        Commands::Stop { name } => stop_vm(name).await?,
        Commands::List => list_vms()?,
        Commands::Passwords => show_passwords()?,
        Commands::Destroy { name, yes } => destroy_vm(name, yes).await?,
        Commands::Console { name } => connect_console(name)?,
        Commands::GenerateShortcuts { name } => generate_shortcuts(name).await?,
        Commands::Launch { name, app } => launch_app(name, app).await?,
        Commands::Apps { name } => list_apps(name).await?,
    }
    Ok(())
}

async fn create_vm(
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
    xpra_html_port: Option<u16>,
    xpra_html_lan: bool,
    enable_microphone: bool,
    usb_addresses: Vec<String>,
    usb_hotplug: bool,
    share_paths: Vec<String>,
    share_readonly: bool,
    network_bridge: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 VM Provisioner - Dynamic Package Installer");
    println!("==============================================");

    // Validate bridge interface exists if specified
    if let Some(ref bridge) = network_bridge {
        let bridge_path = format!("/sys/class/net/{}", bridge);
        if !Path::new(&bridge_path).exists() {
            eprintln!("\n❌ Error: Bridge interface '{}' not found.", bridge);
            eprintln!("\nTo create a bridge (one-time setup):");
            eprintln!("  1. Find your network interface: nmcli device status");
            eprintln!("  2. Create bridge:");
            eprintln!("     sudo nmcli connection add type bridge ifname {} con-name {}", bridge, bridge);
            eprintln!("     sudo nmcli connection add type bridge-slave ifname <your-interface> master {}", bridge);
            eprintln!("     sudo nmcli connection up {}", bridge);
            return Err(format!("Bridge interface '{}' does not exist", bridge).into());
        }
        println!("\n🌐 Using bridged networking via '{}'", bridge);
    }

    let config = if let Some(path) = config_path {
        toml::from_str::<AppVMConfig>(&std::fs::read_to_string(path)?)?
    } else {
        let vm_name = name.unwrap_or_else(|| {
            if !flatpak_packages.is_empty() {
                format!("{}-vm", flatpak_packages[0].replace('.', "-"))
            } else if !system_packages.is_empty() {
                format!("{}-vm", system_packages[0])
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

        let pci_devices = if !pci_addresses.is_empty() {
            println!("\n🔍 Detecting PCI devices...");
            pci_addresses
                .iter()
                .map(|addr| provisioner::detect_pci_device(addr))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        let usb_devices = if !usb_addresses.is_empty() {
            println!("\n🔍 Detecting USB devices...");
            usb_addresses
                .iter()
                .map(|addr| provisioner::detect_usb_device(addr))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        let shared_folders = if !share_paths.is_empty() {
            println!("\n📁 Configuring shared folders...");
            share_paths
                .iter()
                .map(|path| parse_share_path(path, share_readonly))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        AppVMConfig::new(
            vm_name,
            memory,
            vcpus,
            disk,
            system_packages,
            flatpak_packages,
            headless,
            pci_devices,
            pci_hotplug,
            xpra_html_port,
            xpra_html_lan,
            enable_microphone,
            usb_devices,
            usb_hotplug,
            shared_folders,
            network_bridge,
        )
    };

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
    if let Some(port) = config.xpra_html_port {
        println!("   HTML5 Web Access: http://<vm-ip>:{}/", port);
    }
    println!("   System Packages: {:?}", config.system_packages);
    println!("   Flatpak Packages: {:?}", config.flatpak_packages);
    println!("   Memory: {} MB", config.memory_mb);
    println!("   vCPUs: {}", config.vcpus);
    println!("   Disk: {} GB", config.disk_size_gb);
    if config.enable_microphone {
        println!("   Microphone: enabled");
    }
    if !config.usb_devices.is_empty() {
        println!("   USB Devices: {} device(s){}",
            config.usb_devices.len(),
            if config.usb_hotplug { " (hot-plug)" } else { " (permanent)" }
        );
        for usb in &config.usb_devices {
            println!("     - {} ({}:{})", usb.description, usb.vendor_id, usb.product_id);
        }
    }
    if !config.shared_folders.is_empty() {
        println!("   Shared Folders: {} folder(s)", config.shared_folders.len());
        for folder in &config.shared_folders {
            println!("     - {} -> {} ({})",
                folder.host_path,
                folder.guest_path,
                if folder.readonly { "read-only" } else { "read-write" }
            );
        }
    }

    if !skip_confirm
        && !Confirm::new()
            .with_prompt("Proceed with VM creation?")
            .default(true)
            .interact()?
    {
        println!("❌ VM creation cancelled");
        return Ok(());
    }

    let config_dir = format!("{}/.config/vm-provisioner", std::env::var("HOME")?);
    std::fs::create_dir_all(&config_dir)?;
    let config_file = format!("{}/{}.toml", config_dir, config.name);
    std::fs::write(&config_file, toml::to_string_pretty(&config)?)?;
    println!("💾 Configuration saved to: {}", config_file);

    let mut passwords = VMPasswords::load_or_create(&config_dir)?;
    passwords.add_vm(&config.name, &config.user_password);
    passwords.save(&config_dir)?;

    AppVMProvisioner::new(config.clone()).provision_vm().await?;

    println!("\n✅ VM created successfully!");
    println!("   VM Name: {}", config.name);
    println!("   Username: user");
    println!("   Password: {}", config.user_password);
    println!("   Start with: vm-provisioner start {}", config.name);

    Ok(())
}

async fn start_vm(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("▶️  Starting VM: {}", name);
    let config_file = format!(
        "{}/.config/vm-provisioner/{}.toml",
        std::env::var("HOME")?,
        name
    );
    let config = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(config_file)?)?;

    AppVMProvisioner::new(config.clone()).start_vm()?;

    if config.headless {
        println!(
            "\n💡 Headless VM - connect via console: virsh console {}",
            name
        );
    } else {
        println!("\n🪟 Seamless window integration enabled via Xpra");
        println!(
            "   Use `vm-provisioner generate-shortcuts {}` to create .desktop files.",
            name
        );
        if let Some(port) = config.xpra_html_port {
            println!(
                "   Or access via browser: http://<vm-ip>:{}/",
                port
            );
        }
    }
    Ok(())
}

async fn stop_vm(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("⏹️  Stopping VM: {}", name);
    let config_file = format!(
        "{}/.config/vm-provisioner/{}.toml",
        std::env::var("HOME")?,
        name
    );
    let config = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(config_file)?)?;
    AppVMProvisioner::new(config).stop_vm()?;
    println!("✅ VM stopped");
    Ok(())
}

fn list_vms() -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 Available VMs:");
    let config_dir = format!("{}/.config/vm-provisioner", std::env::var("HOME")?);
    if !Path::new(&config_dir).exists() {
        println!("No VMs configured yet.");
        return Ok(());
    }
    for entry in std::fs::read_dir(&config_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            if let Ok(config) = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(&path)?) {
                println!("  {} [{}]", config.name, get_vm_status(&config.name));
            }
        }
    }
    Ok(())
}

async fn destroy_vm(name: String, skip_confirm: bool) -> Result<(), Box<dyn std::error::Error>> {
    if !skip_confirm
        && !Confirm::new()
            .with_prompt(format!(
                "Permanently delete VM '{}' and all its data?",
                name
            ))
            .default(false)
            .interact()?
    {
        println!("❌ Destruction cancelled");
        return Ok(());
    }

    let config_file = format!(
        "{}/.config/vm-provisioner/{}.toml",
        std::env::var("HOME")?,
        name
    );
    let config = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(&config_file)?)?;

    let bridge = get_display_bridge(&config)?;
    bridge.remove_desktop_files()?;

    AppVMProvisioner::new(config).destroy_vm()?;
    std::fs::remove_file(&config_file)?;
    println!("✅ VM destroyed");
    Ok(())
}

fn connect_console(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("🖥️  Connecting to VM console: {}", name);
    std::process::Command::new("virsh")
        .args(&["-c", "qemu:///system", "console", &name])
        .status()?;
    Ok(())
}

fn get_vm_status(name: &str) -> String {
    match std::process::Command::new("virsh")
        .args(&["-c", "qemu:///system", "domstate", name])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "not created".to_string(),
    }
}

fn parse_share_path(path: &str, readonly: bool) -> Result<SharedFolder, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = path.split(':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid share format '{}'. Expected format: '/host/path:/guest/path'",
            path
        ).into());
    }

    let host_path = parts[0];
    let guest_path = parts[1];

    // Validate host path exists
    if !Path::new(host_path).exists() {
        return Err(format!(
            "Host path '{}' does not exist",
            host_path
        ).into());
    }

    // Generate a tag from guest path (replace / with _ and remove leading _)
    let tag = guest_path
        .trim_start_matches('/')
        .replace('/', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();

    println!("   {} -> {} ({})", host_path, guest_path, if readonly { "read-only" } else { "read-write" });

    Ok(SharedFolder {
        host_path: host_path.to_string(),
        guest_path: guest_path.to_string(),
        tag,
        readonly,
    })
}

fn show_passwords() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = format!("{}/.config/vm-provisioner", std::env::var("HOME")?);
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

async fn generate_shortcuts(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 Generating application shortcuts for VM: {}", name);
    let config_file = format!(
        "{}/.config/vm-provisioner/{}.toml",
        std::env::var("HOME")?,
        name
    );
    let config = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(config_file)?)?;

    if get_vm_status(&name) != "running" {
        eprintln!(
            "❌ VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    println!("⏳ Waiting for VM to be fully ready...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let bridge = get_display_bridge(&config)?;
    bridge.generate_desktop_files()?;

    println!("\n✅ Application shortcuts created!");
    Ok(())
}

async fn launch_app(name: String, app: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Launching application in VM: {}", name);
    let config_file = format!(
        "{}/.config/vm-provisioner/{}.toml",
        std::env::var("HOME")?,
        name
    );
    let config = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(config_file)?)?;

    if get_vm_status(&name) != "running" {
        eprintln!(
            "❌ VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    let bridge = get_display_bridge(&config)?;
    bridge.launch_app(&app)?;
    Ok(())
}

async fn list_apps(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("📱 Applications available in VM: {}", name);
    let config_file = format!(
        "{}/.config/vm-provisioner/{}.toml",
        std::env::var("HOME")?,
        name
    );
    let config = toml::from_str::<AppVMConfig>(&std::fs::read_to_string(config_file)?)?;

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
