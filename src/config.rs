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

    // Vsock (for network-disabled VMs)
    #[serde(default)]
    pub vsock_cid: Option<u32>,  // Auto-assigned by libvirt, stored post-creation
    #[serde(default)]
    pub enable_vsock: bool,      // Auto-enabled for NetworkMode::None

    // Authentication
    pub user_password: String,

    // Custom kickstart additions (for library consumers)
    /// Custom kickstart script to inject before the final reboot command.
    /// Used by library consumers to add custom setup steps.
    #[serde(default)]
    pub custom_kickstart: Option<String>,
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
}

impl AppVMConfig {
    /// Validate a VM name
    ///
    /// VM names must be 1-64 characters, containing only alphanumeric chars,
    /// hyphens, or underscores.
    pub fn validate_vm_name(name: &str) -> crate::error::Result<()> {
        if name.is_empty() {
            return Err(crate::error::ConfigError::Invalid(
                "VM name cannot be empty".to_string(),
            )
            .into());
        }
        if name.len() > 64 {
            return Err(crate::error::ConfigError::Invalid(
                "VM name must be 64 characters or less".to_string(),
            )
            .into());
        }
        let valid = name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(crate::error::ConfigError::Invalid(
                "VM name must contain only alphanumeric characters, hyphens, or underscores"
                    .to_string(),
            )
            .into());
        }
        // Prevent names that could cause path traversal or shell issues
        if name.starts_with('-') || name.starts_with('.') {
            return Err(crate::error::ConfigError::Invalid(
                "VM name cannot start with '-' or '.'".to_string(),
            )
            .into());
        }
        Ok(())
    }

    /// Load a VM configuration from disk by name
    pub fn load(vm_name: &str) -> crate::error::Result<Self> {
        let config_path = Self::config_path(vm_name)?;
        let content = std::fs::read_to_string(&config_path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Get the path to a VM's configuration file
    pub fn config_path(vm_name: &str) -> crate::error::Result<String> {
        Ok(format!(
            "{}/.config/vm-provisioner/{}.toml",
            std::env::var("HOME")?,
            vm_name
        ))
    }

    /// Get the directory containing all VM configurations
    pub fn config_dir() -> crate::error::Result<String> {
        Ok(format!(
            "{}/.config/vm-provisioner",
            std::env::var("HOME")?
        ))
    }

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

            // Vsock is auto-enabled for network-disabled VMs
            vsock_cid: None, // Will be populated after VM creation
            enable_vsock: no_network,

            user_password: generate_password(),

            // No custom kickstart by default
            custom_kickstart: None,
        }
    }
}

fn generate_password() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

// ============================================================================
// Builder pattern for AppVMConfig (for library consumers)
// ============================================================================

/// Builder for creating AppVMConfig with sensible defaults.
///
/// This provides an ergonomic way to construct VM configurations
/// when using vm-provisioner as a library.
///
/// # Example
///
/// ```rust,ignore
/// use vm_provisioner::AppVMConfigBuilder;
///
/// let config = AppVMConfigBuilder::new("my-vm")
///     .memory_mb(2048)
///     .vcpus(2)
///     .add_system_package("nginx")
///     .build()?;
/// ```
#[derive(Debug, Clone)]
#[allow(dead_code)] // Library-only: used by external consumers, not the CLI binary
pub struct AppVMConfigBuilder {
    name: String,
    memory_mb: u64,
    vcpus: u32,
    disk_size_gb: u64,
    system_packages: Vec<String>,
    flatpak_packages: Vec<String>,
    headless: bool,
    pci_devices: Vec<PciDevice>,
    pci_hotplug: bool,
    usb_devices: Vec<UsbDevice>,
    usb_hotplug: bool,
    shared_folders: Vec<SharedFolder>,
    network_mode: NetworkMode,
    web_port: Option<u16>,
    grant_device_access: bool,
    custom_kickstart: Option<String>,
}

#[allow(dead_code)] // Library-only: used by external consumers, not the CLI binary
impl AppVMConfigBuilder {
    /// Create a new builder with required name and sensible defaults.
    ///
    /// Default values:
    /// - memory_mb: 2048
    /// - vcpus: 2
    /// - disk_size_gb: 20
    /// - headless: false
    /// - network_mode: Nat
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            memory_mb: 2048,
            vcpus: 2,
            disk_size_gb: 20,
            system_packages: Vec::new(),
            flatpak_packages: Vec::new(),
            headless: false,
            pci_devices: Vec::new(),
            pci_hotplug: false,
            usb_devices: Vec::new(),
            usb_hotplug: false,
            shared_folders: Vec::new(),
            network_mode: NetworkMode::Nat,
            web_port: None,
            grant_device_access: false,
            custom_kickstart: None,
        }
    }

    /// Set the VM memory in megabytes.
    pub fn memory_mb(mut self, mb: u64) -> Self {
        self.memory_mb = mb;
        self
    }

    /// Set the number of virtual CPUs.
    pub fn vcpus(mut self, count: u32) -> Self {
        self.vcpus = count;
        self
    }

    /// Set the disk size in gigabytes.
    pub fn disk_size_gb(mut self, gb: u64) -> Self {
        self.disk_size_gb = gb;
        self
    }

    /// Set the system packages to install.
    pub fn system_packages(mut self, packages: Vec<String>) -> Self {
        self.system_packages = packages;
        self
    }

    /// Add a single system package to install.
    pub fn add_system_package(mut self, package: impl Into<String>) -> Self {
        self.system_packages.push(package.into());
        self
    }

    /// Set the Flatpak packages to install.
    pub fn flatpak_packages(mut self, packages: Vec<String>) -> Self {
        self.flatpak_packages = packages;
        self
    }

    /// Add a single Flatpak package to install.
    pub fn add_flatpak_package(mut self, package: impl Into<String>) -> Self {
        self.flatpak_packages.push(package.into());
        self
    }

    /// Set whether the VM is headless (no GUI).
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Set the PCI devices to pass through.
    pub fn pci_devices(mut self, devices: Vec<PciDevice>) -> Self {
        self.pci_devices = devices;
        self
    }

    /// Add a single PCI device to pass through.
    pub fn add_pci_device(mut self, device: PciDevice) -> Self {
        self.pci_devices.push(device);
        self
    }

    /// Set whether to use PCI hotplug (attach/detach while VM running).
    pub fn pci_hotplug(mut self, hotplug: bool) -> Self {
        self.pci_hotplug = hotplug;
        self
    }

    /// Set the USB devices to pass through.
    pub fn usb_devices(mut self, devices: Vec<UsbDevice>) -> Self {
        self.usb_devices = devices;
        self
    }

    /// Add a single USB device to pass through.
    pub fn add_usb_device(mut self, device: UsbDevice) -> Self {
        self.usb_devices.push(device);
        self
    }

    /// Set whether to use USB hotplug.
    pub fn usb_hotplug(mut self, hotplug: bool) -> Self {
        self.usb_hotplug = hotplug;
        self
    }

    /// Set the shared folders (virtiofs).
    pub fn shared_folders(mut self, folders: Vec<SharedFolder>) -> Self {
        self.shared_folders = folders;
        self
    }

    /// Add a single shared folder.
    pub fn add_shared_folder(mut self, folder: SharedFolder) -> Self {
        self.shared_folders.push(folder);
        self
    }

    /// Set the network mode.
    pub fn network_mode(mut self, mode: NetworkMode) -> Self {
        self.network_mode = mode;
        self
    }

    /// Configure the VM with no network (airgapped).
    pub fn no_network(mut self) -> Self {
        self.network_mode = NetworkMode::None;
        self
    }

    /// Configure the VM with bridged networking.
    pub fn bridge(mut self, bridge_name: impl Into<String>) -> Self {
        self.network_mode = NetworkMode::Bridge(bridge_name.into());
        self
    }

    /// Set the web streaming port (for Selkies-GStreamer).
    pub fn web_port(mut self, port: u16) -> Self {
        self.web_port = Some(port);
        self
    }

    /// Grant Flatpak apps access to all devices.
    pub fn grant_device_access(mut self, grant: bool) -> Self {
        self.grant_device_access = grant;
        self
    }

    /// Set custom kickstart script additions.
    ///
    /// This script will be inserted into the kickstart file before
    /// the final cleanup and reboot commands.
    pub fn custom_kickstart(mut self, script: impl Into<String>) -> Self {
        self.custom_kickstart = Some(script.into());
        self
    }

    /// Build the final AppVMConfig.
    ///
    /// This validates the configuration and returns the built config.
    pub fn build(self) -> crate::error::Result<AppVMConfig> {
        // Validate VM name
        AppVMConfig::validate_vm_name(&self.name)?;

        // Determine network bridge if applicable
        let network_bridge = match &self.network_mode {
            NetworkMode::Bridge(name) => Some(name.clone()),
            _ => None,
        };

        let no_network = matches!(self.network_mode, NetworkMode::None);

        // Build config using existing constructor
        let mut config = AppVMConfig::new(
            self.name,
            self.memory_mb,
            self.vcpus,
            self.disk_size_gb,
            self.system_packages,
            self.flatpak_packages,
            self.headless,
            self.pci_devices,
            self.pci_hotplug,
            self.web_port,
            self.usb_devices,
            self.usb_hotplug,
            self.shared_folders,
            network_bridge,
            self.grant_device_access,
            no_network,
        );

        // Apply custom kickstart if provided
        config.custom_kickstart = self.custom_kickstart;

        Ok(config)
    }
}
