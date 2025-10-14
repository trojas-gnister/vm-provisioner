mod config;
mod provisioner;
mod xpra_manager;

use std::path::Path;
use std::collections::HashMap;
use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use tokio;
use serde::{Serialize, Deserialize};

use config::AppVMConfig;
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
        // Ensure directory exists
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
    /// Create a new application VM
    Create {
        /// VM name
        #[arg(short, long)]
        name: Option<String>,

        /// System packages to install (can be used multiple times)
        #[arg(long, action = clap::ArgAction::Append)]
        system: Vec<String>,

        /// Flatpak packages to install (can be used multiple times)
        #[arg(long, action = clap::ArgAction::Append)]
        flatpak: Vec<String>,

        /// Skip interactive configuration
        #[arg(short = 'y', long)]
        yes: bool,

        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,

        /// Memory in MB (default: 2048)
        #[arg(long, default_value = "2048")]
        memory: u64,

        /// Number of CPUs (default: 2)
        #[arg(long, default_value = "2")]
        vcpus: u32,

        /// Disk size in GB (default: 20)
        #[arg(long, default_value = "20")]
        disk: u64,

        /// Headless mode - no GUI/desktop environment (CLI only)
        #[arg(long)]
        headless: bool,

        /// PCI device to passthrough (can be used multiple times, format: 0000:01:00.0)
        #[arg(long, action = clap::ArgAction::Append)]
        pci: Vec<String>,

        /// Enable PCI hot-plug mode (attach on start, detach on stop)
        #[arg(long)]
        pci_hotplug: bool,
    },
    
    /// Start an existing VM
    Start {
        /// VM name
        name: String,
    },
    
    /// Stop a running VM
    Stop {
        /// VM name
        name: String,
    },
    
    /// List all VMs
    List,
    
    /// Show passwords for all VMs
    Passwords,
    
    /// Destroy a VM
    Destroy {
        /// VM name
        name: String,
        
        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },
    
    /// Connect to VM console
    Console {
        /// VM name
        name: String,
    },

    /// Generate .desktop shortcuts for VM applications
    GenerateShortcuts {
        /// VM name
        name: String,
    },

    /// Launch a specific application in a VM via Xpra
    Launch {
        /// VM name
        name: String,

        /// Application command to launch
        app: String,
    },

    /// List applications available in a VM
    Apps {
        /// VM name
        name: String,
    },

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Create { name, system, flatpak, yes, config, memory, vcpus, disk, headless, pci, pci_hotplug } => {
            create_vm(name, system, flatpak, yes, config, memory, vcpus, disk, headless, pci, pci_hotplug).await?;
        }
        
        Commands::Start { name } => {
            start_vm(name).await?;
        }
        
        Commands::Stop { name } => {
            stop_vm(name).await?;
        }
        
        Commands::List => {
            list_vms()?;
        }
        
        Commands::Passwords => {
            show_passwords()?;
        }
        
        Commands::Destroy { name, yes } => {
            destroy_vm(name, yes).await?;
        }
        
        Commands::Console { name } => {
            connect_console(name)?;
        }

        Commands::GenerateShortcuts { name } => {
            generate_shortcuts(name).await?;
        }

        Commands::Launch { name, app } => {
            launch_app(name, app).await?;
        }

        Commands::Apps { name } => {
            list_apps(name)?;
        }

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
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 VM Provisioner - Dynamic Package Installer");
    println!("==============================================");
    
    let config = if let Some(path) = config_path {
        // Load from file
        let content = std::fs::read_to_string(path)?;
        toml::from_str::<AppVMConfig>(&content)?
    } else {
        // Generate VM name if not provided
        let vm_name = if let Some(name) = name {
            name
        } else if !flatpak_packages.is_empty() {
            format!("{}-vm", flatpak_packages[0].replace(".", "-"))
        } else if !system_packages.is_empty() {
            format!("{}-vm", system_packages[0])
        } else {
            format!("app-vm-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
        };
        
        // Detect and validate PCI devices
        let pci_devices = if !pci_addresses.is_empty() {
            println!("\n🔍 Detecting PCI devices...");
            let mut devices = Vec::new();
            for address in &pci_addresses {
                match provisioner::detect_pci_device(address) {
                    Ok(device) => {
                        println!("   ✓ {} - {}", device.address, device.description);
                        if let Some(driver) = &device.original_driver {
                            println!("     Current driver: {}", driver);
                        }
                        if let Some(group) = device.iommu_group {
                            println!("     IOMMU group: {}", group);
                        }
                        devices.push(device);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to detect PCI device {}: {}", address, e);
                        std::process::exit(1);
                    }
                }
            }
            devices
        } else {
            Vec::new()
        };

        // Create config with dynamic packages
        AppVMConfig::new(vm_name, memory, vcpus, disk, system_packages, flatpak_packages, headless, pci_devices, pci_hotplug)
    };
    
    // Display configuration
    println!("\n📋 VM Configuration:");
    println!("   Name: {}", config.name);
    println!("   Mode: {}", if config.headless { "Headless (CLI only)" } else { "GUI" });
    println!("   System Packages: {:?}", config.system_packages);
    println!("   Flatpak Packages: {:?}", config.flatpak_packages);
    println!("   Memory: {} MB", config.memory_mb);
    println!("   vCPUs: {}", config.vcpus);
    println!("   Disk: {} GB", config.disk_size_gb);
    if !config.headless {
        println!("   Graphics: {:?}", config.graphics_backend);
        println!("   Clipboard: {}", if config.enable_clipboard { "✓" } else { "✗" });
        println!("   Audio: {}", if config.enable_audio { "✓" } else { "✗" });
    }
    println!("   Network: {:?}", config.network_mode);

    if !config.pci_devices.is_empty() {
        println!("   PCI Passthrough: {} devices", config.pci_devices.len());
        println!("   PCI Mode: {}", if config.pci_hotplug { "Hot-plug (dynamic)" } else { "Permanent (XML)" });
        for device in &config.pci_devices {
            println!("     - {} ({})", device.address, device.description);
        }
    }
    
    if !skip_confirm {
        let confirm = Confirm::new()
            .with_prompt("Proceed with VM creation?")
            .default(true)
            .interact()?;
            
        if !confirm {
            println!("❌ VM creation cancelled");
            return Ok(());
        }
    }
    
    // Save configuration for future reference
    let config_dir = format!("{}/.config/vm-provisioner", std::env::var("HOME")?);
    std::fs::create_dir_all(&config_dir)?;
    let config_file = format!("{}/{}.toml", config_dir, config.name);
    std::fs::write(&config_file, toml::to_string_pretty(&config)?)?;
    println!("💾 Configuration saved to: {}", config_file);
    
    // Save password to centralized password file
    let mut passwords = VMPasswords::load_or_create(&config_dir)?;
    passwords.add_vm(&config.name, &config.user_password);
    passwords.save(&config_dir)?;
    
    // Create and provision VM
    let provisioner = AppVMProvisioner::new(config.clone());
    provisioner.provision_vm().await?;
    
    println!("\n✅ VM created successfully!");
    println!("   VM Name: {}", config.name);
    println!("   Username: user");
    println!("   Password: {}", config.user_password);
    println!("   Config: {}", config_file);
    println!("   Passwords: {}/.config/vm-provisioner/vm-passwords.toml", std::env::var("HOME")?);
    println!("   Start with: vm-provisioner start {}", config.name);
    
    Ok(())
}

async fn start_vm(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("▶️  Starting VM: {}", name);
    
    // Load VM configuration
    let config_file = format!("{}/.config/vm-provisioner/{}.toml", 
                             std::env::var("HOME")?, name);
    
    if !Path::new(&config_file).exists() {
        eprintln!("❌ VM configuration not found: {}", name);
        eprintln!("   Available VMs:");
        list_vms()?;
        std::process::exit(1);
    }
    
    let content = std::fs::read_to_string(&config_file)?;
    let config = toml::from_str::<AppVMConfig>(&content)?;
    
    // Start the VM
    let provisioner = AppVMProvisioner::new(config.clone());
    provisioner.start_vm()?;

    // For headless VMs, skip Xpra and SPICE viewer
    if config.headless {
        println!("\n🔑 VM Login Credentials:");
        println!("   Username: user");
        println!("   Password: {}", config.user_password);
        println!("   Console: virsh console {}", name);
        println!("\n💡 Headless VM - connect via console");
        return Ok(());
    }

    // Display information about Xpra integration
    println!("\n🪟 Seamless Window Integration via Xpra");
    println!("   Xpra connects via SSH (passwordless)");
    println!("   Use commands:");
    println!("     vm-provisioner generate-shortcuts {}  # Create .desktop files", name);
    println!("     vm-provisioner launch {} <app>       # Launch specific app", name);
    println!("     vm-provisioner apps {}               # List available apps", name);

    if config.enable_clipboard {
        println!("\n📋 Clipboard sharing enabled via Xpra");
    }

    // Display login credentials
    println!("\n🔑 VM Login Credentials:");
    println!("   Username: user");
    println!("   Password: {}", config.user_password);
    println!("   Console: virsh console {}", name);

    Ok(())
}

async fn stop_vm(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("⏹️  Stopping VM: {}", name);
    
    // Load VM configuration
    let config_file = format!("{}/.config/vm-provisioner/{}.toml", 
                             std::env::var("HOME")?, name);
    
    if !Path::new(&config_file).exists() {
        eprintln!("❌ VM configuration not found: {}", name);
        std::process::exit(1);
    }
    
    let content = std::fs::read_to_string(&config_file)?;
    let config = toml::from_str::<AppVMConfig>(&content)?;
    
    let provisioner = AppVMProvisioner::new(config);
    provisioner.stop_vm()?;
    
    println!("✅ VM stopped");
    
    Ok(())
}

fn list_vms() -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 Available VMs:");
    println!("================");
    
    let config_dir = format!("{}/.config/vm-provisioner", std::env::var("HOME")?);
    
    if !Path::new(&config_dir).exists() {
        println!("No VMs configured yet.");
        println!("Create one with: vm-provisioner create");
        return Ok(());
    }
    
    // List all .toml files
    for entry in std::fs::read_dir(&config_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let content = std::fs::read_to_string(&path)?;
            if let Ok(config) = toml::from_str::<AppVMConfig>(&content) {
                // Check VM status
                let status = get_vm_status(&config.name);
                
                println!("  {} [{}]", config.name, status);
                println!("    System Packages: {:?}", config.system_packages);
                println!("    Flatpak Packages: {:?}", config.flatpak_packages);
                println!("    Memory: {} MB", config.memory_mb);
                println!("    Graphics: {:?}", config.graphics_backend);
            }
        }
    }
    
    Ok(())
}

async fn destroy_vm(name: String, skip_confirm: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("🗑️  Preparing to destroy VM: {}", name);

    if !skip_confirm {
        println!("⚠️  This will permanently delete the VM and all its data!");

        let confirm = Confirm::new()
            .with_prompt("Are you sure?")
            .default(false)
            .interact()?;

        if !confirm {
            println!("❌ Destruction cancelled");
            return Ok(());
        }
    }

    // Check both user config and root config locations
    let user_config = format!("{}/.config/vm-provisioner/{}.toml",
                             std::env::var("HOME")?, name);
    let root_config = format!("/root/.config/vm-provisioner/{}.toml", name);

    let config_file = if Path::new(&user_config).exists() {
        user_config
    } else if Path::new(&root_config).exists() {
        root_config
    } else {
        eprintln!("❌ VM configuration not found for: {}", name);
        eprintln!("   Checked: {}", user_config);
        eprintln!("   Checked: {}", root_config);
        eprintln!("   The VM may have been created with sudo.");
        eprintln!("   Try running: sudo ./target/release/vm-provisioner destroy {} --yes", name);
        std::process::exit(1);
    };

    println!("   Using config: {}", config_file);

    let content = std::fs::read_to_string(&config_file)?;
    let config = toml::from_str::<AppVMConfig>(&content)?;

    let provisioner = AppVMProvisioner::new(config.clone());
    provisioner.destroy_vm()?;

    // Remove desktop shortcuts
    if let Ok(xpra_manager) = XpraManager::new(&config) {
        if let Err(e) = xpra_manager.remove_desktop_files() {
            println!("   ⚠️  Warning: Could not remove desktop files: {}", e);
        } else {
            println!("   ✅ Desktop shortcuts removed");
        }
    }

    // Remove configuration file
    if let Err(e) = std::fs::remove_file(&config_file) {
        println!("   ⚠️  Warning: Could not remove config file: {}", e);
        println!("   You may need to run: sudo rm {}", config_file);
    } else {
        println!("   ✅ Config file removed");
    }

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
        _ => "not created".to_string()
    }
}

fn show_passwords() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = format!("{}/.config/vm-provisioner", std::env::var("HOME")?);
    let password_file = format!("{}/vm-passwords.toml", config_dir);
    
    if !Path::new(&password_file).exists() {
        println!("❌ No password file found");
        println!("   Create a VM first to generate passwords");
        return Ok(());
    }
    
    let passwords = VMPasswords::load_or_create(&config_dir)?;
    
    if passwords.vms.is_empty() {
        println!("ℹ️  No VM passwords stored yet");
        return Ok(());
    }
    
    println!("🔑 VM Login Credentials:");
    println!("   File: {}", password_file);
    println!();
    
    for (vm_name, password) in &passwords.vms {
        println!("   {} | user:{}", vm_name, password);
    }
    
    println!("\n💡 Usage:");
    println!("   sudo virsh console <vm-name>");
    println!("   vm-provisioner start <vm-name>  # Shows password");

    Ok(())
}

async fn generate_shortcuts(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 Generating application shortcuts for VM: {}", name);

    // Load VM configuration
    let config_file = format!("{}/.config/vm-provisioner/{}.toml",
                             std::env::var("HOME")?, name);

    if !Path::new(&config_file).exists() {
        eprintln!("❌ VM configuration not found: {}", name);
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&config_file)?;
    let config = toml::from_str::<AppVMConfig>(&content)?;

    // Check if VM is running
    let status = get_vm_status(&name);
    if status != "running" {
        eprintln!("❌ VM is not running (status: {})", status);
        eprintln!("   Start it with: vm-provisioner start {}", name);
        std::process::exit(1);
    }

    // Wait a bit for VM to be fully ready
    println!("⏳ Waiting for VM to be fully ready...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Create Xpra manager and generate shortcuts
    let xpra_manager = XpraManager::new(&config)?;
    xpra_manager.generate_desktop_files()?;

    println!("\n✅ Application shortcuts created!");
    println!("   Location: ~/.local/share/applications/vm-provisioner/");
    println!("   Applications should now appear in your application menu");
    println!("\n💡 Tip: You may need to refresh your application menu or log out/in");

    Ok(())
}

async fn launch_app(name: String, app: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Launching application in VM: {}", name);

    // Load VM configuration
    let config_file = format!("{}/.config/vm-provisioner/{}.toml",
                             std::env::var("HOME")?, name);

    if !Path::new(&config_file).exists() {
        eprintln!("❌ VM configuration not found: {}", name);
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&config_file)?;
    let config = toml::from_str::<AppVMConfig>(&content)?;

    // Check if VM is running
    let status = get_vm_status(&name);
    if status != "running" {
        eprintln!("❌ VM is not running (status: {})", status);
        eprintln!("   Start it with: vm-provisioner start {}", name);
        std::process::exit(1);
    }

    // Create Xpra manager and launch app
    let xpra_manager = XpraManager::new(&config)?;
    xpra_manager.launch_app(&app)?;

    println!("✅ Application launch initiated");

    Ok(())
}

fn list_apps(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("📱 Applications available in VM: {}", name);
    println!("=====================================");

    // Load VM configuration
    let config_file = format!("{}/.config/vm-provisioner/{}.toml",
                             std::env::var("HOME")?, name);

    if !Path::new(&config_file).exists() {
        eprintln!("❌ VM configuration not found: {}", name);
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&config_file)?;
    let config = toml::from_str::<AppVMConfig>(&content)?;

    // Create Xpra manager and list apps
    let xpra_manager = XpraManager::new(&config)
        .unwrap_or_else(|_| {
            eprintln!("⚠️  Could not get VM IP (VM may not be running)");
            eprintln!("   Showing configured packages only");
            panic!();
        });

    let apps = xpra_manager.list_applications();

    if apps.is_empty() {
        println!("   No applications installed");
    } else {
        println!("\n📦 System Packages:");
        for pkg in &config.system_packages {
            println!("   - {}", pkg);
        }

        if !config.flatpak_packages.is_empty() {
            println!("\n📦 Flatpak Packages:");
            for pkg in &config.flatpak_packages {
                println!("   - {}", pkg);
            }
        }

        println!("\n💡 Usage:");
        println!("   vm-provisioner launch {} <app-command>", name);
        println!("   vm-provisioner generate-shortcuts {}  # Create .desktop files", name);
    }

    Ok(())
}
