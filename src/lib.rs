//! VM Provisioner Library
//!
//! This crate provides VM provisioning capabilities with seamless window integration.
//! VMs are created using KVM/libvirt with Xpra for display forwarding.
//!
//! # Library Usage
//!
//! ```rust,ignore
//! use vm_provisioner::{AppVMConfigBuilder, AppVMProvisioner, Installation};
//!
//! let config = AppVMConfigBuilder::new("my-vm")
//!     .memory_mb(2048)
//!     .vcpus(2)
//!     .add_system_package("nginx")
//!     .build()?;
//!
//! let provisioner = AppVMProvisioner::new(config);
//! provisioner.provision_vm()?;
//! ```

pub mod config;
pub mod display_bridge;
pub mod error;
pub mod provisioner;
pub mod templates;
pub mod xpra_manager;

// Re-export commonly used types
pub use config::{
    AppVMConfig, AppVMConfigBuilder, DisplayProtocol, GraphicsBackend, NetworkMode, PciDevice,
    SharedFolder, UsbDevice,
};
pub use display_bridge::DisplayBridge;
pub use error::{Result, VmProvisionerError};
pub use provisioner::AppVMProvisioner;
pub use xpra_manager::XpraManager;

// Re-export traits for library consumers
pub use provisioner::installation::{Installation, FEDORA_VERSION};
pub use provisioner::lifecycle::Lifecycle;
pub use provisioner::pci::PciPassthrough;
pub use provisioner::usb::UsbPassthrough;

// Re-export IOMMU helper functions
pub use provisioner::{
    check_iommu_enabled, detect_pci_device, detect_usb_device, get_iommu_group, get_vsock_cid,
    is_clean_iommu_group, list_iommu_group_devices,
};
