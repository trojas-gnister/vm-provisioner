//! Virsh/libvirt utility functions
//!
//! Centralized utilities for interacting with VMs via virsh commands.
//! This module provides both low-level command execution helpers and
//! high-level operations for VM management.

use crate::error::{ProvisioningError, Result};
use log::debug;
use std::process::Command;

/// Default libvirt connection URI for QEMU/KVM system VMs
pub const QEMU_URI: &str = "qemu:///system";

// ============================================================================
// Command Builders
// ============================================================================

/// Create a virsh command with the default QEMU connection URI
///
/// Returns a Command that can be further customized with additional arguments.
/// Does not use sudo - for user-level virsh operations.
pub fn virsh_command() -> Command {
    let mut cmd = Command::new("virsh");
    cmd.args(["-c", QEMU_URI]);
    cmd
}

/// Create a virsh command with sudo and the default QEMU connection URI
///
/// Returns a Command that can be further customized with additional arguments.
/// Uses sudo for privileged operations like starting/stopping VMs.
pub fn virsh_sudo_command() -> Command {
    let mut cmd = Command::new("sudo");
    cmd.args(["virsh", "-c", QEMU_URI]);
    cmd
}

// ============================================================================
// Checked Execution Helpers
// ============================================================================

/// Execute a virsh command (without sudo) and return output or error
///
/// Returns the stdout as a String if successful, or an error if the command
/// fails (either IO error or non-zero exit code).
pub fn run_checked(args: &[&str]) -> Result<String> {
    debug!("virsh {}", args.join(" "));
    let output = virsh_command().args(args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ProvisioningError::Installation(format!(
            "virsh {} failed: {}",
            args.join(" "),
            stderr
        ))
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Execute a virsh command with sudo and return output or error
///
/// Returns the stdout as a String if successful, or an error if the command
/// fails (either IO error or non-zero exit code).
pub fn run_sudo_checked(args: &[&str]) -> Result<String> {
    debug!("sudo virsh {}", args.join(" "));
    let output = virsh_sudo_command().args(args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ProvisioningError::Installation(format!(
            "virsh {} failed: {}",
            args.join(" "),
            stderr
        ))
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Execute a virsh command with sudo, ignoring failures
///
/// Useful for cleanup operations where failure is acceptable.
/// Returns true if successful, false otherwise. Logs failures at debug level.
pub fn run_sudo_unchecked(args: &[&str]) -> bool {
    debug!("sudo virsh {} (unchecked)", args.join(" "));
    match virsh_sudo_command().args(args).output() {
        Ok(output) => {
            if !output.status.success() {
                debug!(
                    "virsh {} returned non-zero (ignored): {}",
                    args.join(" "),
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            output.status.success()
        }
        Err(e) => {
            debug!("virsh {} failed (ignored): {}", args.join(" "), e);
            false
        }
    }
}

// ============================================================================
// High-Level Operations
// ============================================================================

/// Attach a device to a VM using XML definition
///
/// # Arguments
/// * `vm_name` - Name of the VM
/// * `xml_path` - Path to the XML file defining the device
/// * `live` - If true, attach to running VM
/// * `config` - If true, make permanent in VM config
pub fn attach_device(vm_name: &str, xml_path: &str, live: bool, config: bool) -> Result<()> {
    let mut args = vec!["attach-device", vm_name, xml_path];
    if live {
        args.push("--live");
    }
    if config {
        args.push("--config");
    }
    run_sudo_checked(&args)?;
    Ok(())
}

/// Detach a device from a VM using XML definition
///
/// # Arguments
/// * `vm_name` - Name of the VM
/// * `xml_path` - Path to the XML file defining the device
/// * `live` - If true, detach from running VM
/// * `config` - If true, remove from VM config permanently
pub fn detach_device(vm_name: &str, xml_path: &str, live: bool, config: bool) -> Result<()> {
    let mut args = vec!["detach-device", vm_name, xml_path];
    if live {
        args.push("--live");
    }
    if config {
        args.push("--config");
    }
    run_sudo_checked(&args)?;
    Ok(())
}

/// Get the XML definition of a VM
pub fn dumpxml(vm_name: &str) -> Result<String> {
    run_sudo_checked(&["dumpxml", vm_name])
}

/// Check if a VM domain exists in libvirt
pub fn domain_exists(vm_name: &str) -> bool {
    run_sudo_checked(&["dominfo", vm_name]).is_ok()
}

/// List all VMs (returns raw virsh output)
pub fn list_all() -> Result<String> {
    run_sudo_checked(&["list", "--all"])
}

/// Undefine (delete) a VM, optionally with storage
///
/// # Arguments
/// * `vm_name` - Name of the VM to undefine
/// * `remove_storage` - If true, also remove storage volumes
pub fn undefine(vm_name: &str, remove_storage: bool) -> Result<()> {
    let args = if remove_storage {
        vec!["undefine", vm_name, "--remove-all-storage", "--nvram"]
    } else {
        vec!["undefine", vm_name]
    };
    run_sudo_checked(&args)?;
    Ok(())
}

/// Set VM memory configuration
pub fn set_memory(vm_name: &str, memory_mb: u64, max_memory: bool) -> Result<()> {
    let mem_str = format!("{}M", memory_mb);
    let cmd = if max_memory { "setmaxmem" } else { "setmem" };
    run_sudo_checked(&[cmd, vm_name, &mem_str, "--config"])?;
    Ok(())
}

/// Set VM vCPU count configuration
pub fn set_vcpus(vm_name: &str, count: u32, maximum: bool) -> Result<()> {
    let count_str = count.to_string();
    let flag = if maximum { "--maximum" } else { "--current" };
    run_sudo_checked(&["setvcpus", vm_name, &count_str, "--config", flag])?;
    Ok(())
}

// ============================================================================
// Update Orchestration
// ============================================================================

/// Result of an update operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateResult {
    /// Update was successfully applied
    Applied,
    /// Update requires VM to be stopped first
    RequiresStop(String),
    /// Update is not allowed (e.g., decreasing vCPUs)
    NotAllowed(String),
}

/// Update VM memory with safety checks.
///
/// Memory updates require the VM to be stopped. This function checks the VM
/// state and returns an appropriate result.
///
/// # Arguments
/// * `vm_name` - Name of the VM
/// * `memory_mb` - New memory size in megabytes
///
/// # Returns
/// - `Ok(UpdateResult::Applied)` if update succeeded
/// - `Ok(UpdateResult::RequiresStop)` if VM is running
pub fn update_memory(vm_name: &str, memory_mb: u64) -> Result<UpdateResult> {
    let state = get_vm_state(vm_name).unwrap_or_else(|| "unknown".to_string());
    if state == "running" {
        return Ok(UpdateResult::RequiresStop(
            "Memory update requires VM to be stopped".into(),
        ));
    }

    set_memory(vm_name, memory_mb, true)?; // max
    set_memory(vm_name, memory_mb, false)?; // current
    Ok(UpdateResult::Applied)
}

/// Update VM vCPU count with safety checks.
///
/// vCPU updates require the VM to be stopped. Additionally, vCPU count
/// can only be increased, not decreased (libvirt limitation).
///
/// # Arguments
/// * `vm_name` - Name of the VM
/// * `vcpus` - New vCPU count
/// * `old_vcpus` - Current vCPU count (for validation)
///
/// # Returns
/// - `Ok(UpdateResult::Applied)` if update succeeded
/// - `Ok(UpdateResult::RequiresStop)` if VM is running
/// - `Ok(UpdateResult::NotAllowed)` if trying to decrease vCPUs
pub fn update_vcpus(vm_name: &str, vcpus: u32, old_vcpus: u32) -> Result<UpdateResult> {
    if vcpus < old_vcpus {
        return Ok(UpdateResult::NotAllowed(
            "Cannot decrease vCPU count".into(),
        ));
    }

    let state = get_vm_state(vm_name).unwrap_or_else(|| "unknown".to_string());
    if state == "running" {
        return Ok(UpdateResult::RequiresStop(
            "vCPU update requires VM to be stopped".into(),
        ));
    }

    set_vcpus(vm_name, vcpus, true)?; // max
    set_vcpus(vm_name, vcpus, false)?; // current
    Ok(UpdateResult::Applied)
}

/// Get display connection info for a VM
pub fn get_display(vm_name: &str) -> Option<String> {
    run_checked(&["domdisplay", vm_name])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ============================================================================
// Query Functions (return Option for non-critical lookups)
// ============================================================================

/// Get the IP address of a running VM from libvirt
///
/// Queries `virsh domifaddr` and parses the IPv4 address from the output.
/// Returns None if the VM is not running or has no IP assigned yet.
///
/// For bridged networks, this falls back to querying the QEMU guest agent.
pub fn get_vm_ip(vm_name: &str) -> Option<String> {
    // First try the standard method (works for NAT networks)
    if let Ok(output) = run_sudo_checked(&["domifaddr", vm_name]) {
        if let Some(ip) = parse_ip_from_domifaddr(&output) {
            return Some(ip);
        }
    }

    // Fall back to guest agent method (works for bridged networks)
    // This requires qemu-guest-agent installed and running in the VM
    if let Ok(output) = run_sudo_checked(&["domifaddr", vm_name, "--source", "agent"]) {
        if let Some(ip) = parse_ip_from_domifaddr_agent(&output) {
            return Some(ip);
        }
    }

    None
}

/// Parse an IPv4 address from virsh domifaddr output
///
/// Expected format:
/// ```text
///  Name       MAC address          Protocol     Address
///  vnet0      52:54:00:xx:xx:xx    ipv4         192.168.122.x/24
/// ```
pub fn parse_ip_from_domifaddr(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.contains("ipv4") {
            if let Some(ip_part) = line.split_whitespace().nth(3) {
                if let Some(ip) = ip_part.split('/').next() {
                    // Validate IP address format
                    if validate_ip_address(ip) {
                        return Some(ip.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Parse an IPv4 address from virsh domifaddr --source agent output
///
/// Guest agent output format is similar but may include more interfaces.
/// We skip loopback (127.x.x.x) and link-local (169.254.x.x) addresses.
///
/// Example output:
/// ```text
///  Name       MAC address          Protocol     Address
///  lo         00:00:00:00:00:00    ipv4         127.0.0.1/8
///  enp1s0     52:54:00:xx:xx:xx    ipv4         192.168.1.100/24
/// ```
pub fn parse_ip_from_domifaddr_agent(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.contains("ipv4") {
            if let Some(ip_part) = line.split_whitespace().nth(3) {
                if let Some(ip) = ip_part.split('/').next() {
                    // Skip loopback and link-local addresses
                    if ip.starts_with("127.") || ip.starts_with("169.254.") {
                        continue;
                    }
                    // Validate IP address format
                    if validate_ip_address(ip) {
                        return Some(ip.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Validate that a string is a valid IPv4 address
pub fn validate_ip_address(ip: &str) -> bool {
    ip.parse::<std::net::Ipv4Addr>().is_ok()
}

/// Get the current state of a VM from libvirt
///
/// Returns states like "running", "shut off", "paused", etc.
pub fn get_vm_state(vm_name: &str) -> Option<String> {
    run_sudo_checked(&["domstate", vm_name])
        .ok()
        .map(|s| s.trim().to_string())
}

/// Check if a VM is currently running
pub fn is_vm_running(vm_name: &str) -> bool {
    get_vm_state(vm_name)
        .map(|s| s == "running")
        .unwrap_or(false)
}

/// Start a VM
///
/// Returns Ok(()) on success, Err on failure.
pub fn start(vm_name: &str) -> Result<()> {
    run_sudo_checked(&["start", vm_name])?;
    Ok(())
}

/// Start a VM, ignoring errors (useful for "start if not running" scenarios)
pub fn start_if_stopped(vm_name: &str) -> bool {
    run_sudo_unchecked(&["start", vm_name])
}

/// Shutdown a VM gracefully
///
/// Returns Ok(()) on success, Err on failure.
pub fn shutdown(vm_name: &str) -> Result<()> {
    run_sudo_checked(&["shutdown", vm_name])?;
    Ok(())
}

/// Shutdown a VM gracefully, ignoring errors
pub fn shutdown_unchecked(vm_name: &str) -> bool {
    run_sudo_unchecked(&["shutdown", vm_name])
}

/// Destroy (force stop) a VM
///
/// Returns Ok(()) on success, Err on failure.
pub fn destroy(vm_name: &str) -> Result<()> {
    run_sudo_checked(&["destroy", vm_name])?;
    Ok(())
}

/// Define a VM from an XML file
pub fn define(xml_path: &str) -> Result<()> {
    run_sudo_checked(&["define", xml_path])?;
    Ok(())
}

/// Destroy (force stop) a VM, ignoring errors
pub fn destroy_unchecked(vm_name: &str) -> bool {
    run_sudo_unchecked(&["destroy", vm_name])
}
