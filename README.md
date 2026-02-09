# VM Provisioner

**Run applications in isolated NixOS VMs with hardware-accelerated Vulkan.** Each app gets its own virtual machine built from a declarative NixOS configuration, with Venus virtio-gpu providing near-native GPU performance — no GPU passthrough required.

## Why Use This?

| Traditional VMs | VM Provisioner |
|-----------------|----------------|
| Full desktop in a window | Lightweight, purpose-built NixOS VMs |
| Manual guest OS install | Declarative NixOS provisioning via `nixos-generators` |
| Static configuration | Dynamic package installation (system + Flatpak) |
| Shared attack surface | One VM per app = isolation |

**Use cases:**
- Run untrusted software without risking your system
- Isolate browsers for banking, social media, or sketchy websites
- Test software in a clean environment
- Separate work and personal applications

## Quick Start

### 1. Prerequisites

**Host requirements:**
- libvirt + QEMU/KVM
- Nix package manager (for `nixos-generators`)
- AMD or Intel GPU (for Venus Vulkan acceleration)

```bash
# Install libvirt and QEMU
# Fedora:
sudo dnf install libvirt qemu-kvm virt-install
# Debian/Ubuntu:
sudo apt install libvirt-daemon-system qemu-kvm virtinst
# Arch:
sudo pacman -S libvirt qemu-full virt-install

# Enable libvirtd
sudo systemctl enable --now libvirtd
sudo usermod -aG libvirt $USER  # Log out and back in after this

# Install nixos-generators (requires Nix package manager)
# If you don't have Nix: https://nixos.org/download
nix-env -iA nixpkgs.nixos-generators
```

### 2. Install VM Provisioner

```bash
git clone https://github.com/trojas-gnister/vm-provisioner.git
cd vm-provisioner
cargo build --release
sudo install -m 755 target/release/vm-provisioner /usr/local/bin/
```

### 3. Create Your First VM

**Using the TUI (recommended):**
```bash
# Launch the interactive terminal UI
vm-provisioner
```

The TUI lets you create, start, stop, and destroy VMs with a visual interface.

**Using the CLI:**
```bash
# Create a browser VM (first run builds a NixOS image, ~5-10 minutes)
vm-provisioner create --flatpak io.gitlab.librewolf-community --name browser

# Start the VM
vm-provisioner start browser

# Check VM status and IP
vm-provisioner list

# Connect via SSH or serial console
vm-provisioner console browser
```

## Command Reference

### TUI

Running `vm-provisioner` with no subcommand launches the interactive terminal UI (built with ratatui). The TUI provides:

- VM dashboard with status overview
- VM creation form with all configuration options
- Start/stop/destroy operations
- Live provisioning log viewer
- VM detail view with configuration summary

### CLI Commands

| Command | Description |
|---------|-------------|
| `create` | Provision a new VM with specified packages |
| `start <vm>` | Boot a stopped VM |
| `stop <vm>` | Graceful shutdown |
| `destroy <vm> -y` | Permanently delete VM and disk image |
| `list` | Show all VMs with status and IP addresses |
| `passwords` | Display VM user credentials |
| `console <vm>` | Attach to VM serial console (Ctrl+] to exit) |
| `usb-attach <vm> <device>` | Hot-attach a USB device to a running VM |
| `usb-detach <vm> <device>` | Detach a USB device from a running VM |

Run `vm-provisioner --help` or `vm-provisioner <command> --help` for all options.

## Creating VMs

### Basic Examples

```bash
# Flatpak application (recommended for GUI apps)
vm-provisioner create --flatpak org.mozilla.firefox --name firefox

# System packages
vm-provisioner create --system gimp inkscape --name graphics

# Mixed: system packages + Flatpaks
vm-provisioner create --system git nodejs --flatpak com.visualstudio.code --name dev

# Headless VM (no GUI, SSH/console access only)
vm-provisioner create --system python3 git --headless --name cli-tools
```

### Resource Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--memory` | 2048 | RAM in MB |
| `--vcpus` | 2 | Virtual CPU cores |
| `--disk` | 20 | Disk size in GB |

```bash
# High-resource VM for development
vm-provisioner create \
  --flatpak com.visualstudio.code \
  --memory 8192 \
  --vcpus 4 \
  --disk 50 \
  --name dev-workstation
```

## GPU Virtualization (Venus Vulkan)

GUI VMs automatically use **Venus**, a virtio-GPU Vulkan driver that forwards Vulkan calls from the guest to the host GPU — no GPU passthrough required.

### How It Works

1. **Host-side**: vm-provisioner detects GPU render nodes via sysfs, selects the best GPU (AMD > Intel; NVIDIA not supported by Venus), and configures QEMU with `venus=true`, `blob=true`, and the correct Vulkan ICD
2. **Guest-side**: The generated NixOS configuration installs Mesa, sets `MESA_LOADER_DRIVER_OVERRIDE=virtio_gpu`, and points `VK_DRIVER_FILES` to the Venus ICD

### Requirements

| Component | Minimum Version |
|-----------|----------------|
| QEMU | >= 9.2 |
| Host kernel | >= 6.6 |
| Guest kernel | >= 6.13 (auto-configured via `linuxPackages_latest`) |
| Mesa (guest) | >= 25.x (Venus stable since Mesa 25.x) |
| Host GPU | AMD (RADV) or Intel (ANV) — NVIDIA lacks Venus support |

### Current Limitations

- **D3D12 Feature Level 12_2 not yet supported**: Games requiring FL 12_2 (e.g. FF7 Rebirth) will fail to create a D3D12 device. Mesa 26.0 (expected late February 2026) adds mesh shader support to Venus, which is required for FL 12_2 via vkd3d-proton.
- **Performance overhead**: Venus adds a serialization layer between guest and host Vulkan. Lightweight workloads (emulators, older games) run well; demanding AAA titles may experience lower framerates compared to bare metal or VFIO passthrough.
- **NVIDIA GPUs**: Venus requires an open-source Vulkan driver (RADV or ANV). NVIDIA's proprietary driver is not supported.

### Verified Working

- Steam (native Linux client)
- Games requiring D3D12 Feature Level <= 12_1 via Proton/vkd3d-proton
- Vulkan-native applications (e.g. `vulkaninfo`, `vkcube`)
- Console emulators that require Vulkan 1.2+

### Waiting On

- **Mesa 26.0** (RC3 released Feb 5 2026, stable expected late Feb 2026): Adds Venus mesh shader support, enabling D3D12 FL 12_2. This will unlock more modern Proton games. Track the [Mesa release calendar](https://docs.mesa3d.org/release-calendar.html).

## Hardware Passthrough

### USB Devices

Pass USB devices (webcams, audio interfaces, hardware keys) directly to VMs:

```bash
# Find your device
lsusb
# Bus 001 Device 005: ID 046d:0825 Logitech HD Webcam C270

# Create VM with USB passthrough
vm-provisioner create --flatpak org.mozilla.firefox --usb 046d:0825 --name webcam-vm
```

**Hot-plug mode** returns devices to the host when the VM stops:
```bash
vm-provisioner create --flatpak ... --usb 046d:0825 --usb-hotplug --name vm
```

> **Note:** For Flatpak apps to access USB devices, add `--grant-device-access`.

### Shared Folders

Share host directories with VMs using virtiofs:

```bash
vm-provisioner create \
  --flatpak com.visualstudio.code \
  --share /home/user/projects:/mnt/projects \
  --name dev

# Read-only mode (applies to all --share paths)
vm-provisioner create ... --share /path:/mnt/path --share-readonly
```

Inside the VM, shared folders mount automatically at the specified guest path (e.g., `/mnt/projects`).

## Networking Options

### Default (NAT)

VMs get IPs on a private network (192.168.122.x) and can access the internet. Find a VM's IP with:

```bash
vm-provisioner list
# or
virsh domifaddr <vm-name>
```

### Bridged Networking

Give VMs an IP directly on your LAN (useful for accessing from other devices):

```bash
# One-time setup: create a network bridge
# WARNING: This briefly disconnects your network
nmcli device status  # Find your interface (e.g., enp0s31f6)
sudo nmcli connection add type bridge ifname br0 con-name br0
sudo nmcli connection add type bridge-slave ifname enp0s31f6 master br0
sudo nmcli connection up br0

# Create VM on the bridge
vm-provisioner create --flatpak ... --network-bridge br0 --name lan-accessible
```

### Network-Disabled (Airgapped)

Maximum isolation — VM has no network interface:

```bash
vm-provisioner create --system git --headless --no-network --name airgapped
```

Access via `virsh console` only. Flatpak installation requires networking, so `--no-network` is best suited for headless VMs with system packages only.

## Configuration Files

Configs are stored in `~/.config/vm-provisioner/`:

| File | Contents |
|------|----------|
| `<vm-name>.toml` | VM configuration (memory, packages, network mode, etc.) |
| `vm-passwords.toml` | Credentials for all VMs |

You can edit `.toml` files directly, but changes only take effect after recreating the VM.

## Troubleshooting

### Common Issues

**Permission denied / Cannot connect to libvirt**
```bash
# Add yourself to the libvirt group
sudo usermod -aG libvirt $USER
# Log out and back in, or run: newgrp libvirt
```

**VM won't start: "network 'default' is not active"**
```bash
sudo virsh net-start default
sudo virsh net-autostart default
```

**Venus Vulkan: "failed to initialize venus renderer"**

Venus requires a working Vulkan driver on the **host**. Install the appropriate driver:

```bash
# AMD (RADV)
sudo pacman -S vulkan-radeon    # Arch
sudo dnf install vulkan-loader mesa-vulkan-drivers  # Fedora

# Intel
sudo pacman -S vulkan-intel     # Arch

# Verify with:
vulkaninfo --summary  # Requires vulkan-tools
```

After installing, restart the VM. Verify inside the guest with `vulkaninfo --summary` — you should see a Venus device instead of llvmpipe.

Venus also requires QEMU's seccomp sandbox to allow process spawning (virglrenderer runs Venus in an isolated process). If Venus still fails after installing Vulkan drivers, disable the sandbox in `/etc/libvirt/qemu.conf`:

```bash
# Edit /etc/libvirt/qemu.conf
seccomp_sandbox = 0

# Then restart libvirtd
sudo systemctl restart libvirtd
```

> **Note:** Venus requires QEMU >= 9.2, Linux kernel >= 6.13 on the guest, virglrenderer with Venus support, and a working host Vulkan driver.

**NixOS image build fails**

Ensure `nixos-generators` is installed and the Nix daemon is running:

```bash
nixos-generate --help   # Should show usage
systemctl status nix-daemon  # Should be active (if using multi-user Nix)
```

If the build fails with permission errors, ensure your user is in the `nix-users` group or is a trusted user in `/etc/nix/nix.conf`.

**No audio in VM**

Audio uses PipeWire inside the NixOS guest (auto-configured for GUI VMs). For audio input devices (microphones), use USB passthrough:

```bash
vm-provisioner create --usb <audio-device-id> --usb-hotplug ...
```

### Getting Help

```bash
vm-provisioner --help
vm-provisioner create --help
```

## Development

```bash
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run tests
cargo fmt             # Format code
```

### Architecture

- `src/main.rs` — Entrypoint, delegates to CLI or TUI
- `src/cli/` — CLI command handlers
  - `mod.rs` — Clap CLI definition and dispatch; launches TUI when no subcommand given
  - `create.rs` — VM creation: validation, config building, post-provisioning
  - `vm_ops.rs` — start, stop, destroy, list, passwords, console
  - `usb.rs` — USB attach/detach command handlers
- `src/tui/` — Interactive terminal UI (ratatui)
  - `app.rs` — Application state, VM list management, provisioning orchestration
  - `ui.rs` — Screen rendering (dashboard, detail, create form, provisioning log)
  - `handler.rs` — Keyboard input handling
  - `event.rs` — Event loop
  - `logger.rs` — Log capture for TUI display
- `src/nixos/` — NixOS guest configuration
  - `config_gen.rs` — Generates `configuration.nix` with Venus, PipeWire, Flatpak, etc.
  - `image_builder.rs` — Builds qcow2 images via `nixos-generators`
  - `packages.rs` — Package name mapping (Fedora names -> nixpkgs equivalents)
- `src/config.rs` — VM configuration structures and builder
- `src/provisioner/` — VM lifecycle and hardware management
  - `installation.rs` — VM provisioning orchestration: NixOS build, virt-install import, Venus/SPICE setup
  - `lifecycle.rs` — Start/stop/destroy operations
  - `device_detection.rs` — GPU render node and USB device detection
  - `usb.rs` — USB passthrough
  - `network.rs` — Network interface management
- `src/virsh.rs` — Centralized libvirt/virsh command helpers
- `src/libvirt_xml.rs` — XML generation for libvirt devices (USB, network)
- `src/constants.rs` — Static configuration values, GPU vendor IDs, NixOS channel
- `src/passwords.rs` — VM credential management
- `src/error.rs` — Typed error hierarchy (`VmProvisionerError`, `ProvisioningError`, etc.)
- `src/validation.rs` — USB and shared folder validation helpers

## Library Usage

vm-provisioner can be used as a Rust library in your own projects:

### Add Dependency

```toml
[dependencies]
vm-provisioner = { git = "https://github.com/trojas-gnister/vm-provisioner" }
```

### Example

```rust
use vm_provisioner::{AppVMConfigBuilder, AppVMProvisioner, Installation};

fn main() -> vm_provisioner::Result<()> {
    let config = AppVMConfigBuilder::new("my-vm")
        .memory_mb(2048)
        .vcpus(2)
        .add_system_package("nginx")
        .headless(true)
        .build()?;

    let provisioner = AppVMProvisioner::new(config);
    provisioner.provision_vm()?;

    Ok(())
}
```

### Custom NixOS Configuration

Inject custom Nix configuration into the generated `configuration.nix`:

```rust
let config = AppVMConfigBuilder::new("custom-vm")
    .add_system_package("nginx")
    .custom_nix_config(r#"
        services.nginx.enable = true;
        services.nginx.virtualHosts."localhost" = {
            root = "/var/www";
        };
    "#)
    .build()?;
```

### GPU Detection

Query host GPU render nodes for Venus Vulkan support:

```rust
use vm_provisioner::{detect_gpu_render_nodes, select_gpu_for_venus, GpuVendor};

let nodes = detect_gpu_render_nodes();
if let Some(gpu) = select_gpu_for_venus(&nodes) {
    println!("Best GPU for Venus: {:?} at {}", gpu.vendor, gpu.pci_slot);
    println!("Render node: {}", gpu.render_node);
    println!("Stable path: {}", gpu.by_path);
}
```

### Available Exports

| Type | Description |
|------|-------------|
| `AppVMConfigBuilder` | Builder pattern for VM configuration |
| `AppVMProvisioner` | Main provisioner struct |
| `Installation`, `Lifecycle` | Traits for VM operations |
| `UsbPassthrough` | Trait for USB device passthrough |
| `detect_usb_device()` | Detect USB device by vendor:product |
| `detect_gpu_render_nodes()` | Detect GPU render nodes via sysfs |
| `select_gpu_for_venus()` | Select best GPU for Venus Vulkan |
| `GpuVendor`, `GpuRenderNode` | GPU detection types |
| `validate_usb_device()` | Validate USB device config format |
| `validate_shared_folder()` | Validate shared folder config format |

## License

MIT
