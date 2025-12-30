//! VM lifecycle operations
//!
//! This module handles start, stop, destroy, list, passwords, and console commands.

use dialoguer::Confirm;
use log::{debug, info, warn};
use std::path::Path;

use vm_provisioner::config::AppVMConfig;
use vm_provisioner::error::Result;
use vm_provisioner::passwords::VMPasswords;
use vm_provisioner::provisioner::{AppVMProvisioner, Lifecycle};
use vm_provisioner::virsh;

use super::{get_display_bridge, get_vm_status};

/// Start a VM
pub fn start_vm(name: String) -> Result<()> {
    info!("Starting VM: {}", name);
    let config = AppVMConfig::load(&name)?;

    AppVMProvisioner::new(config.clone()).start_vm()?;

    if config.headless {
        println!(
            "\n💡 Headless VM - connect via console: virsh console {}",
            name
        );
    } else {
        println!("\n🪟 Seamless window integration enabled via Xpra");
        println!(
            "   Use `vm-provisioner generate-shortcuts {}` to create .desktop files.",
            name
        );
        if let Some(port) = config.web_port {
            println!(
                "   Or access via browser: http://<vm-ip>:{}/ (Selkies WebRTC)",
                port
            );
        }
    }
    Ok(())
}

/// Stop a VM
pub fn stop_vm(name: String) -> Result<()> {
    info!("Stopping VM: {}", name);
    let config = AppVMConfig::load(&name)?;
    AppVMProvisioner::new(config).stop_vm()?;
    info!("VM stopped");
    Ok(())
}

/// List all configured VMs
pub fn list_vms() -> Result<()> {
    println!("📋 Available VMs:");
    let config_dir = AppVMConfig::config_dir()?;
    if !Path::new(&config_dir).exists() {
        println!("No VMs configured yet.");
        return Ok(());
    }
    for entry in std::fs::read_dir(&config_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            // Skip the password file
            if path.file_stem().and_then(|s| s.to_str()) == Some("vm-passwords") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str::<AppVMConfig>(&content) {
                    println!("  {} [{}]", config.name, get_vm_status(&config.name));
                }
            }
        }
    }
    Ok(())
}

/// Destroy a VM and all its data
pub fn destroy_vm(name: String, skip_confirm: bool) -> Result<()> {
    if !skip_confirm
        && !Confirm::new()
            .with_prompt(format!(
                "Permanently delete VM '{}' and all its data?",
                name
            ))
            .default(false)
            .interact()?
    {
        warn!("Destruction cancelled");
        return Ok(());
    }

    let config_file = AppVMConfig::config_path(&name)?;
    let config = AppVMConfig::load(&name)?;

    let bridge = get_display_bridge(&config)?;
    bridge.remove_desktop_files()?;

    // Clean up SSH known_hosts entry for this VM
    if let Some(ip) = virsh::get_vm_ip(&name) {
        debug!("Cleaning up SSH key for {}", ip);
        let _ = std::process::Command::new("ssh-keygen")
            .args(["-R", &ip])
            .output();
    }

    AppVMProvisioner::new(config).destroy_vm()?;
    std::fs::remove_file(&config_file)?;
    info!("VM destroyed");
    Ok(())
}

/// Connect to VM console
pub fn connect_console(name: String) -> Result<()> {
    info!("Connecting to VM console: {}", name);
    std::process::Command::new("virsh")
        .args(["-c", "qemu:///system", "console", &name])
        .status()?;
    Ok(())
}

/// Show stored VM passwords
pub fn show_passwords() -> Result<()> {
    let config_dir = AppVMConfig::config_dir()?;
    let passwords = VMPasswords::load_or_create(&config_dir)?;
    if passwords.is_empty() {
        println!("No VM passwords stored yet");
        return Ok(());
    }
    println!("VM Login Credentials:");
    for (vm_name, password) in passwords.iter() {
        println!("   {} | user:{}", vm_name, password);
    }
    Ok(())
}
