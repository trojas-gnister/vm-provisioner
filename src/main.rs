mod config;
mod provisioner;
mod window_proxy;

use std::path::Path;
use std::collections::HashMap;
use clap::{Parser, Subcommand};
use dialoguer::{Select, Confirm};
use tokio;
use serde::{Serialize, Deserialize};

use config::AppVMConfig;
use provisioner::AppVMProvisioner;
use window_proxy::VMIntegrationHost;

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
        /// Use a specific template
        #[arg(short, long)]
        template: Option<String>,
        
        /// Skip interactive configuration
        #[arg(short = 'y', long)]
        yes: bool,
        
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
    },
    
    /// Start an existing VM
    Start {
        /// VM name
        name: String,
        
        /// Enable seamless window mode
        #[arg(short, long, default_value = "true")]
        seamless: bool,
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
    
    /// Manage templates
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },
}

#[derive(Subcommand)]
enum TemplateAction {
    /// List available templates
    List,
    
    /// Create custom template
    Create {
        /// Template name
        name: String,
    },
    
    /// Export template
    Export {
        /// Template name
        name: String,
        
        /// Output file
        #[arg(short, long)]
        output: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Create { template, yes, config } => {
            create_vm(template, yes, config).await?;
        }
        
        Commands::Start { name, seamless } => {
            start_vm(name, seamless).await?;
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
        
        Commands::Template { action } => {
            handle_template(action).await?;
        }
    }
    
    Ok(())
}

async fn create_vm(
    template: Option<String>, 
    skip_confirm: bool, 
    config_path: Option<String>
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 VM Provisioner - Application VM Creator");
    println!("==========================================");
    
    let config = if let Some(path) = config_path {
        // Load from file
        let content = std::fs::read_to_string(path)?;
        toml::from_str::<AppVMConfig>(&content)?
    } else if let Some(template_name) = template {
        // Use predefined template
        match template_name.as_str() {
            "librewolf" | "browser" => AppVMConfig::librewolf_template(),
            "office" => AppVMConfig::office_template(),
            "dev" | "development" => AppVMConfig::development_template(),
            _ => {
                eprintln!("Unknown template: {}", template_name);
                eprintln!("Available templates: librewolf, office, dev");
                std::process::exit(1);
            }
        }
    } else {
        // Interactive mode
        AppVMConfig::interactive_config().await?
    };
    
    // Display configuration
    println!("\n📋 VM Configuration:");
    println!("   Name: {}", config.name);
    println!("   Type: {:?}", config.app_type);
    println!("   Memory: {} MB", config.memory_mb);
    println!("   vCPUs: {}", config.vcpus);
    println!("   Disk: {} GB", config.disk_size_gb);
    println!("   Graphics: {:?}", config.graphics_backend);
    println!("   Network: {:?}", config.network_mode);
    println!("   Clipboard: {}", if config.enable_clipboard { "✓" } else { "✗" });
    println!("   Audio: {}", if config.enable_audio { "✓" } else { "✗" });
    
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

async fn start_vm(name: String, seamless: bool) -> Result<(), Box<dyn std::error::Error>> {
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
    
    // Start window integration if seamless mode
    if seamless && matches!(config.graphics_backend, 
                           config::GraphicsBackend::VirtioGpu) {
        println!("🪟 Starting seamless window integration...");
        
        // Launch window proxy in background
        let vm_name_clone = name.clone();
        std::thread::spawn(move || {
            let mut integration = VMIntegrationHost::new(vm_name_clone);
            if let Err(e) = integration.start() {
                eprintln!("Window integration error: {}", e);
            }
        });
        
        println!("✅ Seamless mode active");
        println!("   Application windows will appear as native windows");
        
        if config.enable_clipboard {
            println!("   Clipboard sharing enabled");
        }
    } else if seamless {
        println!("ℹ️  Seamless mode not available (requires VirtIO-GPU)");
        println!("   Connect using: virt-viewer {}", name);
    }
    
    // Display login credentials
    println!("\n🔑 VM Login Credentials:");
    println!("   Username: user");
    println!("   Password: {}", config.user_password);
    println!("   Console: sudo virsh console {}", name);
    
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
                println!("    Type: {:?}", config.app_type);
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
    
    // Load configuration
    let config_file = format!("{}/.config/vm-provisioner/{}.toml", 
                             std::env::var("HOME")?, name);
    
    if Path::new(&config_file).exists() {
        let content = std::fs::read_to_string(&config_file)?;
        let config = toml::from_str::<AppVMConfig>(&content)?;
        
        let provisioner = AppVMProvisioner::new(config);
        provisioner.destroy_vm()?;
        
        // Remove configuration file
        std::fs::remove_file(&config_file)?;
    }
    
    println!("✅ VM destroyed");
    
    Ok(())
}

fn connect_console(name: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("🖥️  Connecting to VM console: {}", name);
    
    std::process::Command::new("virsh")
        .args(&["console", &name])
        .status()?;
    
    Ok(())
}

async fn handle_template(action: TemplateAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        TemplateAction::List => {
            println!("📋 Available Templates:");
            println!("======================");
            println!("  librewolf  - LibreWolf browser in isolated VM");
            println!("  office     - LibreOffice suite");
            println!("  dev        - Development environment");
            println!("");
            println!("Use: vm-provisioner create --template <name>");
        }
        
        TemplateAction::Create { name } => {
            println!("Creating custom template: {}", name);
            let config = AppVMConfig::interactive_config().await?;
            
            // Save as template
            let template_dir = format!("{}/.config/vm-provisioner/templates", 
                                      std::env::var("HOME")?);
            std::fs::create_dir_all(&template_dir)?;
            
            let template_file = format!("{}/{}.toml", template_dir, name);
            std::fs::write(&template_file, toml::to_string_pretty(&config)?)?;
            
            println!("✅ Template saved: {}", template_file);
        }
        
        TemplateAction::Export { name, output } => {
            let template_file = format!("{}/.config/vm-provisioner/templates/{}.toml", 
                                       std::env::var("HOME")?, name);
            
            if !Path::new(&template_file).exists() {
                eprintln!("❌ Template not found: {}", name);
                std::process::exit(1);
            }
            
            std::fs::copy(&template_file, &output)?;
            println!("✅ Template exported to: {}", output);
        }
    }
    
    Ok(())
}

fn get_vm_status(name: &str) -> String {
    match std::process::Command::new("virsh")
        .args(&["domstate", name])
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
