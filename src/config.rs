use std::fs;
use std::path::Path;
use std::process::Command;
use serde::{Deserialize, Serialize};
use dialoguer::{Input, Confirm, Select};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppVMConfig {
    // Core VM settings
    pub name: String,
    pub memory_mb: u64,
    pub vcpus: u32,
    pub disk_size_gb: u64,
    pub vm_dir: String,
    
    // Application settings
    pub app_type: AppType,
    pub app_packages: Vec<String>,
    pub app_command: String,
    pub app_env_vars: Vec<(String, String)>,
    
    // Graphics and windowing
    pub graphics_backend: GraphicsBackend,
    pub enable_clipboard: bool,
    pub enable_audio: bool,
    pub enable_usb_passthrough: bool,
    
    // Security settings
    pub network_mode: NetworkMode,
    pub firewall_rules: Vec<String>,
    pub vpn_config: Option<VpnConfig>,
    
    // System packages (base system tools)
    pub system_packages: Vec<String>,
    pub user_password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AppType {
    Browser { profile_path: Option<String> },
    Office,
    Development { languages: Vec<String> },
    Media,
    Custom,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum GraphicsBackend {
    VirtioGpu,      // Hardware accelerated
    QxlSpice,       // SPICE protocol
    VncOnly,        // Fallback
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NetworkMode {
    Nat,
    None,
    Bridge(String),
    VpnOnly,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VpnConfig {
    pub provider: String,
    pub config_path: String,
    pub credentials_path: Option<String>,
}

impl AppVMConfig {
    pub fn librewolf_template() -> Self {
        Self {
            name: "librewolf-vm".to_string(),
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 20,
            vm_dir: "/var/lib/libvirt/images".to_string(),
            
            app_type: AppType::Browser { 
                profile_path: Some("/home/user/.librewolf".to_string()) 
            },
            app_packages: vec![
                // LibreWolf specific repository will be added during provisioning
                "librewolf".to_string(),
            ],
            app_command: "/usr/bin/librewolf".to_string(),
            app_env_vars: vec![
                ("MOZ_ENABLE_WAYLAND".to_string(), "1".to_string()),
                ("WAYLAND_DISPLAY".to_string(), "wayland-0".to_string()),
            ],
            
            graphics_backend: GraphicsBackend::VirtioGpu,
            enable_clipboard: true,
            enable_audio: true,
            enable_usb_passthrough: false,
            
            network_mode: NetworkMode::Nat,
            firewall_rules: vec![
                // Allow DNS
                "OUTPUT -p udp --dport 53 -j ACCEPT".to_string(),
                "OUTPUT -p tcp --dport 53 -j ACCEPT".to_string(),
                // Allow HTTP/HTTPS
                "OUTPUT -p tcp --dport 80 -j ACCEPT".to_string(),
                "OUTPUT -p tcp --dport 443 -j ACCEPT".to_string(),
            ],
            vpn_config: None,
            
            system_packages: vec![
                // Wayland compositor and requirements
                "cage".to_string(),
                "wayland".to_string(),
                "wayland-protocols".to_string(),
                "mesa-dri-drivers".to_string(),
                "mesa-vulkan-drivers".to_string(),
                "xorg-x11-server-Xwayland".to_string(),
                
                // Audio support
                "pipewire".to_string(),
                "pipewire-pulseaudio".to_string(),
                "wireplumber".to_string(),
                
                // Clipboard support
                "wl-clipboard".to_string(),
                
                // Fonts and theming
                "fontconfig".to_string(),
                "liberation-fonts".to_string(),
                "google-noto-fonts".to_string(),
                "adwaita-icon-theme".to_string(),
                "adwaita-gtk3-theme".to_string(),
                
                // Basic utilities
                "qemu-guest-agent".to_string(),
                "spice-vdagent".to_string(),
                "curl".to_string(),
                "wget".to_string(),
            ],
            user_password: Self::generate_password(),
        }
    }
    
    pub fn office_template() -> Self {
        let mut config = Self::librewolf_template();
        config.name = "office-vm".to_string();
        config.app_type = AppType::Office;
        config.app_packages = vec![
            "libreoffice".to_string(),
            "libreoffice-gtk3".to_string(),
        ];
        config.app_command = "/usr/bin/libreoffice".to_string();
        config.memory_mb = 6144; // More RAM for office apps
        config
    }
    
    pub fn development_template() -> Self {
        let mut config = Self::librewolf_template();
        config.name = "dev-vm".to_string();
        config.app_type = AppType::Development { 
            languages: vec!["rust".to_string(), "python".to_string()] 
        };
        config.app_packages = vec![
            "neovim".to_string(),
            "git".to_string(),
            "gcc".to_string(),
            "make".to_string(),
            "rust".to_string(),
            "cargo".to_string(),
            "python3".to_string(),
            "python3-pip".to_string(),
        ];
        config.app_command = "/usr/bin/bash".to_string(); // Terminal by default
        config.memory_mb = 8192;
        config.disk_size_gb = 40;
        config
    }
    
    fn generate_password() -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let mut hasher = DefaultHasher::new();
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .hash(&mut hasher);
        format!("vm-{:x}", hasher.finish())
            .chars()
            .take(12)
            .collect()
    }
    
    pub async fn interactive_config() -> Result<Self, Box<dyn std::error::Error>> {
        println!("🔧 Application VM Configuration");
        println!("================================");
        
        // Select VM template
        let templates = vec![
            "LibreWolf Browser VM",
            "Office Suite VM",
            "Development VM",
            "Custom VM",
        ];
        
        let template_idx = Select::new()
            .with_prompt("Select VM template")
            .items(&templates)
            .default(0)
            .interact()?;
        
        let mut config = match template_idx {
            0 => Self::librewolf_template(),
            1 => Self::office_template(),
            2 => Self::development_template(),
            _ => Self::librewolf_template(), // Start with browser as base
        };
        
        // Allow customization
        config.name = Input::new()
            .with_prompt("VM Name")
            .default(config.name)
            .interact_text()?;
        
        config.memory_mb = Input::new()
            .with_prompt("Memory (MB)")
            .default(config.memory_mb)
            .interact_text()?;
        
        config.vcpus = Input::new()
            .with_prompt("Virtual CPUs")
            .default(config.vcpus)
            .interact_text()?;
        
        config.disk_size_gb = Input::new()
            .with_prompt("Disk Size (GB)")
            .default(config.disk_size_gb)
            .interact_text()?;
        
        // Graphics backend selection
        let graphics_options = vec![
            "VirtIO-GPU (Recommended - Hardware acceleration)",
            "QXL/SPICE (Good compatibility)",
            "VNC only (Fallback)",
        ];
        
        let graphics_idx = Select::new()
            .with_prompt("Graphics backend")
            .items(&graphics_options)
            .default(0)
            .interact()?;
        
        config.graphics_backend = match graphics_idx {
            0 => GraphicsBackend::VirtioGpu,
            1 => GraphicsBackend::QxlSpice,
            _ => GraphicsBackend::VncOnly,
        };
        
        config.enable_clipboard = Confirm::new()
            .with_prompt("Enable clipboard sharing?")
            .default(true)
            .interact()?;
        
        config.enable_audio = Confirm::new()
            .with_prompt("Enable audio?")
            .default(true)
            .interact()?;
        
        // Network configuration
        let network_options = vec![
            "NAT (Internet access)",
            "None (Isolated)",
            "VPN Only",
        ];
        
        let network_idx = Select::new()
            .with_prompt("Network mode")
            .items(&network_options)
            .default(0)
            .interact()?;
        
        config.network_mode = match network_idx {
            0 => NetworkMode::Nat,
            1 => NetworkMode::None,
            _ => NetworkMode::VpnOnly,
        };
        
        // VPN configuration if needed
        if matches!(config.network_mode, NetworkMode::VpnOnly) {
            let vpn_config_path: String = Input::new()
                .with_prompt("VPN config file path")
                .interact_text()?;
            
            config.vpn_config = Some(VpnConfig {
                provider: Input::new()
                    .with_prompt("VPN provider")
                    .default("mullvad".to_string())
                    .interact_text()?,
                config_path: vpn_config_path,
                credentials_path: None,
            });
        }
        
        Ok(config)
    }
}
