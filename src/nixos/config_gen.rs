//! NixOS configuration generator
//!
//! Generates a complete `configuration.nix` from an `AppVMConfig`, then
//! validates the Nix expression with `rnix`.

use crate::config::{AppVMConfig, GraphicsBackend, NetworkMode};
use crate::error::{DisplayError, ProvisioningError, Result};
use crate::nixos::packages;
use log::debug;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Generate a complete `configuration.nix` as a String from the given VM config.
///
/// The generated config includes base system, SSH, packages, optional GUI
/// services, flatpak, shared folders, networking, vsock, firewall, and user setup.
///
/// The output is validated with `rnix::Root::parse()` before returning.
pub fn generate_configuration_nix(config: &AppVMConfig) -> Result<String> {
    let ssh_public_key = get_ssh_public_key()
        .map_err(|e| ProvisioningError::NixConfigInvalid(format!("SSH key error: {}", e)))?;

    let mut nix = String::with_capacity(4096);

    nix.push_str("{ config, pkgs, lib, ... }:\n\n{\n");

    // Imports
    nix.push_str("  imports = [ ];\n\n");

    // Allow unfree packages (e.g. Steam, NVIDIA drivers)
    nix.push_str("  nixpkgs.config.allowUnfree = true;\n\n");

    // Boot loader
    nix.push_str("  boot.loader.grub.enable = true;\n");
    nix.push_str("  boot.loader.grub.device = \"/dev/vda\";\n\n");

    // Use latest kernel for VirtioGpu (kernel >= 6.13 required for Venus support)
    if matches!(config.graphics_backend, GraphicsBackend::VirtioGpu) && !config.headless {
        nix.push_str("  boot.kernelPackages = pkgs.linuxPackages_latest;\n\n");
    }

    // Kernel modules
    if config.enable_vsock {
        nix.push_str("  boot.kernelModules = [ \"vhost_vsock\" ];\n\n");
    }

    // Networking
    nix.push_str(&format!(
        "  networking.hostName = \"{}\";\n",
        config.name
    ));

    match &config.network_mode {
        NetworkMode::None => {
            nix.push_str("  networking.useDHCP = false;\n");
            nix.push_str("  networking.interfaces = { };\n");
        }
        NetworkMode::Nat | NetworkMode::Bridge(_) => {
            nix.push_str("  networking.useDHCP = true;\n");
        }
    }

    // Firewall
    nix.push_str("  networking.firewall.allowedTCPPorts = [ 22 ];\n\n");

    // Users
    nix.push_str("  users.users.user = {\n");
    nix.push_str("    isNormalUser = true;\n");
    nix.push_str("    extraGroups = [ \"wheel\" \"video\" \"audio\" \"render\" \"input\" ];\n");
    nix.push_str(&format!(
        "    hashedPassword = \"{}\";\n",
        hash_password(&config.user_password)
    ));
    nix.push_str(&format!(
        "    openssh.authorizedKeys.keys = [ \"{}\" ];\n",
        ssh_public_key.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    nix.push_str("  };\n\n");

    // Sudo
    nix.push_str("  security.sudo.wheelNeedsPassword = false;\n\n");

    // SSH
    nix.push_str("  services.openssh.enable = true;\n\n");

    // QEMU guest agent
    nix.push_str("  services.qemuGuest.enable = true;\n\n");

    // Packages
    let mut nix_packages: Vec<String> = Vec::new();
    for pkg in &config.system_packages {
        if let Some(nix_name) = packages::map_package(pkg) {
            nix_packages.push(nix_name.to_string());
        }
    }
    // Add Venus/Vulkan packages for VirtioGpu
    if matches!(config.graphics_backend, GraphicsBackend::VirtioGpu) && !config.headless {
        if !nix_packages.contains(&"mesa".to_string()) {
            nix_packages.push("mesa".to_string());
        }
        if !nix_packages.contains(&"vulkan-loader".to_string()) {
            nix_packages.push("vulkan-loader".to_string());
        }
        if !nix_packages.contains(&"vulkan-tools".to_string()) {
            nix_packages.push("vulkan-tools".to_string());
        }
    }
    nix_packages.dedup();

    nix.push_str("  environment.systemPackages = with pkgs; [\n");
    for pkg in &nix_packages {
        nix.push_str(&format!("    {}\n", pkg));
    }
    nix.push_str("  ];\n\n");

    // Hardware graphics and Venus configuration for VirtioGpu
    if matches!(config.graphics_backend, GraphicsBackend::VirtioGpu) && !config.headless {
        nix.push_str("  hardware.graphics = {\n");
        nix.push_str("    enable = true;\n");
        nix.push_str("    enable32Bit = true;\n");
        nix.push_str("    extraPackages = with pkgs; [ mesa vulkan-loader ];\n");
        nix.push_str("    extraPackages32 = with pkgs.pkgsi686Linux; [ mesa vulkan-loader ];\n");
        nix.push_str("  };\n\n");

        nix.push_str("  environment.variables = {\n");
        nix.push_str("    MESA_LOADER_DRIVER_OVERRIDE = \"virtio_gpu\";\n");
        nix.push_str("    VK_DRIVER_FILES = \"${pkgs.mesa.drivers}/share/vulkan/icd.d/virtio_icd.x86_64.json\";\n");
        nix.push_str("  };\n\n");
    }

    // PipeWire audio (for GUI VMs)
    if config.enable_audio && !config.headless {
        nix.push_str("  services.pipewire = {\n");
        nix.push_str("    enable = true;\n");
        nix.push_str("    alsa.enable = true;\n");
        nix.push_str("    pulse.enable = true;\n");
        nix.push_str("  };\n\n");
    }

    // Flatpak
    if !config.flatpak_packages.is_empty() && !config.headless {
        nix.push_str("  services.flatpak.enable = true;\n\n");

        // Systemd oneshot services to install flatpak packages on first boot
        for pkg in &config.flatpak_packages {
            let service_name = pkg.replace('.', "-").to_lowercase();
            nix.push_str(&format!(
                "  systemd.services.\"flatpak-install-{}\" = {{\n",
                service_name
            ));
            nix.push_str("    description = \"Install flatpak package\";\n");
            nix.push_str("    wantedBy = [ \"multi-user.target\" ];\n");
            nix.push_str("    after = [ \"network-online.target\" \"flatpak-system-helper.service\" ];\n");
            nix.push_str("    wants = [ \"network-online.target\" ];\n");
            nix.push_str("    serviceConfig = {\n");
            nix.push_str("      Type = \"oneshot\";\n");
            nix.push_str("      RemainAfterExit = true;\n");
            nix.push_str(&format!(
                "      ExecStart = \"${{pkgs.flatpak}}/bin/flatpak install -y flathub {}\";\n",
                pkg
            ));
            nix.push_str("    };\n");
            nix.push_str("  };\n\n");
        }
    }

    // Shared folders (virtiofs)
    for folder in &config.shared_folders {
        let options = if folder.readonly {
            "[ \"defaults\" \"nofail\" \"ro\" ]"
        } else {
            "[ \"defaults\" \"nofail\" ]"
        };
        nix.push_str(&format!(
            "  fileSystems.\"{}\" = {{\n",
            folder.guest_path
        ));
        nix.push_str(&format!("    device = \"{}\";\n", folder.tag));
        nix.push_str("    fsType = \"virtiofs\";\n");
        nix.push_str(&format!("    options = {};\n", options));
        nix.push_str("  };\n\n");
    }

    // Auto-login for GUI VMs
    if config.enable_auto_login && !config.headless {
        nix.push_str("  services.getty.autologinUser = \"user\";\n\n");
    }

    // NixOS state version
    nix.push_str("  system.stateVersion = \"24.11\";\n");

    nix.push_str("}\n");

    // Validate with rnix
    validate_nix_syntax(&nix)?;

    debug!("Generated NixOS configuration ({} bytes)", nix.len());
    Ok(nix)
}

/// Get the user's SSH public key, generating one if necessary.
fn get_ssh_public_key() -> Result<String> {
    let home = std::env::var("HOME")?;
    let ssh_dir = format!("{}/.ssh", home);

    // Check for existing keys in priority order
    let key_types = ["id_ed25519", "id_rsa", "id_ecdsa"];
    for key_type in &key_types {
        let pub_key_path = format!("{}/{}.pub", ssh_dir, key_type);
        if Path::new(&pub_key_path).exists() {
            return Ok(fs::read_to_string(&pub_key_path)?.trim().to_string());
        }
    }

    // No key exists, generate a new ed25519 key
    log::info!("No SSH key found, generating new ed25519 key...");
    fs::create_dir_all(&ssh_dir)?;

    let key_path = format!("{}/id_ed25519", ssh_dir);
    let status = Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-f", &key_path, "-N", "", "-q"])
        .status()?;

    if !status.success() {
        return Err(
            DisplayError::ConnectionFailed("Failed to generate SSH key".to_string()).into(),
        );
    }

    let pub_key_path = format!("{}.pub", key_path);
    Ok(fs::read_to_string(&pub_key_path)?.trim().to_string())
}

/// Validate that a Nix expression parses without errors using rnix.
fn validate_nix_syntax(nix_source: &str) -> Result<()> {
    let parse = rnix::Root::parse(nix_source);
    let errors = parse.errors();
    if !errors.is_empty() {
        let msg = errors
            .iter()
            .map(|e: &rnix::parser::ParseError| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(
            ProvisioningError::NixConfigInvalid(format!("Nix syntax errors: {}", msg)).into(),
        );
    }
    Ok(())
}

/// Hash a plaintext password for use in NixOS `hashedPassword`.
///
/// Uses SHA-512 crypt format compatible with `/etc/shadow`.
fn hash_password(password: &str) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let salt: String = (0..16)
        .map(|_| {
            const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789./";
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    // SHA-512 crypt: $6$salt$hash
    // We use a simple approach — shell out to mkpasswd or openssl if available,
    // otherwise return a placeholder that NixOS can use.
    // For robustness, we generate inline using the same format.
    let output = std::process::Command::new("mkpasswd")
        .args(["--method=sha-512", "--salt", &salt, password])
        .output();

    match output {
        Ok(ref o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => {
            // Fallback: try openssl
            let output = std::process::Command::new("openssl")
                .args(["passwd", "-6", "-salt", &salt, password])
                .output();
            match output {
                Ok(ref o) if o.status.success() => {
                    String::from_utf8_lossy(&o.stdout).trim().to_string()
                }
                _ => {
                    // Last resort: return a known format that will need to be changed on first login
                    format!("$6${}$placeholder", salt)
                }
            }
        }
    }
}
