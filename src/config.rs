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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsbDevice {
    pub vendor_id: String,    // "1234" (hex)
    pub product_id: String,   // "5678" (hex)
    pub description: String,  // "Logitech USB Webcam"
    pub bus: Option<u8>,      // USB bus number (for disambiguation)
    pub device: Option<u8>,   // Device number on bus
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SharedFolder {
    pub host_path: String,   // "/home/user/documents"
    pub guest_path: String,  // "/mnt/shared/documents"
    pub tag: String,         // virtiofs mount tag (auto-generated)
    pub readonly: bool,      // default: false (read-write)
}

// Xpra is the only supported display protocol
// Waypipe and Selkies have been deprecated
#[derive(Debug, Serialize, Clone, PartialEq, Default)]
pub enum DisplayProtocol {
    #[default]
    Xpra,
}

// Custom deserializer to handle migration from old configs
impl<'de> Deserialize<'de> for DisplayProtocol {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "xpra" => Ok(DisplayProtocol::Xpra),
            "waypipe" => {
                log::warn!("Waypipe protocol is deprecated. Migrating to Xpra.");
                Ok(DisplayProtocol::Xpra)
            }
            "selkies" => {
                log::warn!("Selkies protocol is deprecated. Migrating to Xpra.");
                Ok(DisplayProtocol::Xpra)
            }
            _ => Err(serde::de::Error::custom(format!(
                "unknown display protocol: {}. Only 'Xpra' is supported.",
                s
            ))),
        }
    }
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
    #[serde(default)]
    pub auto_launch_apps: Vec<String>, // Commands to run on startup

    // Graphics and windowing
    pub graphics_backend: GraphicsBackend,
    pub display_protocol: DisplayProtocol,
    #[serde(default)]
    pub web_port: Option<u16>, // Port for Selkies-GStreamer WebRTC web access (None = disabled)
    pub enable_clipboard: bool,
    pub enable_audio: bool,
    pub enable_usb_passthrough: bool,
    pub enable_auto_login: bool,
    pub headless: bool, // CLI-only mode, no GUI
    #[serde(default)]
    pub grant_device_access: bool, // Grant flatpak apps access to all devices

    // PCI passthrough
    pub pci_devices: Vec<PciDevice>,
    pub pci_hotplug: bool, // true = hot-attach/detach, false = permanent XML

    // USB passthrough
    pub usb_devices: Vec<UsbDevice>,
    pub usb_hotplug: bool, // true = hot-attach/detach, false = permanent XML

    // Shared storage (virtiofs)
    #[serde(default)]
    pub shared_folders: Vec<SharedFolder>,

    // Security settings
    pub network_mode: NetworkMode,
    pub firewall_rules: Vec<String>,
    #[serde(default)]
    pub vpn_config: Option<VpnConfig>,

    // Vsock (for network-disabled VMs)
    #[serde(default)]
    pub vsock_cid: Option<u32>,  // Auto-assigned by libvirt, stored post-creation
    #[serde(default)]
    pub enable_vsock: bool,      // Auto-enabled for NetworkMode::None

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
        web_port: Option<u16>,
        usb_devices: Vec<UsbDevice>,
        usb_hotplug: bool,
        shared_folders: Vec<SharedFolder>,
        network_bridge: Option<String>,
        grant_device_access: bool,
        no_network: bool,
    ) -> Self {
        // Default system packages - different for headless vs GUI
        let mut default_system_packages = if headless {
            // Headless mode: minimal packages, no GUI/X11
            vec!["git".to_string()]
        } else {
            // Xpra packages for GUI mode
            vec![
                "xpra".to_string(),
                "xorg-x11-server-Xvfb".to_string(),
                "pulseaudio-libs".to_string(),
                "git".to_string(),
                "openssh-server".to_string(),
            ]
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
            display_protocol: DisplayProtocol::Xpra,
            web_port,
            enable_clipboard: !headless,
            enable_audio: !headless,
            enable_usb_passthrough: !usb_devices.is_empty(),
            enable_auto_login: !headless,
            headless,
            grant_device_access,

            pci_devices,
            pci_hotplug,

            usb_devices,
            usb_hotplug,

            shared_folders,

            network_mode: if no_network {
                NetworkMode::None
            } else {
                match network_bridge {
                    Some(bridge) => NetworkMode::Bridge(bridge),
                    None => NetworkMode::Nat,
                }
            },
            firewall_rules: vec![
                // Allow DNS
                "OUTPUT -p udp --dport 53 -j ACCEPT".to_string(),
                "OUTPUT -p tcp --dport 53 -j ACCEPT".to_string(),
                // Allow HTTP/HTTPS
                "OUTPUT -p tcp --dport 80 -j ACCEPT".to_string(),
                "OUTPUT -p tcp --dport 443 -j ACCEPT".to_string(),
            ],
            vpn_config: None,

            // Vsock is auto-enabled for network-disabled VMs
            vsock_cid: None, // Will be populated after VM creation
            enable_vsock: no_network,

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
        .expect("System clock is set before UNIX epoch")
        .as_nanos()
        .hash(&mut hasher);
    format!("vm-{:x}", hasher.finish())
        .chars()
        .take(12)
        .collect()
}
