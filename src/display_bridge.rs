use crate::config::AppVMConfig;
use std::error::Error;

pub trait DisplayBridge {
    /// Initializes the bridge with the VM's configuration.
    fn new(config: &AppVMConfig) -> Result<Self, Box<dyn Error>>
    where
        Self: Sized;

    /// Generates the `Exec=` line for a .desktop file.
    fn generate_exec_command(&self, app_command: &str) -> String;

    /// Launches an application directly.
    fn launch_app(&self, app_command: &str) -> Result<(), Box<dyn Error>>;

    /// Generates .desktop files for the applications in the VM.
    fn generate_desktop_files(&self) -> Result<(), Box<dyn Error>>;

    /// Removes the .desktop files for the applications in the VM.
    fn remove_desktop_files(&self) -> Result<(), Box<dyn Error>>;

    /// Lists the applications available in the VM.
    fn list_applications(&self) -> Vec<String>;

    /// Returns a list of packages required on the guest VM.
    fn guest_packages(&self) -> Vec<String>;

    /// Generates any protocol-specific configuration for the kickstart file.
    fn kickstart_config(&self, ssh_public_key: &str) -> String;
}
