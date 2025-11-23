# VM Provisioner - Qubes-like Application Isolation

Rust CLI that provisions Fedora-based VMs per application, with seamless window forwarding via Xpra (X11). This repository provides a single Rust 2021 codebase built around libvirt/KVM and Tokio async workflows.

## Snapshot

- **Isolation model:** each app or workload runs inside its own VM with on-demand package installation (dnf + Flatpak).
- **Display stack:** `xpra` + transient Xvfb for seamless windowing; headless mode bypasses the display bridge entirely.
- **Host support:** Fedora, Debian/Ubuntu, and other libvirt-enabled distros on x86_64 or aarch64. Requires `libvirtd`, `qemu-kvm`, `virt-install`, and `virt-viewer`.
- **Guest base:** Fedora 41 Server via virt-install network installs and kickstart templates stored under `/tmp/<vm>-kickstart`.
- **CLI:** `vm-provisioner create/start/stop/destroy/generate-shortcuts/launch/apps/passwords/console/list`.
- **Config + secrets:** TOML configs and password store live in `~/.config/vm-provisioner/`.

## Capabilities

### What already works

- Automated provisioning flow with prerequisite checks, ISO download/cache, kickstart generation, virt-install invocation, and post-install scripts (Flatpak setup, SSH keys, Xpra enablement, audio forwarding, firewall rules).
- Dynamic package selection through `--system` (dnf) and `--flatpak` (Flathub). Defaults include Xpra and X11 packages for GUI mode.
- Display abstraction via `DisplayBridge` trait with `XpraManager` implementation (`src/xpra_manager.rs`).
- Per-app `.desktop` generation under `~/.local/share/applications/vm-provisioner/` plus `vm-provisioner launch <vm> "<command>"`.
- VM lifecycle helpers (`create`, `start`, `stop`, `destroy`, `list`, `console`, `passwords`, `generate-shortcuts`, `launch`, `apps`).
- Smart memory trim: installers boot with 4 GB, then `virsh setmem/setmaxmem` shrinks the VM to the requested runtime memory (2 GB default).
- PCI passthrough and optional hot-plug support with IOMMU warnings and driver rebinding helpers.
- Password vault stored at `~/.config/vm-provisioner/vm-passwords.toml` with per-VM configs alongside it.
- Unit tests in `tests/display_bridge_tests.rs` plus manual guides/scripts (`tests/e2e_testing_guide.rs`, `test-xpra.sh`).

### Still in progress

- Xpra manual/QA coverage (VPN/audio resilience, host capability probes, doc polish).
- Documentation pass for protocol-specific troubleshooting once more validation runs conclude.

## Display Protocol

| Protocol | Default | Guest stack | Host requirement | Recommended use | Status |
|----------|---------|-------------|------------------|-----------------|--------|
| Xpra     | Yes     | `xpra`, `xorg-x11-server-Xvfb`, `pulseaudio-libs`, `git`, `openssh-server` | `xpra` CLI on host | All GUI applications | Production-ready |
| Headless | `--headless` | Minimal CLI stack (`git` by default) | None beyond libvirt/kvm | Server/CLI workloads; access via `virsh console` | Stable |

### Xpra path (native client - recommended)

1. Validates the `xpra` binary at bridge initialization. Recommend enabling "Allow LAN/local network" in your VPN so the libvirt subnet (192.168.122.0/24) stays reachable.
2. Kickstart keeps the guest in multi-user target, injects host SSH keys, enables Unix-socket forwarding (`StreamLocalBindUnlink yes`), disables PipeWire auto-start, and exports `PULSE_SERVER=unix:/run/user/1000/pulse/native`.
3. Host `.desktop` entries run `xpra start ssh://user@<vm-ip>/ --ssh="<ssh options>" --start-child="<command>" --exit-with-children`. The `--ssh` string binds to the libvirt source IP and adds `-R` PulseAudio socket tunneling.
4. Sessions launch on demand and exit once the child process ends - no persistent xpra state to clean up.
5. `test-xpra.sh` offers an automated smoke test (create VM -> shortcuts -> xpra launch/audio -> cleanup).

### Web streaming via Selkies-GStreamer (optional)

For scenarios where you need browser-based access to VM applications (remote access, no xpra client installed, etc.), you can enable Selkies-GStreamer WebRTC streaming:

```bash
# Create VM with web streaming on port 8080
vm-provisioner create --flatpak io.gitlab.librewolf-community --name browser-vm --web-port 8080
```

**Access:** Open `http://<vm-ip>:8080/` in your browser. Login with username `user` and the VM password (run `vm-provisioner passwords` to see it).

**Features:**
- WebRTC-based streaming with low latency
- Built-in audio support (no SSH forwarding needed)
- Basic authentication (username/password)
- Automatic application launch on connect
- Works alongside native xpra (no session conflicts)

**Recommendation:** Use `--web-port` when you need browser-based access from any device. For normal desktop use on the host, the native xpra client provides better integration without the browser overhead.

### Installing the protocol tooling

```bash
# Shared prerequisites
sudo dnf install libvirt qemu-kvm virt-install virt-viewer   # Fedora
sudo apt install libvirt-daemon qemu-kvm virtinst virt-viewer # Debian/Ubuntu
sudo systemctl enable --now libvirtd

# Xpra (required for GUI VMs)
sudo dnf install xpra           # Fedora/RHEL
sudo apt install xpra           # Debian/Ubuntu
sudo pacman -S xpra             # Arch
```

## Getting Started

```bash
git clone <repo-url>
cd vm-provisioner
cargo build --release
./target/release/vm-provisioner --help
```

### Creating VMs

```bash
# GUI VM with a Flatpak browser
vm-provisioner create --flatpak io.gitlab.librewolf-community

# GUI VM with mixed packages
vm-provisioner create \
  --system git gcc rust cargo nodejs npm \
  --flatpak com.visualstudio.code \
  --memory 8192 --disk 40 --name dev-vm

# Headless CLI VM
vm-provisioner create --system python3 python3-pip git --headless --name cli-vm
```

### Operating VMs

- `vm-provisioner start <name>` - boots the VM and launches the SPICE viewer (blank screen for Xpra VMs; use shortcuts to run apps).
- `vm-provisioner generate-shortcuts <name>` - after the VM boots, waits 5s and writes `.desktop` files to `~/.local/share/applications/vm-provisioner/`.
- `vm-provisioner launch <name> "<command>"` - runs ad-hoc commands via Xpra.
- `vm-provisioner apps <name>` - lists system packages (minus helper deps) plus Flatpaks to help build menus.
- `vm-provisioner stop <name>` / `destroy <name> -y` - graceful shutdown and cleanup (including `.desktop` files).
- `vm-provisioner passwords` - prints stored credentials. Console logins use `user/<generated password>`; graphical sessions auto-login.

## VM Modes & Advanced Features

### Headless mode

`--headless` provisions a minimal Fedora guest (no GUI packages, no display bridge) and presents only a serial console. Great for server workloads, automation tasks, or development stacks you want to reach via `virsh console`.

### PCI passthrough

Use `--pci <BDF>` (repeatable) to attach devices and `--pci-hotplug` to only take ownership while the VM runs.

```bash
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --pci 0000:01:00.0 --pci 0000:01:00.1 \
  --pci-hotplug \
  --name gpu-browser
```

Enable IOMMU in firmware (`intel_iommu=on` or `amd_iommu=on`), check groupings with `lspci -nn -D` and `find /sys/kernel/iommu_groups`, and expect warnings if multiple devices must move together.

### USB passthrough

Use `--usb <vendor:product>` (repeatable) to pass USB devices to the VM. Find device IDs with `lsusb`.

```bash
# Pass a webcam and USB microphone to the VM
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --usb 046d:c52b \
  --usb 08bb:2902 \
  --name webcam-vm

# Hot-plug mode: devices return to host on VM stop
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --usb 046d:c52b \
  --usb-hotplug \
  --name usb-hotplug-vm
```

USB devices are specified in `vendor:product` format (e.g., `046d:c52b` for a Logitech Unifying Receiver).

### Microphone support

Enable microphone input from host to VM with `--microphone`:

```bash
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --microphone \
  --name voice-vm
```

This enables the xpra `--microphone=yes` flag, forwarding audio input over SSH alongside the existing audio output.

### Shared storage (virtiofs)

Share host directories with the VM using `--share /host/path:/guest/mount/path` (repeatable). Files are accessible and modifiable by both host and VM.

```bash
# Share a single folder
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --share /home/user/Downloads:/mnt/Downloads \
  --name browser-vm

# Share multiple folders
vm-provisioner create \
  --flatpak com.visualstudio.code \
  --share /home/user/projects:/mnt/projects \
  --share /home/user/Documents:/mnt/Documents \
  --name dev-vm

# Read-only shared folder
vm-provisioner create \
  --flatpak io.gitlab.librewolf-community \
  --share /home/user/reference:/mnt/reference \
  --share-readonly \
  --name readonly-vm
```

Shared folders use virtiofs for high-performance file sharing. They are mounted automatically on VM boot at the specified guest path.

### Bridged networking

By default, VMs use NAT networking (192.168.122.x subnet). For VMs that need to be directly accessible from your LAN (e.g., HTML5 xpra access from other devices), use bridged networking with `--network-bridge`:

**One-time host setup (create a network bridge):**

```bash
# 1. Find your network interface
nmcli device status
# Look for your primary interface (e.g., enp0s31f6, eth0, wlp82s0)

# 2. Create the bridge
sudo nmcli connection add type bridge ifname br0 con-name br0

# 3. Add your interface as a bridge slave (replace enp0s31f6 with your interface)
sudo nmcli connection add type bridge-slave ifname enp0s31f6 master br0

# 4. Bring up the bridge
sudo nmcli connection up br0

# Note: Your network will briefly disconnect during this process
# The bridge persists across reboots - only needs to be done once
```

**Creating VMs with bridged networking:**

```bash
# VM gets an IP directly from your LAN's DHCP server
vm-provisioner create \
  --flatpak io.gitlab.librewolf-community \
  --web-port 8080 \
  --network-bridge br0 \
  --name lan-browser

# Now accessible from any LAN device at http://<vm-lan-ip>:8080/
```

**Benefits:**
- VM gets a real LAN IP address (e.g., 192.168.1.x)
- Directly accessible from other devices on the network
- No port forwarding or iptables rules needed
- VM can be reached even if the host IP changes

**When to use:**
- Web streaming access from phones, tablets, or other computers
- VMs that need to provide network services
- Multi-machine development environments

### Memory management

Installers always boot with 4 GB RAM; after provisioning, the tool powers the VM off, applies `virsh setmem/setmaxmem`, and restarts so runtime memory matches `--memory` (default 2048 MB). No manual action required.

### Desktop integration

Generated `.desktop` entries contain the full launch string including SSH options, reverse PulseAudio tunnels, and `--exit-with-children`. Remove them with `vm-provisioner destroy <name>` or regenerate after pruning packages.

## Configuration & Secrets

- Per-VM configs: `~/.config/vm-provisioner/<vm>.toml`
- Password store: `~/.config/vm-provisioner/vm-passwords.toml`

Typical Xpra VM config:

```toml
name = "browser-vm"
memory_mb = 4096
vcpus = 2
disk_size_gb = 20
vm_dir = "/var/lib/libvirt/images"
system_packages = ["xpra", "xorg-x11-server-Xvfb", "pulseaudio-libs", "git", "openssh-server", "flatpak"]
flatpak_packages = ["org.mozilla.firefox"]
display_protocol = "Xpra"
web_port = 8080             # Optional: port for Selkies-GStreamer WebRTC web access (omit to disable)
graphics_backend = "VirtioGpu"
enable_clipboard = true
enable_audio = true
enable_microphone = false
network_mode = "Nat"

[[shared_folders]]
host_path = "/home/user/Downloads"
guest_path = "/mnt/Downloads"
tag = "mnt_Downloads"
readonly = false
```

For headless VMs the defaults collapse to just `["git"]` and Flatpaks are ignored.

## Troubleshooting & Testing

- `cargo test` - runs unit tests (config defaults, display bridge wiring).
- `cargo test --test e2e_testing_guide -- --ignored` - documentation-style E2E checklist once infrastructure is available.
- `./test-xpra.sh` - optional helper to exercise the Xpra stack (creation, shortcuts, xpra launch, audio, cleanup).

Common fixes:

- **Missing virtualization tools:** Install `virsh`, `virt-install`, `qemu-img`, ensure `libvirtd` is running.
- **Xpra drops on VPN enable:** Allow LAN/local network traffic in your VPN client or add a manual route so 192.168.122.0/24 stays reachable; xpra's SSH transport binds to the libvirt source IP and cannot traverse VPNs that block that subnet.
- **`generate-shortcuts` fails:** VM must be running; rerun after `vm-provisioner start <name>` and give it a few extra seconds on slower disks.
- **No audio:** Confirm PipeWire/PulseAudio is running on the host (`pactl info`). The guest disables its own audio stack and expects the reverse SSH socket.
- **Old "Waypipe" config:** Old VMs with `display_protocol = "Waypipe"` will be automatically migrated to Xpra when loaded. Consider re-provisioning for best results.

## Development

```bash
cargo fmt
cargo clippy --all-targets
cargo test
```

New display protocols can be added by implementing the `DisplayBridge` trait. Follow the patterns in `xpra_manager.rs`.

Contributions should keep documentation in sync with the current Rust implementation so new contributors can rely on this README as the canonical workflow guide.

## Migration from Waypipe

Waypipe/Wayland support has been deprecated due to critical provisioning bugs. If you have existing VMs created with Waypipe:

1. **Automatic migration:** When you load an old config, it will automatically be migrated to use Xpra.
2. **Recommended:** Destroy and recreate VMs for best results:
   ```bash
   vm-provisioner destroy <old-vm> -y
   vm-provisioner create --flatpak <your-apps> --name <new-vm>
   ```
