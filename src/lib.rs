//! VM Provisioner Library
//!
//! This crate provides VM provisioning capabilities with seamless window integration.
//! VMs are created using KVM/libvirt with Xpra for display forwarding.

pub mod config;
pub mod display_bridge;
pub mod error;
pub mod provisioner;
pub mod templates;
pub mod xpra_manager;

// Re-export commonly used types
pub use config::{AppVMConfig, DisplayProtocol, NetworkMode, PciDevice, SharedFolder, UsbDevice};
pub use display_bridge::DisplayBridge;
pub use error::{Result, VmProvisionerError};
pub use provisioner::AppVMProvisioner;
pub use xpra_manager::XpraManager;
