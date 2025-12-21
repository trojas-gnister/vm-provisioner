//! CPU pinning management
//!
//! This module handles CPU pinning for VM performance optimization:
//! - vCPU to host CPU core mapping
//! - Emulator CPU pinning (QEMU process)
//! - CPU topology configuration
//! - CPU mode (host-passthrough for best performance)

use crate::config::CpuMode;
use crate::error::{CpuError, Result};
use log::{debug, info, warn};
use std::fs;
use std::process::Command;

/// CPU pinning operations for AppVMProvisioner
pub trait CpuPinningOps {
    /// Validate CPU pinning configuration against host capabilities
    fn validate_cpu_pinning(&self) -> Result<()>;
    /// Generate libvirt XML for the <cputune> section
    fn generate_cputune_xml(&self) -> Result<String>;
    /// Generate libvirt XML for the <cpu> section
    fn generate_cpu_xml(&self) -> Result<String>;
    /// Apply CPU pinning to VM configuration (permanent mode via virsh)
    fn setup_cpu_pinning_permanent(&self) -> Result<()>;
}

impl CpuPinningOps for super::AppVMProvisioner {
    fn validate_cpu_pinning(&self) -> Result<()> {
        // Check if any CPU config is specified
        let has_config = self.config.cpu_pinning.cpu_affinity.is_some()
            || self.config.cpu_pinning.emulator_pin.is_some()
            || self.config.cpu_pinning.topology.is_some();

        if !has_config {
            return Ok(());
        }

        info!("Validating CPU pinning configuration...");

        // Get number of host CPUs
        let host_cpus = get_host_cpu_count()?;
        debug!("Host has {} CPU threads", host_cpus);

        // Validate cpu_affinity doesn't reference non-existent host CPUs
        if let Some(ref affinity) = self.config.cpu_pinning.cpu_affinity {
            for &cpu in affinity {
                if cpu >= host_cpus {
                    return Err(CpuError::InvalidCpuCore {
                        requested: cpu,
                        available: host_cpus,
                    }
                    .into());
                }
            }
        }

        // Validate emulator pin
        if let Some(ref emulator_cpus) = self.config.cpu_pinning.emulator_pin {
            for &cpu in emulator_cpus {
                if cpu >= host_cpus {
                    return Err(CpuError::InvalidCpuCore {
                        requested: cpu,
                        available: host_cpus,
                    }
                    .into());
                }
            }
        }

        // Validate topology if specified
        if let Some(ref topo) = self.config.cpu_pinning.topology {
            let total = topo.sockets * topo.cores * topo.threads;
            if total != self.config.vcpus {
                return Err(CpuError::TopologyMismatch {
                    topology_total: total,
                    vcpus: self.config.vcpus,
                }
                .into());
            }
        }

        debug!("CPU pinning configuration validated successfully");
        Ok(())
    }

    fn generate_cputune_xml(&self) -> Result<String> {
        // Check if we have any cputune settings
        let has_affinity = self.config.cpu_pinning.cpu_affinity.is_some();
        let has_emulator = self.config.cpu_pinning.emulator_pin.is_some();

        if !has_affinity && !has_emulator {
            return Ok(String::new());
        }

        let mut xml = String::from("  <cputune>\n");

        // Add vcpupin entries - each vCPU gets the same affinity set
        if let Some(ref affinity) = self.config.cpu_pinning.cpu_affinity {
            let cpuset = format_cpuset(affinity);
            for vcpu in 0..self.config.vcpus {
                xml.push_str(&format!(
                    "    <vcpupin vcpu='{}' cpuset='{}'/>\n",
                    vcpu, cpuset
                ));
            }
        }

        // Add emulatorpin if specified
        if let Some(ref emulator_cpus) = self.config.cpu_pinning.emulator_pin {
            let cpuset = format_cpuset(emulator_cpus);
            xml.push_str(&format!("    <emulatorpin cpuset='{}'/>\n", cpuset));
        }

        xml.push_str("  </cputune>");
        Ok(xml)
    }

    fn generate_cpu_xml(&self) -> Result<String> {
        // Check if we need to generate a CPU section
        let has_topology = self.config.cpu_pinning.topology.is_some();
        let has_affinity = self.config.cpu_pinning.cpu_affinity.is_some();

        // CPU section is needed for topology or if we have affinity (implies host-passthrough)
        if !has_topology && !has_affinity {
            return Ok(String::new());
        }

        let mode = match &self.config.cpu_pinning.cpu_mode {
            CpuMode::HostPassthrough => "host-passthrough",
            CpuMode::HostModel => "host-model",
            CpuMode::Custom(name) => name,
        };

        let mut xml = format!("  <cpu mode='{}'>\n", mode);

        if let Some(ref topo) = self.config.cpu_pinning.topology {
            xml.push_str(&format!(
                "    <topology sockets='{}' cores='{}' threads='{}'/>\n",
                topo.sockets, topo.cores, topo.threads
            ));
        }

        xml.push_str("  </cpu>");
        Ok(xml)
    }

    fn setup_cpu_pinning_permanent(&self) -> Result<()> {
        // Check if any CPU config is specified
        let has_config = self.config.cpu_pinning.cpu_affinity.is_some()
            || self.config.cpu_pinning.emulator_pin.is_some()
            || self.config.cpu_pinning.topology.is_some();

        if !has_config {
            return Ok(());
        }

        info!("Setting up CPU pinning (permanent mode)...");

        // Dump current XML
        let dumpxml = Command::new("virsh")
            .args(["-c", "qemu:///system", "dumpxml", &self.config.name])
            .output()?;

        if !dumpxml.status.success() {
            return Err(CpuError::XmlReadFailed(format!(
                "Failed to dump VM XML: {}",
                String::from_utf8_lossy(&dumpxml.stderr)
            ))
            .into());
        }

        let mut current_xml = String::from_utf8_lossy(&dumpxml.stdout).to_string();
        debug!("Current VM XML length: {} bytes", current_xml.len());

        // Generate CPU pinning XML sections
        let cputune_xml = self.generate_cputune_xml()?;
        let cpu_xml = self.generate_cpu_xml()?;

        debug!("Generated cputune XML:\n{}", cputune_xml);
        debug!("Generated cpu XML:\n{}", cpu_xml);

        // Update vcpu element to use static placement
        current_xml = update_vcpu_placement(&current_xml, self.config.vcpus);

        // Insert or replace cputune section
        if !cputune_xml.is_empty() {
            current_xml = insert_or_replace_section(&current_xml, "cputune", &cputune_xml);
        }

        // Insert or replace cpu section
        if !cpu_xml.is_empty() {
            current_xml = insert_or_replace_section(&current_xml, "cpu", &cpu_xml);
        }

        // Write modified XML to temp file and redefine
        let xml_path = format!("/tmp/{}-cpu-pinning.xml", self.config.name);
        fs::write(&xml_path, &current_xml).map_err(|e| {
            CpuError::XmlWriteFailed(format!("Failed to write temp XML: {}", e))
        })?;

        let define_result = Command::new("virsh")
            .args(["-c", "qemu:///system", "define", &xml_path])
            .output()?;

        // Clean up temp file
        let _ = fs::remove_file(&xml_path);

        if !define_result.status.success() {
            let stderr = String::from_utf8_lossy(&define_result.stderr);
            warn!("Failed to apply CPU pinning: {}", stderr);
            return Err(CpuError::ConfigurationFailed(stderr.to_string()).into());
        }

        info!("CPU pinning configured successfully");
        Ok(())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Format a list of CPU numbers as a cpuset string (e.g., "8,9,10,11" or "8-11")
fn format_cpuset(cpus: &[u32]) -> String {
    if cpus.is_empty() {
        return String::new();
    }

    // Simple comma-separated for now; could optimize to ranges
    cpus.iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Update the <vcpu> element to use static placement for pinning
fn update_vcpu_placement(xml: &str, vcpus: u32) -> String {
    // Find and replace <vcpu>N</vcpu> with <vcpu placement='static'>N</vcpu>
    // Also handles <vcpu placement='auto'>N</vcpu>
    let vcpu_pattern_simple = format!("<vcpu>{}</vcpu>", vcpus);
    let vcpu_pattern_auto = format!("<vcpu placement='auto'>{}</vcpu>", vcpus);
    let vcpu_replacement = format!("<vcpu placement='static'>{}</vcpu>", vcpus);

    xml.replace(&vcpu_pattern_simple, &vcpu_replacement)
        .replace(&vcpu_pattern_auto, &vcpu_replacement)
}

/// Insert or replace a section in the VM XML
fn insert_or_replace_section(xml: &str, section_name: &str, new_content: &str) -> String {
    let start_tag = format!("<{}", section_name);
    let end_tag = format!("</{}>", section_name);

    // Check if section exists
    if let Some(start_pos) = xml.find(&start_tag) {
        if let Some(end_pos) = xml[start_pos..].find(&end_tag) {
            // Replace existing section
            let absolute_end = start_pos + end_pos + end_tag.len();
            let mut result = String::new();
            result.push_str(&xml[..start_pos]);
            result.push_str(new_content);
            result.push_str(&xml[absolute_end..]);
            return result;
        }
    }

    // Insert before </domain>
    if let Some(pos) = xml.rfind("</domain>") {
        let mut result = String::new();
        result.push_str(&xml[..pos]);
        result.push_str(new_content);
        result.push('\n');
        result.push_str(&xml[pos..]);
        return result;
    }

    // Fallback: return original
    xml.to_string()
}

// ============================================================================
// Standalone CPU detection functions (for library consumers)
// ============================================================================

/// Get the number of CPU threads on the host
pub fn get_host_cpu_count() -> Result<u32> {
    let output = Command::new("nproc").output()?;
    let count = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(1);
    Ok(count)
}

/// Detect host CPU topology (physical cores, threads per core)
///
/// Returns (physical_cores, threads_per_core)
#[allow(dead_code)]
pub fn detect_host_cpu_topology() -> Result<(u32, u32)> {
    let online = fs::read_to_string("/sys/devices/system/cpu/online").unwrap_or_else(|_| "0".to_string());
    let total_cpus = parse_cpu_range(&online);

    // Try to detect hyperthreading
    let siblings =
        fs::read_to_string("/sys/devices/system/cpu/cpu0/topology/thread_siblings_list")
            .unwrap_or_else(|_| "0".to_string());

    let threads_per_core = parse_cpu_range(&siblings);
    let physical_cores = total_cpus / threads_per_core.max(1);

    Ok((physical_cores, threads_per_core))
}

/// Parse a CPU range string like "0-7" or "0,2,4,6" into a count
fn parse_cpu_range(range: &str) -> u32 {
    let mut count = 0;
    for part in range.trim().split(',') {
        if part.contains('-') {
            let bounds: Vec<&str> = part.split('-').collect();
            if bounds.len() == 2 {
                let start: u32 = bounds[0].parse().unwrap_or(0);
                let end: u32 = bounds[1].parse().unwrap_or(0);
                count += end - start + 1;
            }
        } else if !part.is_empty() {
            count += 1;
        }
    }
    count.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_cpuset() {
        assert_eq!(format_cpuset(&[8, 9, 10, 11]), "8,9,10,11");
        assert_eq!(format_cpuset(&[0]), "0");
        assert_eq!(format_cpuset(&[]), "");
    }

    #[test]
    fn test_parse_cpu_range() {
        assert_eq!(parse_cpu_range("0-7"), 8);
        assert_eq!(parse_cpu_range("0,2,4,6"), 4);
        assert_eq!(parse_cpu_range("0-3,8-11"), 8);
        assert_eq!(parse_cpu_range("0"), 1);
    }

    #[test]
    fn test_update_vcpu_placement() {
        let xml = "<domain><vcpu>4</vcpu></domain>";
        let result = update_vcpu_placement(xml, 4);
        assert!(result.contains("placement='static'"));
    }
}
