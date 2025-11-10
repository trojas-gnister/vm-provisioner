// VM Provisioner Library
// Exports modules for integration testing and external use

pub mod config;
pub mod display_bridge;
pub mod provisioner;
pub mod waypipe_manager;
pub mod x2go_manager;

// Re-export commonly used types
pub use config::{AppVMConfig, DisplayProtocol};
pub use display_bridge::DisplayBridge;
pub use waypipe_manager::WaypipeManager;
pub use x2go_manager::X2GoManager;
pub use provisioner::AppVMProvisioner;
