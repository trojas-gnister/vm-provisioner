//! VM Provisioner Library
//!
//! This crate provides VM provisioning capabilities with NixOS-based
//! isolated application VMs.
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
pub mod constants;
pub mod error;
pub mod libvirt_xml;
pub mod nixos;
pub mod passwords;
pub mod provisioner;
pub mod validation;
pub mod virsh;

// Re-export commonly used types
pub use config::{AppVMConfig, AppVMConfigBuilder, GraphicsBackend, NetworkMode, SharedFolder, UsbDevice};
pub use error::{Result, VmProvisionerError};
pub use provisioner::AppVMProvisioner;

// Re-export traits for library consumers
pub use provisioner::installation::Installation;
pub use provisioner::lifecycle::Lifecycle;
pub use provisioner::usb::UsbPassthrough;

// Re-export device detection helpers
pub use provisioner::{
    detect_gpu_render_nodes, detect_usb_device, get_vsock_cid, select_gpu_for_venus, GpuRenderNode,
    GpuVendor,
};

// Re-export validation helpers
pub use validation::{validate_shared_folder, validate_usb_device};

// Re-export virsh update orchestration
pub use virsh::{update_memory, update_vcpus, UpdateResult};
