//! VM password management
//!
//! This module handles storage and retrieval of VM passwords in a local TOML file.
//! Passwords are stored in `~/.config/vm-provisioner/vm-passwords.toml`.

use crate::constants::{CONFIG_DIR_NAME, PASSWORD_FILE_NAME};
use crate::error::Result;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Storage for VM passwords
#[derive(Debug, Serialize, Deserialize)]
pub struct VMPasswords {
    /// Map of VM name to password
    vms: HashMap<String, String>,
}

impl VMPasswords {
    /// Create a new empty password store
    pub fn new() -> Self {
        Self {
            vms: HashMap::new(),
        }
    }

    /// Load passwords from file or create new if not exists
    pub fn load_or_create(config_dir: &str) -> Result<Self> {
        let password_file = Path::new(config_dir).join(PASSWORD_FILE_NAME);

        if password_file.exists() {
            let content = std::fs::read_to_string(&password_file)?;
            Ok(toml::from_str(&content).unwrap_or_else(|_| Self::new()))
        } else {
            Ok(Self::new())
        }
    }

    /// Save passwords to file
    pub fn save(&self, config_dir: &str) -> Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let password_file = Path::new(config_dir).join(PASSWORD_FILE_NAME);
        std::fs::write(&password_file, toml::to_string_pretty(self)?)?;
        info!("Passwords saved to: {}", password_file.display());
        Ok(())
    }

    /// Add or update a VM password
    pub fn add_vm(&mut self, vm_name: &str, password: &str) {
        self.vms.insert(vm_name.to_string(), password.to_string());
    }

    /// Get the password for a VM
    pub fn get(&self, vm_name: &str) -> Option<&String> {
        self.vms.get(vm_name)
    }

    /// Remove a VM's password
    pub fn remove(&mut self, vm_name: &str) -> Option<String> {
        self.vms.remove(vm_name)
    }

    /// Check if a VM has a stored password
    pub fn contains(&self, vm_name: &str) -> bool {
        self.vms.contains_key(vm_name)
    }

    /// Check if the password store is empty
    pub fn is_empty(&self) -> bool {
        self.vms.is_empty()
    }

    /// Iterate over all VM names and passwords
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.vms.iter()
    }

    /// Get config directory path for the current user
    pub fn get_config_dir() -> Result<String> {
        let home = std::env::var("HOME").map_err(|_| {
            crate::error::ConfigError::Invalid("HOME environment variable not set".to_string())
        })?;
        let path = Path::new(&home).join(CONFIG_DIR_NAME);
        Ok(path.to_string_lossy().to_string())
    }
}

impl Default for VMPasswords {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmpasswords_new() {
        let passwords = VMPasswords::new();
        assert!(passwords.vms.is_empty());
    }

    #[test]
    fn test_vmpasswords_add_and_get() {
        let mut passwords = VMPasswords::new();
        passwords.add_vm("test-vm", "secret123");
        assert_eq!(passwords.get("test-vm"), Some(&"secret123".to_string()));
    }

    #[test]
    fn test_vmpasswords_remove() {
        let mut passwords = VMPasswords::new();
        passwords.add_vm("test-vm", "secret123");
        assert!(passwords.contains("test-vm"));
        passwords.remove("test-vm");
        assert!(!passwords.contains("test-vm"));
    }
}
