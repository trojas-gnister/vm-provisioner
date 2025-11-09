use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PciDevice {
    pub address: String,                 // "0000:01:00.0"
    pub vendor_id: String,               // "10de"
    pub device_id: String,               // "1c03"
    pub description: String,             // "NVIDIA GeForce GTX 1050"
    pub original_driver: Option<String>, // "nvidia" - for restoration
    pub iommu_group: Option<u32>,        // IOMMU group number
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, ValueEnum)]
pub enum DisplayProtocol {
    Waypipe,
    X2Go,
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
    pub auto_launch_apps: Vec<String>, // Commands to run on startup

    // Graphics and windowing
    pub graphics_backend: GraphicsBackend,
    pub display_protocol: DisplayProtocol,
    pub enable_clipboard: bool,
    pub enable_audio: bool,
    pub enable_usb_passthrough: bool,
    pub enable_auto_login: bool,
    pub headless: bool, // CLI-only mode, no GUI

    // PCI passthrough
    pub pci_devices: Vec<PciDevice>,
    pub pci_hotplug: bool, // true = hot-attach/detach, false = permanent XML

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
    VirtioGpu, // Hardware accelerated
    QxlSpice,  // SPICE protocol
    VncOnly,   // Fallback
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
        display_protocol: DisplayProtocol,
    ) -> Self {
        // Default system packages - different for headless vs GUI
        let mut default_system_packages = if headless {
            // Headless mode: minimal packages, no GUI/X11
            vec!["git".to_string()]
        } else {
            match display_protocol {
                DisplayProtocol::Waypipe => vec![
                    "sway".to_string(),
                    "swaylock".to_string(),
                    "swayidle".to_string(),
                    "waybar".to_string(),
                    "i3status".to_string(),
                    "dmenu".to_string(),
                    "rofi".to_string(),
                    "wl-clipboard".to_string(),
                    "pipewire".to_string(),
                    "kitty".to_string(),
                    "git".to_string(),
                    "waypipe".to_string(),
                    "openssh-server".to_string(),
                ],
                DisplayProtocol::X2Go => vec![
                    "xorg-x11-server-Xorg".to_string(),
                    "xorg-x11-xinit".to_string(),
                    "i3".to_string(),
                    "i3status".to_string(),
                    "dmenu".to_string(),
                    "rofi".to_string(),
                    "x2goserver".to_string(),
                    "x2goserver-xsession".to_string(),
                    "pulseaudio".to_string(),
                    "pulseaudio-utils".to_string(),
                    "xclip".to_string(),
                    "kitty".to_string(),
                    "git".to_string(),
                    "openssh-server".to_string(),
                ],
            }
        };

        // Add user-specified system packages
        default_system_packages.extend(system_packages);

        Self {
            name,
            memory_mb,
            vcpus,
            disk_size_gb,
            vm_dir: "/var/lib/libvirt/images".to_string(),

            system_packages: default_system_packages,
            flatpak_packages: if headless { vec![] } else { flatpak_packages },
            auto_launch_apps: Vec::new(),

            graphics_backend: if headless {
                GraphicsBackend::VncOnly
            } else {
                GraphicsBackend::VirtioGpu
            },
            display_protocol,
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
