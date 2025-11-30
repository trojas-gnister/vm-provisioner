//! VM Provisioner module
//!
//! This module handles all aspects of VM lifecycle management:
//! - Device detection and validation
//! - Kickstart configuration generation
//! - VM installation and provisioning
//! - PCI/USB passthrough management
//! - Network interface management
//! - VM lifecycle (start/stop/destroy)

pub mod device_detection;
pub mod installation;
pub mod kickstart;
pub mod lifecycle;
pub mod network;
pub mod pci;
pub mod usb;

use crate::config::AppVMConfig;

// Re-export public functions for external use
pub use device_detection::{detect_pci_device, detect_usb_device, get_vsock_cid};
#[allow(unused_imports)] // Library-only exports: used by external consumers, not the CLI binary
pub use pci::{check_iommu_enabled, get_iommu_group, is_clean_iommu_group, list_iommu_group_devices};

// Re-export traits for method access
pub use installation::Installation;
pub use lifecycle::Lifecycle;

/// Main provisioner struct that orchestrates VM creation and management
pub struct AppVMProvisioner {
    /// The VM configuration. Public for library consumers to access/modify.
    pub config: AppVMConfig,
}

impl AppVMProvisioner {
    /// Create a new provisioner instance with the given configuration
    pub fn new(config: AppVMConfig) -> Self {
        Self { config }
    }
}
