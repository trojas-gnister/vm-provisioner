//! VM Provisioner module
//!
//! This module handles all aspects of VM lifecycle management:
//! - Device detection and validation
//! - Kickstart configuration generation
//! - VM installation and provisioning
//! - USB passthrough management
//! - Network interface management
//! - VM lifecycle (start/stop/destroy)

pub mod device_detection;
pub mod installation;
pub mod lifecycle;
pub mod network;
pub mod usb;

use crate::config::AppVMConfig;

// Re-export public functions for external use
pub use device_detection::{
    detect_gpu_render_nodes, detect_usb_device, get_vsock_cid, select_gpu_for_venus, GpuRenderNode,
    GpuVendor,
};

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
