# VM Provisioner

**Run desktop applications in isolated Fedora VMs with seamless window integration.** Each app gets its own virtual machine, but windows appear natively on your desktop—no visible VM boundary.

## Why Use This?

| Traditional VMs | VM Provisioner |
|-----------------|----------------|
| Full desktop in a window | Individual app windows on your desktop |
| Manual guest OS install | Automated Fedora provisioning |
| Static configuration | Dynamic package installation |
| Shared attack surface | One VM per app = isolation |

**Use cases:**
- Run untrusted software without risking your system
- Isolate browsers for banking, social media, or sketchy websites
- Test software in a clean environment
- Separate work and personal applications

## Quick Start

### 1. Prerequisites

**Fedora:**
```bash
sudo dnf install libvirt qemu-kvm virt-install xpra
sudo systemctl enable --now libvirtd
sudo usermod -aG libvirt $USER  # Log out and back in after this
```

**Debian/Ubuntu:**
```bash
sudo apt install libvirt-daemon-system qemu-kvm virtinst xpra
sudo systemctl enable --now libvirtd
sudo usermod -aG libvirt $USER  # Log out and back in after this
```

**Arch Linux:**
```bash
sudo pacman -S libvirt qemu-full virt-install xpra
sudo systemctl enable --now libvirtd
sudo usermod -aG libvirt $USER  # Log out and back in after this
```

### 2. Install VM Provisioner

```bash
git clone https://github.com/trojas-gnister/vm-provisioner.git
cd vm-provisioner
cargo build --release
sudo install -m 755 target/release/vm-provisioner /usr/local/bin/
```

### 3. Create Your First VM

```bash
# Create a browser VM (first run downloads Fedora ISO, ~5-10 minutes)
vm-provisioner create --flatpak io.gitlab.librewolf-community --name browser

# Start the VM
vm-provisioner start browser

# Wait ~30 seconds for VM to boot, then generate shortcuts
vm-provisioner generate-shortcuts browser

# Launch the browser
vm-provisioner launch browser "flatpak run io.gitlab.librewolf-community"
```

The browser window appears on your desktop just like a native app.

## Command Reference

### VM Lifecycle

| Command | Description |
|---------|-------------|
| `create` | Provision a new VM with specified packages |
| `start <vm>` | Boot a stopped VM |
| `stop <vm>` | Graceful shutdown (saves state) |
| `destroy <vm> -y` | Permanently delete VM and disk image |
| `list` | Show all VMs with status and IP addresses |

### Application Management

| Command | Description |
|---------|-------------|
| `launch <vm> "command"` | Run a command in the VM via Xpra |
| `generate-shortcuts <vm>` | Create .desktop files in `~/.local/share/applications/` |
| `apps <vm>` | List launchable applications |

### Utilities

| Command | Description |
|---------|-------------|
| `passwords` | Display VM user credentials |
| `console <vm>` | Attach to VM serial console (Ctrl+] to exit) |

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

### PCI/GPU Passthrough

Pass entire PCI devices (GPUs, network cards, etc.) to VMs.

**Requirements:**
1. Enable IOMMU in BIOS
2. Add kernel parameter: `intel_iommu=on` (Intel) or `amd_iommu=on` (AMD)
3. Reboot

```bash
# Find device address
lspci -nn -D | grep -i vga
# 0000:01:00.0 VGA compatible controller [0300]: NVIDIA...

vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --pci 0000:01:00.0 \
  --pci-hotplug \
  --name gpu-vm
```

> **Warning:** Passing your only GPU will make the host display unusable. Use `--pci-hotplug` to return the device when the VM stops.

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

Maximum isolation—VM has no network interface:

```bash
vm-provisioner create --flatpak ... --no-network --name airgapped
```

Display forwarding uses virtio-vsock (host-guest communication channel) instead of SSH over TCP. Requires `socat` on both host and guest.

## Web-Based Remote Access

Access VMs from any browser using Selkies-GStreamer (WebRTC streaming):

```bash
vm-provisioner create \
  --flatpak io.gitlab.librewolf-community \
  --web-port 8080 \
  --name remote-browser
```

After booting, access at `http://<vm-ip>:8080/`. Login with:
- Username: `user`
- Password: from `vm-provisioner passwords`

Useful for accessing VMs from mobile devices or machines without Xpra installed.

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

**Xpra drops when VPN connects**

Your VPN may block local network traffic. Either:
- Allow 192.168.122.0/24 in VPN split-tunnel settings, or
- Use `--network-bridge` for LAN networking

**No audio in VM**

Check PulseAudio is running on the host:
```bash
pactl info
```

For low-latency audio input (microphones), use USB passthrough instead of network audio:
```bash
vm-provisioner create --usb <audio-device-id> --usb-hotplug ...
```

**generate-shortcuts says VM not found**

The VM must be running. After `vm-provisioner start`, wait ~30 seconds for the network to come up.

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

- `src/main.rs` — CLI interface and command routing
- `src/config.rs` — VM configuration structures
- `src/provisioner/` — VM lifecycle (create, start, stop, destroy)
- `src/xpra_manager.rs` — Xpra display bridge implementation
- `src/templates/` — Kickstart shell script templates

To add a new display protocol, implement the `DisplayBridge` trait in `src/display_bridge.rs`.

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

### Custom Kickstart Scripts

Inject custom setup scripts into the VM provisioning process:

```rust
let config = AppVMConfigBuilder::new("custom-vm")
    .add_system_package("usbip")
    .custom_kickstart(r#"
        # Custom setup script
        systemctl enable my-custom-service
        echo "Custom setup complete"
    "#)
    .build()?;
```

### IOMMU Helpers

Check IOMMU status for PCI passthrough:

```rust
use vm_provisioner::{check_iommu_enabled, get_iommu_group, is_clean_iommu_group};

if check_iommu_enabled()? {
    if let Some(group) = get_iommu_group("0000:00:14.0") {
        if is_clean_iommu_group(group)? {
            println!("Device is safe for passthrough");
        }
    }
}
```

### Available Exports

| Type | Description |
|------|-------------|
| `AppVMConfigBuilder` | Builder pattern for VM configuration |
| `AppVMProvisioner` | Main provisioner struct |
| `Installation`, `Lifecycle` | Traits for VM operations |
| `PciPassthrough`, `UsbPassthrough` | Traits for device passthrough |
| `check_iommu_enabled()` | Check if IOMMU is enabled |
| `get_iommu_group()` | Get IOMMU group for a PCI device |
| `is_clean_iommu_group()` | Check if group has single device |
| `detect_pci_device()` | Detect PCI device by address |
| `detect_usb_device()` | Detect USB device by vendor:product |

## License

MIT
