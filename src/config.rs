use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PciDevice {
    pub address: String,                  // "0000:01:00.0"
    pub vendor_id: String,                // "10de"
    pub device_id: String,                // "1c03"
    pub description: String,              // "NVIDIA GeForce GTX 1050"
    pub original_driver: Option<String>,  // "nvidia" - for restoration
    pub iommu_group: Option<u32>,         // IOMMU group number
}

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
    pub headless: bool,  // CLI-only mode, no GUI

    // PCI passthrough
    pub pci_devices: Vec<PciDevice>,
    pub pci_hotplug: bool,  // true = hot-attach/detach, false = permanent XML
    
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
        headless: bool,
        pci_devices: Vec<PciDevice>,
        pci_hotplug: bool,
    ) -> Self {
        // Default system packages - different for headless vs GUI
        let mut default_system_packages = if headless {
            // Headless mode: minimal packages, no GUI/X11
            vec![
                "git".to_string(),               // Version control
            ]
        } else {
            // GUI mode: full desktop environment with Xpra support
            vec![
                "i3".to_string(),
                "i3status".to_string(),
                "i3lock".to_string(),
                "dmenu".to_string(),
                "rofi".to_string(),                  // Better application launcher with Flatpak support
                "xorg-x11-server-Xorg".to_string(),
                "xorg-x11-xinit".to_string(),
                "pipewire".to_string(),              // Audio system
                "spice-vdagent".to_string(),         // SPICE agent for clipboard/resolution (useful for SPICE debugging)
                "kitty".to_string(),                 // Default terminal emulator
                "git".to_string(),                   // Version control
                "xpra".to_string(),                  // Xpra for seamless window integration
                "openssh-server".to_string(),        // SSH server for Xpra connections
            ]
        };

        // Add user-specified system packages
        default_system_packages.extend(system_packages);

        // Auto-launch commands (empty by default - user can manually configure in i3)
        let auto_launch_apps = Vec::new();
        
        Self {
            name,
            memory_mb,
            vcpus,
            disk_size_gb,
            vm_dir: "/var/lib/libvirt/images".to_string(),

            system_packages: default_system_packages,
            flatpak_packages: if headless { vec![] } else { flatpak_packages.clone() },
            auto_launch_apps,

            graphics_backend: if headless { GraphicsBackend::VncOnly } else { GraphicsBackend::VirtioGpu },
            enable_clipboard: !headless,
            enable_audio: !headless,
            enable_usb_passthrough: false,
            enable_auto_login: !headless,
            headless,

            pci_devices,
            pci_hotplug,

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
