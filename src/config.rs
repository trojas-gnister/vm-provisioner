use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppVMConfig {
    // Core VM settings
    pub name: String,
    pub memory_mb: u64,
    pub vcpus: u32,
    pub disk_size_gb: u64,
    pub vm_dir: String,
    
    // Package installation
    pub system_packages: Vec<String>,
    pub flatpak_packages: Vec<String>,
    pub auto_launch_apps: Vec<String>,  // Commands to run on startup
    
    // Graphics and windowing
    pub graphics_backend: GraphicsBackend,
    pub enable_clipboard: bool,
    pub enable_audio: bool,
    pub enable_usb_passthrough: bool,
    pub enable_auto_login: bool,
    
    // Security settings
    pub network_mode: NetworkMode,
    pub firewall_rules: Vec<String>,
    pub vpn_config: Option<VpnConfig>,
    
    // Authentication
    pub user_password: String,
}

// Remove AppType enum as we're now using dynamic packages

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
    pub fn new(
        name: String,
        memory_mb: u64,
        vcpus: u32,
        disk_size_gb: u64,
        system_packages: Vec<String>,
        flatpak_packages: Vec<String>,
    ) -> Self {
        // Default system packages including kitty terminal
        // Build dependencies are now installed in post-install script
        let mut default_system_packages = vec![
            "i3".to_string(),
            "i3status".to_string(),
            "i3lock".to_string(),
            "dmenu".to_string(),
            "rofi".to_string(),                  // Better application launcher with Flatpak support
            "xorg-x11-server-Xorg".to_string(),
            "xorg-x11-xinit".to_string(),
            "xset".to_string(),                  // X11 settings utility (CRITICAL for startup)
            "xrandr".to_string(),                // X11 resolution control
            "wmctrl".to_string(),                // Window management for guest agent
            "xwininfo".to_string(),              // Window information for guest agent
            "pipewire".to_string(),              // Audio system
            "wl-clipboard".to_string(),          // Clipboard utilities
            "spice-vdagent".to_string(),         // SPICE agent for clipboard/resolution
            "kitty".to_string(),                 // Default terminal emulator
            "git".to_string(),                   // Version control (needed for spice-autorandr)
        ];
        
        // Add user-specified system packages
        default_system_packages.extend(system_packages);
        
        // Generate auto-launch commands for installed packages
        let mut auto_launch_apps = Vec::new();
        
        // Auto-launch flatpak packages
        for pkg in &flatpak_packages {
            auto_launch_apps.push(format!("flatpak run {}", pkg));
        }
        
        Self {
            name,
            memory_mb,
            vcpus,
            disk_size_gb,
            vm_dir: "/var/lib/libvirt/images".to_string(),
            
            system_packages: default_system_packages,
            flatpak_packages: flatpak_packages.clone(),
            auto_launch_apps,
            
            graphics_backend: GraphicsBackend::VirtioGpu,
            enable_clipboard: true,
            enable_audio: true,
            enable_usb_passthrough: false,
            enable_auto_login: true,
            
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
            
            user_password: generate_password(),
        }
    }
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
