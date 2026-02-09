use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    pub enable_clipboard: bool,
    pub enable_audio: bool,
    pub enable_usb_passthrough: bool,
    pub enable_auto_login: bool,
    pub headless: bool, // CLI-only mode, no GUI
    #[serde(default)]
    pub grant_device_access: bool, // Grant flatpak apps access to all devices

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

    // Custom NixOS configuration additions (for library consumers)
    /// Extra Nix configuration snippet to append to the generated configuration.nix.
    #[serde(default)]
    pub custom_nix_config: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub enum GraphicsBackend {
    VirtioGpu, // Venus virtio-gpu with 3D acceleration
    VncOnly,   // Fallback
}

// Custom deserializer to handle migration from old configs with QxlSpice
impl<'de> Deserialize<'de> for GraphicsBackend {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "VirtioGpu" => Ok(GraphicsBackend::VirtioGpu),
            "VncOnly" => Ok(GraphicsBackend::VncOnly),
            "QxlSpice" => {
                log::warn!("QxlSpice is deprecated. Migrating to VirtioGpu (Venus).");
                Ok(GraphicsBackend::VirtioGpu)
            }
            _ => Err(serde::de::Error::custom(format!(
                "unknown graphics backend: {}. Supported: 'VirtioGpu', 'VncOnly'.",
                s
            ))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NetworkMode {
    Nat,
    None,
    Bridge(String),
}

#[allow(clippy::too_many_arguments)]
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
        let home = std::env::var("HOME")?;
        let path = PathBuf::from(home)
            .join(".config")
            .join("vm-provisioner")
            .join(format!("{}.toml", vm_name));
        Ok(path.to_string_lossy().to_string())
    }

    /// Get the directory containing all VM configurations
    pub fn config_dir() -> crate::error::Result<String> {
        let home = std::env::var("HOME")?;
        let path = PathBuf::from(home)
            .join(".config")
            .join("vm-provisioner");
        Ok(path.to_string_lossy().to_string())
    }

    pub fn new(
        name: String,
        memory_mb: u64,
        vcpus: u32,
        disk_size_gb: u64,
        system_packages: Vec<String>,
        flatpak_packages: Vec<String>,
        headless: bool,
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
            // GUI mode: base packages
            vec![
                "openbox".to_string(),
                "xterm".to_string(),
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
            enable_clipboard: !headless,
            enable_audio: !headless,
            enable_usb_passthrough: !usb_devices.is_empty(),
            enable_auto_login: !headless,
            headless,
            grant_device_access,

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

            // No custom NixOS config by default
            custom_nix_config: None,
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
pub struct AppVMConfigBuilder {
    name: String,
    memory_mb: u64,
    vcpus: u32,
    disk_size_gb: u64,
    system_packages: Vec<String>,
    flatpak_packages: Vec<String>,
    headless: bool,
    usb_devices: Vec<UsbDevice>,
    usb_hotplug: bool,
    shared_folders: Vec<SharedFolder>,
    network_mode: NetworkMode,
    grant_device_access: bool,
    custom_nix_config: Option<String>,
}

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
            usb_devices: Vec::new(),
            usb_hotplug: false,
            shared_folders: Vec::new(),
            network_mode: NetworkMode::Nat,
            grant_device_access: false,
            custom_nix_config: None,
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

    /// Grant Flatpak apps access to all devices.
    pub fn grant_device_access(mut self, grant: bool) -> Self {
        self.grant_device_access = grant;
        self
    }

    /// Set custom NixOS configuration snippet to append.
    pub fn custom_nix_config(mut self, config: impl Into<String>) -> Self {
        self.custom_nix_config = Some(config.into());
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
            self.usb_devices,
            self.usb_hotplug,
            self.shared_folders,
            network_bridge,
            self.grant_device_access,
            no_network,
        );

        // Apply custom NixOS config if provided
        config.custom_nix_config = self.custom_nix_config;

        Ok(config)
    }
}
