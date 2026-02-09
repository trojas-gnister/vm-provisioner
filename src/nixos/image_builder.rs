//! Build qcow2 images from NixOS configuration
//!
//! Shells out to `nixos-generate` from the `nixos-generators` package
//! to produce a bootable qcow2 disk image.

use crate::error::{ProvisioningError, Result};
use log::{debug, info};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build a qcow2 image from a NixOS configuration string.
///
/// 1. Writes `configuration.nix` to a temp directory
/// 2. Runs `nixos-generate -f qcow2 -c <path>`
/// 3. Moves the resulting image to `/var/lib/libvirt/images/{vm_name}.qcow2`
pub fn build_image(nix_config: &str, vm_name: &str, vm_dir: &str, disk_size_gb: u64) -> Result<PathBuf> {
    let tmp_dir = format!("/tmp/{}-nixos", vm_name);
    fs::create_dir_all(&tmp_dir)?;

    let config_path = format!("{}/configuration.nix", tmp_dir);
    fs::write(&config_path, nix_config)?;
    debug!("Wrote NixOS configuration to {}", config_path);

    // Write a custom format file that uses "auto" disk sizing (just enough for
    // the closure + headroom). We resize the qcow2 to the user's desired size
    // afterward — the guest has growPartition + autoResize so it expands on boot.
    let format_path = format!("{}/qcow-custom.nix", tmp_dir);
    fs::write(&format_path,
        r#"{ config, lib, pkgs, modulesPath, ... }: {
  imports = [
    "${toString modulesPath}/profiles/qemu-guest.nix"
  ];

  fileSystems."/" = {
    device = "/dev/disk/by-label/nixos";
    autoResize = true;
    fsType = "ext4";
  };

  boot.growPartition = true;
  boot.kernelParams = ["console=ttyS0"];
  boot.loader.grub.device =
    if (pkgs.stdenv.system == "x86_64-linux")
    then (lib.mkDefault "/dev/vda")
    else (lib.mkDefault "nodev");

  boot.loader.grub.efiSupport = lib.mkIf (pkgs.stdenv.system != "x86_64-linux") (lib.mkDefault true);
  boot.loader.grub.efiInstallAsRemovable = lib.mkIf (pkgs.stdenv.system != "x86_64-linux") (lib.mkDefault true);
  boot.loader.timeout = 0;

  system.build.qcow = import "${toString modulesPath}/../lib/make-disk-image.nix" {
    inherit lib config pkgs;
    diskSize = "auto";
    additionalSpace = "2048M";
    format = "qcow2";
    partitionTableType = "hybrid";
  };

  formatAttr = "qcow";
  fileExtension = ".qcow2";
}"#
    )?;
    debug!("Wrote custom qcow format to {}", format_path);

    info!("Building NixOS qcow2 image (this may take a few minutes on first build)...");

    let output = Command::new("nixos-generate")
        .args(["--format-path", &format_path, "-c", &config_path])
        .env("NIXPKGS_ALLOW_UNFREE", "1")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(ProvisioningError::NixBuildFailed(format!(
            "nixos-generate failed: {}",
            stderr
        ))
        .into());
    }

    // nixos-generate prints the output path on stdout
    let result_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let result_path = Path::new(&result_path);

    if !result_path.exists() {
        return Err(ProvisioningError::NixBuildFailed(
            "nixos-generate succeeded but output image not found".to_string(),
        )
        .into());
    }

    // Ensure VM directory exists
    Command::new("sudo")
        .args(["mkdir", "-p", vm_dir])
        .status()?;

    let dest = PathBuf::from(vm_dir).join(format!("{}.qcow2", vm_name));

    // Remove existing disk if present
    Command::new("sudo")
        .args(["rm", "-f", &dest.to_string_lossy()])
        .status()?;

    // Copy image to libvirt images directory (cp instead of mv since Nix store
    // files are read-only hardlinks that can't simply be moved and chmod'd)
    let status = Command::new("sudo")
        .args(["cp", "--no-preserve=mode,ownership", &result_path.to_string_lossy(), &dest.to_string_lossy()])
        .status()?;

    if !status.success() {
        return Err(ProvisioningError::NixBuildFailed(
            "Failed to copy qcow2 image to libvirt directory".to_string(),
        )
        .into());
    }

    // Ensure libvirt can access the image
    Command::new("sudo")
        .args(["chmod", "0644", &dest.to_string_lossy()])
        .status()?;

    // Resize qcow2 to the user's desired disk size; the guest will grow the
    // partition and filesystem automatically on first boot.
    info!("Resizing image to {} GB...", disk_size_gb);
    let resize_status = Command::new("sudo")
        .args(["qemu-img", "resize", &dest.to_string_lossy(), &format!("{}G", disk_size_gb)])
        .status()?;
    if !resize_status.success() {
        return Err(ProvisioningError::NixBuildFailed(
            "Failed to resize qcow2 image".to_string(),
        )
        .into());
    }

    info!("NixOS image built: {}", dest.display());

    // Clean up temp dir
    let _ = fs::remove_dir_all(&tmp_dir);

    Ok(dest)
}
