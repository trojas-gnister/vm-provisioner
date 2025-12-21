//! Kickstart configuration file generation
//!
//! This module generates Fedora kickstart files for automated VM installation.

use crate::display_bridge::DisplayBridge;
use crate::error::{ProvisioningError, Result};
use crate::xpra_manager::XpraManager;
use log::{debug, info};
use std::fs;

/// Kickstart generation for AppVMProvisioner
pub trait KickstartGeneration {
    fn generate_kickstart_config(&self) -> Result<String>;
}

impl KickstartGeneration for super::AppVMProvisioner {
    /// Generate a kickstart configuration file for automated Fedora installation
    fn generate_kickstart_config(&self) -> Result<String> {
        let kickstart_dir = format!("/tmp/{}-kickstart", self.config.name);

        // Always remove existing kickstart dir to ensure fresh generation
        if std::path::Path::new(&kickstart_dir).exists() {
            debug!("Removing stale kickstart directory: {}", kickstart_dir);
            fs::remove_dir_all(&kickstart_dir)?;
        }

        fs::create_dir_all(&kickstart_dir)?;

        let kickstart_path = format!("{}/kickstart.cfg", kickstart_dir);

        info!("Generating kickstart configuration...");

        // For headless VMs (like gaming VMs with full desktop), skip Xpra entirely
        // and only use the user-provided system packages
        let (packages, display_specific_config) = if self.config.headless {
            debug!("Headless mode: skipping Xpra display bridge");

            // Get SSH public key for remote access (still useful for console SSH)
            let ssh_public_key = XpraManager::get_ssh_public_key()
                .map_err(|e| ProvisioningError::KickstartGeneration(e.to_string()))?;

            // Minimal SSH setup without Xpra-specific configuration
            let ssh_config = format!(
                r#"
# SSH configuration for headless VM
mkdir -p /home/user/.ssh
chmod 700 /home/user/.ssh
cat > /home/user/.ssh/authorized_keys << 'SSH_KEY_EOF'
{ssh_key}
SSH_KEY_EOF
chmod 600 /home/user/.ssh/authorized_keys
chown -R user:user /home/user/.ssh

# Enable SSH server
systemctl enable sshd

# Allow SSH through firewall
firewall-offline-cmd --add-service=ssh || true
"#,
                ssh_key = ssh_public_key
            );

            (self.config.system_packages.join(" "), ssh_config)
        } else {
            // GUI mode: use Xpra display bridge
            let display_bridge: Box<dyn DisplayBridge> = Box::new(
                XpraManager::new(&self.config)
                    .map_err(|e| ProvisioningError::KickstartGeneration(e.to_string()))?,
            );

            let mut base_packages = display_bridge.guest_packages();
            base_packages.extend(self.config.system_packages.clone());

            // Get SSH public key
            let ssh_public_key = XpraManager::get_ssh_public_key()
                .map_err(|e| ProvisioningError::KickstartGeneration(e.to_string()))?;
            let config = display_bridge.kickstart_config(&ssh_public_key);

            (base_packages.join(" "), config)
        };

        // Build Flatpak configuration
        let flatpak_config = self.build_flatpak_config();

        // Build audio configuration
        let audio_config = self.build_audio_config();

        // Build firewall rules
        let firewall_rules = self
            .config
            .firewall_rules
            .iter()
            .map(|rule| format!("iptables -A {}", rule))
            .collect::<Vec<_>>()
            .join("\n");

        // Build custom kickstart section if provided
        let custom_kickstart = self.build_custom_kickstart();

        // Determine bootloader config based on UEFI setting
        // For UEFI: omit --location (anaconda handles ESP automatically)
        // For BIOS: use --location=mbr
        let bootloader_line = if self.config.use_uefi {
            "bootloader --append=\"console=tty0 console=ttyS0,115200n8\"".to_string()
        } else {
            "bootloader --location=mbr --append=\"console=tty0 console=ttyS0,115200n8\"".to_string()
        };

        // Generate the complete kickstart file
        let kickstart_content = format!(
            r#"# Kickstart file for Application VM
# Generated for: {vm_name}

# Installation settings
text
lang en_US.UTF-8
keyboard us
timezone UTC
network --bootproto=dhcp --device=link --activate
rootpw --lock
user --name=user --groups=wheel,video,audio,render,input --password={user_password} --plaintext

# Disk configuration
autopart --type=plain
clearpart --all --initlabel
{bootloader_line}

# Security
selinux --permissive
firewall --enabled

# Package selection (minimal - additional packages installed in %post)
%packages --ignoremissing
@core
@base-x
%end

# Post-installation script
%post --log=/var/log/kickstart-post.log

# Enable comprehensive logging for debugging
set -x
exec > >(tee -a /var/log/kickstart-post-detailed.log) 2>&1
echo "=== Post-installation script started at $(date) ==="

# Install additional packages from full repos (not available in Server ISO)
echo "=== Installing additional packages ==="
dnf install -y {packages}

# Install flatpak packages if specified
{flatpak_config}

# Configure sudo for user
echo "user ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers.d/user

{audio_config}

{display_specific_config}

# Configure firewall rules
{firewall_rules}

# Disable unnecessary services
systemctl disable bluetooth
systemctl disable cups

# Enable serial console for virsh console access
systemctl enable serial-getty@ttyS0.service

# Set hostname
echo "{vm_name}" > /etc/hostname

{custom_kickstart}

echo ""
echo "=== POST-INSTALL SCRIPT COMPLETED ==="
echo "Check logs at /var/log/kickstart-post.log and /var/log/kickstart-post-detailed.log"

# Ensure all home directory files are owned by user
chown -R user:user /home/user

# Final cleanup
dnf clean all

%end

# Reboot after installation
reboot"#,
            vm_name = self.config.name,
            user_password = self.config.user_password,
            bootloader_line = bootloader_line,
            packages = packages,
            flatpak_config = flatpak_config,
            audio_config = audio_config,
            display_specific_config = display_specific_config,
            firewall_rules = firewall_rules,
            custom_kickstart = custom_kickstart
        );

        fs::write(&kickstart_path, kickstart_content)?;
        debug!("Kickstart file written to: {}", kickstart_path);
        Ok(kickstart_path)
    }
}

impl super::AppVMProvisioner {
    /// Build Flatpak configuration section for kickstart
    fn build_flatpak_config(&self) -> String {
        if self.config.headless || self.config.flatpak_packages.is_empty() {
            return String::new();
        }

        let mut config = String::from(
            r#"
# Install and configure Flatpak
dnf install -y flatpak

# Add Flathub repository
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo

# Install Flatpak packages
"#,
        );

        for package in &self.config.flatpak_packages {
            config.push_str(&format!("flatpak install -y flathub {}\n", package));
        }

        // Grant device permissions if requested
        if self.config.grant_device_access {
            config.push_str("\n# Grant device access to Flatpak apps\n");
            for package in &self.config.flatpak_packages {
                config.push_str(&format!("flatpak override --user --device=all {}\n", package));
            }
        }

        config.push_str("\n# Verify installations\nflatpak list\n");
        config
    }

    /// Build audio configuration section for kickstart
    fn build_audio_config(&self) -> String {
        if !self.config.enable_audio {
            return String::new();
        }

        r#"
# Prepare helper script to start PipeWire stack within user session
mkdir -p /home/user/.local/bin
cat > /home/user/.local/bin/start-pipewire.sh <<'EOF'
#!/bin/bash
if ! pgrep -u "$UID" -x pipewire >/dev/null; then
    pipewire &
fi
if ! pgrep -u "$UID" -x pipewire-pulse >/dev/null; then
    pipewire-pulse &
fi
if ! pgrep -u "$UID" -x wireplumber >/dev/null; then
    wireplumber &
fi
EOF
chmod +x /home/user/.local/bin/start-pipewire.sh
chown -R user:user /home/user/.local/bin"#
            .to_string()
    }

    /// Build custom kickstart section if provided
    fn build_custom_kickstart(&self) -> String {
        match &self.config.custom_kickstart {
            Some(script) => {
                format!(
                    r#"
# Custom kickstart additions (injected by library consumer)
{}"#,
                    script
                )
            }
            None => String::new(),
        }
    }
}
