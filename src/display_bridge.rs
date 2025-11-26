//! Display bridge trait for display protocol abstraction
//!
//! This module defines the `DisplayBridge` trait that abstracts display forwarding
//! between the host and VM. Currently, Xpra is the only supported implementation.

use crate::config::AppVMConfig;
use crate::error::Result;

/// Trait for display protocol implementations
///
/// Implementors of this trait handle the display forwarding between
/// the host and the VM, including:
/// - Generating exec commands for .desktop files
/// - Launching applications via the display protocol
/// - Managing desktop file integration
pub trait DisplayBridge {
    /// Initializes the bridge with the VM's configuration.
    fn new(config: &AppVMConfig) -> Result<Self>
    where
        Self: Sized;

    /// Generates the `Exec=` line for a .desktop file.
    fn generate_exec_command(&self, app_command: &str) -> String;

    /// Launches an application directly.
    fn launch_app(&self, app_command: &str) -> Result<()>;

    /// Generates .desktop files for the applications in the VM.
    fn generate_desktop_files(&self) -> Result<()>;

    /// Removes the .desktop files for the applications in the VM.
    fn remove_desktop_files(&self) -> Result<()>;

    /// Lists the applications available in the VM.
    fn list_applications(&self) -> Vec<String>;

    /// Returns a list of packages required on the guest VM.
    fn guest_packages(&self) -> Vec<String>;

    /// Generates any protocol-specific configuration for the kickstart file.
    fn kickstart_config(&self, ssh_public_key: &str) -> String;
}
