//! Application shortcut operations
//!
//! This module handles generating desktop shortcuts and launching applications.

use log::{debug, error, info};

use vm_provisioner::config::AppVMConfig;
use vm_provisioner::error::Result;

use super::{get_display_bridge, get_vm_status};

/// Generate desktop shortcuts for VM applications
pub fn generate_shortcuts(name: String) -> Result<()> {
    info!("Generating application shortcuts for VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    if get_vm_status(&name) != "running" {
        error!(
            "VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    debug!("Waiting for VM to be fully ready...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let bridge = get_display_bridge(&config)?;
    bridge.generate_desktop_files()?;

    println!("\n✅ Application shortcuts created!");
    Ok(())
}

/// Launch an application in a VM
pub fn launch_app(name: String, app: String) -> Result<()> {
    info!("Launching application in VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    if get_vm_status(&name) != "running" {
        error!(
            "VM is not running. Start it with: vm-provisioner start {}",
            name
        );
        std::process::exit(1);
    }

    let bridge = get_display_bridge(&config)?;
    bridge.launch_app(&app)?;
    Ok(())
}

/// List applications available in a VM
pub fn list_apps(name: String) -> Result<()> {
    println!("📱 Applications available in VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    let bridge = get_display_bridge(&config)?;
    let apps = bridge.list_applications();

    if apps.is_empty() {
        println!("   No applications found.");
    } else {
        for app in apps {
            println!("   - {}", app);
        }
    }
    Ok(())
}
