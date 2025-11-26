//! Error types for vm-provisioner
//!
//! This module provides a hierarchical error system using `thiserror`:
//! - `VmProvisionerError`: Top-level error enum for all operations
//! - Module-specific errors: `ConfigError`, `ProvisioningError`, `PciError`, `UsbError`, `DisplayError`

use thiserror::Error;

/// Top-level error type for all vm-provisioner operations
#[derive(Error, Debug)]
pub enum VmProvisionerError {
    // Module-specific error wrappers
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Provisioning error: {0}")]
    Provisioning(#[from] ProvisioningError),

    #[error("PCI passthrough error: {0}")]
    Pci(#[from] PciError),

    #[error("USB passthrough error: {0}")]
    Usb(#[from] UsbError),

    #[error("Display bridge error: {0}")]
    Display(#[from] DisplayError),

    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    // Common error variants
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("User interaction error: {0}")]
    Dialoguer(#[from] dialoguer::Error),

    #[error("Environment variable error: {0}")]
    Env(#[from] std::env::VarError),
}

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

/// VM provisioning errors
#[derive(Error, Debug)]
pub enum ProvisioningError {
    #[error("Missing prerequisite: {cmd}. Install with: {install_hint}")]
    MissingPrerequisite { cmd: String, install_hint: String },

    #[error("ISO download failed: {0}")]
    IsoDownload(String),

    #[error("Installation failed: {0}")]
    Installation(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("SSH host key acceptance failed: {0}")]
    SshKeyAcceptance(String),

    #[error("Kickstart generation failed: {0}")]
    KickstartGeneration(String),

    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),
}

/// PCI passthrough errors
#[derive(Error, Debug)]
pub enum PciError {
    #[error("PCI device not found: {0}. Run 'lspci' to see available devices.")]
    DeviceNotFound(String),

    #[error("IOMMU not enabled. Enable VT-d (Intel) or AMD-Vi (AMD) in BIOS and add 'intel_iommu=on' or 'amd_iommu=on' to kernel cmdline.")]
    IommuNotEnabled,

    #[error("vfio-pci kernel module not available")]
    VfioNotAvailable,

    #[error("Invalid PCI address format: {0}. Expected format: '0000:01:00.0'")]
    InvalidAddress(String),

    #[error("Failed to parse vendor:device IDs from lspci output")]
    ParseError,
}

/// USB passthrough errors
#[derive(Error, Debug)]
pub enum UsbError {
    #[error("USB device not found: {0}. Run 'lsusb' to see available devices.")]
    DeviceNotFound(String),

    #[error("Invalid USB address format: {0}. Expected 'vendor:product' (e.g., '046d:c52b')")]
    InvalidFormat(String),

    #[error("Invalid USB vendor:product IDs: {0}. Both should be 4 hex digits (e.g., '046d:c52b')")]
    InvalidIds(String),

    #[error("Failed to attach USB device {vendor_id}:{product_id}: {reason}")]
    AttachFailed {
        vendor_id: String,
        product_id: String,
        reason: String,
    },

    #[error("Failed to detach USB device {vendor_id}:{product_id}: {reason}")]
    DetachFailed {
        vendor_id: String,
        product_id: String,
        reason: String,
    },
}

/// Display bridge errors (Xpra-related)
#[derive(Error, Debug)]
pub enum DisplayError {
    #[error("xpra binary not found on host. Install with: sudo dnf install xpra")]
    XpraNotFound,

    #[error("Failed to execute xpra: {0}")]
    XpraExecution(String),

    #[error("Cannot connect to VM: {0}")]
    ConnectionFailed(String),

    #[error("Vsock CID not configured. VM may need reprovisioning with --no-network")]
    VsockNotConfigured,

    #[error("Application launch failed: {0}")]
    LaunchFailed(String),
}

/// Network-related errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Bridge interface not found: {0}")]
    BridgeNotFound(String),

    #[error("Failed to remove network interface: {0}")]
    InterfaceRemovalFailed(String),

    #[error("Failed to retrieve vsock CID: {0}")]
    VsockCidRetrievalFailed(String),

    #[error("Conflicting network options: {0}")]
    ConflictingOptions(String),
}

/// Type alias for Results using VmProvisionerError
pub type Result<T> = std::result::Result<T, VmProvisionerError>;
