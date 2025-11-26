# VM Provisioner

**Qubes-like application isolation using libvirt/KVM.** Run each application in its own Fedora VM with seamless window integration—apps appear as native windows on your desktop.

## Why?

- **Security through isolation:** Each app runs in a separate VM, preventing cross-application attacks
- **Seamless UX:** Windows appear natively via Xpra—no visible VM boundary
- **Dynamic provisioning:** Install any combination of dnf packages and Flatpaks on-demand

## Quick Start

### Prerequisites

```bash
# Fedora
sudo dnf install libvirt qemu-kvm virt-install virt-viewer xpra
sudo systemctl enable --now libvirtd

# Debian/Ubuntu
sudo apt install libvirt-daemon qemu-kvm virtinst virt-viewer xpra
sudo systemctl enable --now libvirtd
```

### Install

```bash
git clone <repo-url>
cd vm-provisioner
cargo build --release
sudo cp target/release/vm-provisioner /usr/local/bin/
```

### Your First VM

```bash
# Create a browser VM
vm-provisioner create --flatpak io.gitlab.librewolf-community --name browser

# Start it
vm-provisioner start browser

# Generate desktop shortcuts (run after VM boots)
vm-provisioner generate-shortcuts browser

# Launch the browser (or click the generated .desktop file)
vm-provisioner launch browser "flatpak run io.gitlab.librewolf-community"
```

## Usage

### Creating VMs

```bash
# GUI VM with Flatpak
vm-provisioner create --flatpak org.mozilla.firefox --name firefox-vm

# GUI VM with system packages
vm-provisioner create --system gimp inkscape --name graphics-vm

# Mixed packages with custom resources
vm-provisioner create \
  --system git gcc nodejs \
  --flatpak com.visualstudio.code \
  --memory 8192 --disk 40 \
  --name dev-vm

# Headless/CLI-only VM
vm-provisioner create --system python3 git --headless --name cli-vm
```

### Managing VMs

| Command | Description |
|---------|-------------|
| `vm-provisioner list` | Show all VMs and their status |
| `vm-provisioner start <name>` | Boot a VM |
| `vm-provisioner stop <name>` | Graceful shutdown |
| `vm-provisioner destroy <name> -y` | Delete VM and all data |
| `vm-provisioner console <name>` | Serial console access |
| `vm-provisioner passwords` | Show VM credentials |
| `vm-provisioner generate-shortcuts <name>` | Create .desktop files for installed apps |
| `vm-provisioner launch <name> "<cmd>"` | Run a command in the VM |
| `vm-provisioner apps <name>` | List installed applications |

### Desktop Integration

After running `generate-shortcuts`, applications appear in your system menu under the VM name. Shortcuts are stored in `~/.local/share/applications/` with a `vm-provisioner-` prefix.

## Advanced Features

### USB Passthrough

Pass USB devices directly to VMs for webcams, audio devices, etc.

```bash
# Find device IDs
lsusb
# Example output: Bus 001 Device 003: ID 046d:c52b Logitech Unifying Receiver

# Pass device to VM
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --usb 046d:c52b \
  --name webcam-vm

# Hot-plug mode (device returns to host when VM stops)
vm-provisioner create --flatpak ... --usb 046d:c52b --usb-hotplug --name vm
```

### Shared Folders

Share host directories with VMs using virtiofs:

```bash
vm-provisioner create \
  --flatpak com.visualstudio.code \
  --share /home/user/projects:/mnt/projects \
  --share /home/user/Documents:/mnt/Documents \
  --name dev-vm

# Read-only sharing
vm-provisioner create ... --share /path:/mnt/path --share-readonly
```

### PCI Passthrough (GPU, etc.)

Requires IOMMU enabled in BIOS (`intel_iommu=on` or `amd_iommu=on` in kernel cmdline).

```bash
# Find device address
lspci -nn -D | grep -i nvidia

vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --pci 0000:01:00.0 --pci 0000:01:00.1 \
  --pci-hotplug \
  --name gpu-vm
```

### Web-Based Access (Selkies-GStreamer)

Access VMs from any browser without installing xpra:

```bash
vm-provisioner create \
  --flatpak io.gitlab.librewolf-community \
  --web-port 8080 \
  --name remote-browser

# Access at http://<vm-ip>:8080/
# Login: user / <password from vm-provisioner passwords>
```

### Bridged Networking

Give VMs a LAN IP (useful for accessing from other devices):

```bash
# One-time bridge setup
sudo nmcli connection add type bridge ifname br0 con-name br0
sudo nmcli connection add type bridge-slave ifname <your-interface> master br0
sudo nmcli connection up br0

# Create VM with bridge
vm-provisioner create --flatpak ... --network-bridge br0 --name lan-vm
```

### Network-Disabled VMs

For maximum isolation, create airgapped VMs:

```bash
vm-provisioner create --flatpak ... --no-network --name airgapped-vm
```

Display forwarding uses vsock instead of SSH.

## Configuration

VM configs are stored in `~/.config/vm-provisioner/<vm-name>.toml`:

```toml
name = "browser-vm"
memory_mb = 2048
vcpus = 2
disk_size_gb = 20
system_packages = ["xpra", "xorg-x11-server-Xvfb", ...]
flatpak_packages = ["org.mozilla.firefox"]
display_protocol = "Xpra"
network_mode = "Nat"
```

Passwords are stored separately in `~/.config/vm-provisioner/vm-passwords.toml`.

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Missing virtualization tools | Install `libvirt`, `qemu-kvm`, `virt-install`; ensure `libvirtd` is running |
| Xpra drops when VPN connects | Allow LAN traffic (192.168.122.0/24) in VPN settings |
| `generate-shortcuts` fails | VM must be running—wait a few seconds after boot |
| No audio | Check host PulseAudio is running (`pactl info`) |
| Old Waypipe config | Configs auto-migrate; recreate VMs for best results |

## Development

```bash
cargo fmt
cargo clippy --all-targets
cargo test
```

New display protocols can be added by implementing the `DisplayBridge` trait in `src/display_bridge.rs`.

## License

MIT
