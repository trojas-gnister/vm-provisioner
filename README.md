# VM Provisioner - Qubes-like Application Isolation System

A Rust-based application isolation system inspired by Qubes OS. Creates lightweight VMs with dynamic package installation (system + Flatpak) featuring auto-login, a Sway (Wayland) desktop, SPICE integration with auto-resize functionality, and comprehensive Flatpak support.

## Features

- 🔒 **Application Isolation**: Each application runs in its own VM for security
- 📦 **Dynamic Package Installation**: Install any system (dnf) or Flatpak packages on-demand
- 🚀 **Legacy Auto-Launch Hooks**: Optional per-VM startup commands remain available for manual configuration
- 🔓 **Auto-Login**: Passwordless login with the Sway Wayland compositor
- 🖥️ **kitty Terminal**: Default terminal emulator included in every VM
- 🪟 **Seamless Window Integration**: VM applications appear as native host windows via Waypipe (Wayland)
- 📋 **Clipboard Sharing**: Bidirectional clipboard sharing between host and VMs via Waypipe's native clipboard protocol
- 🖥️ **Sway Compositor**: Wayland-first i3-compatible environment tuned for virtual machines
- 🚀 **Application Launcher**: rofi with complete Flatpak integration + native host integration
- 💾 **Password Management**: Centralized storage and individual VM credential management
- 🔧 **Cross-Architecture**: Full support for x86_64 and aarch64 (ARM64)
- 📏 **Dynamic Resolution**: Applications adapt to host display resolution automatically
- 🔗 **Desktop Integration**: Generated .desktop files make VM apps appear in host application menu

## Current Status

**✅ Fully Working System:**
- **Complete VM isolation**: Each application runs in its own secure VM
- **Dynamic package installation**: Install any system (dnf) or Flatpak packages on-demand
- **Auto-login**: Passwordless login with the Sway Wayland compositor ready at boot
- **Seamless window integration**: VM applications appear as native host windows via Waypipe (WinApps-style)
- **Desktop integration**: .desktop files make VM apps appear in the host application menu with Waypipe launchers
- **Waypipe seamless mode**: Individual applications launch over SSH with Zstd compression and PulseAudio socket forwarding
- **Clipboard sharing**: Bidirectional clipboard between host and VMs via Wayland protocol bridging
- **Robust package management**: All critical packages (sway, waybar, wl-clipboard, kitty, SSH) install correctly
- **Cross-architecture support**: ARM64 and x86_64 compatibility verified and working
- **Advanced CLI**: --system and --flatpak flags for dynamic VM creation
- **Application launcher**: rofi with full Flatpak integration and discovery
- **Comprehensive logging**: Detailed installation logs for troubleshooting
- **VM lifecycle management**: create/start/stop/destroy/list/passwords/apps/launch/generate-shortcuts
- **Centralized password storage**: Secure credential management system
- **Smart memory management**: 2GB default, temporarily uses 4GB during installation
- **Low-latency audio**: PulseAudio/PipeWire socket forwarding keeps audio in sync
- **Automatic SSH key acceptance**: Seamless Waypipe connections without manual setup

**🚧 Future Enhancements:**
- GPU passthrough for hardware acceleration (virgl/Venus)
- VirtIO channels for improved performance
- Pre-configured VM templates (Firefox, VS Code, development, media, etc)
- Shared folders via 9p/virtiofs

## VM Modes

### GUI Mode (Default)
Full desktop environment with the Sway compositor, SPICE viewer, and auto-login. Ideal for browser isolation, graphical applications, and desktop productivity tools.

### Headless Mode
Lightweight CLI-only VMs with no X11, no desktop environment, and no graphics. Access via serial console. Perfect for:
- Development environments (compilers, interpreters, build tools)
- Server applications (databases, web servers)
- CLI tools and utilities
- Minimal resource usage

### PCI Passthrough
Direct hardware access for VMs. Two modes:
- **Hot-plug (dynamic)**: Devices attach when VM starts, detach when VM stops (gives device back to host)
- **Permanent**: Devices reserved for VM in XML configuration

Requirements:
- IOMMU enabled in BIOS (VT-d for Intel, AMD-Vi for AMD)
- Kernel parameter: `intel_iommu=on` or `amd_iommu=on`
- Run `lspci` to find device addresses
- Use `--pci <BDF>` (repeatable) on `vm-provisioner create`, optionally combining with `--pci-hotplug` for dynamic attach. A full Steam/Wine GPU passthrough example is shown in the “PCI Passthrough Examples” section.

## Quick Start

### Prerequisites

Install required tools:
```bash
# Fedora/RHEL
sudo dnf install libvirt qemu-kvm virt-install virt-viewer

# Ubuntu/Debian
sudo apt install libvirt-daemon qemu-kvm virtinst virt-viewer

# Start libvirt service
sudo systemctl enable --now libvirtd
```

### Installation

```bash
git clone <repository-url>
cd vm-provisioner
cargo build --release
```

### Basic Usage

1. **Create VMs with Dynamic Packages**:
```bash
# LibreWolf browser VM (auto-named "io-gitlab-librewolf-community-vm")
./target/release/vm-provisioner create --flatpak io.gitlab.librewolf-community

# LibreWolf + qBittorrent with custom name
./target/release/vm-provisioner create --flatpak io.gitlab.librewolf-community --system qbittorrent --name media-vm

# Development environment with multiple tools
./target/release/vm-provisioner create --flatpak com.visualstudio.code --system git gcc rust cargo nodejs npm --name dev-vm --memory 8192

# Office suite with communication apps
./target/release/vm-provisioner create --system libreoffice --flatpak com.slack.Slack --flatpak org.telegram.desktop --name office-vm
```

2. **Start the VM**:
```bash
# GUI VM - launches SPICE viewer
./target/release/vm-provisioner start media-vm
# SPICE viewer launches automatically with Sway running inside the VM
# Auto-login enabled - no password required
# Auto-resize works when you resize virt-viewer window
# Clipboard sharing enabled between host and VM
# Use Mod+d for rofi launcher, Mod+Enter for terminal

# Headless VM - connect via console
./target/release/vm-provisioner start dev-vm
# Then: virsh console dev-vm
```

3. **Manage VMs**:
```bash
# List all VMs
./target/release/vm-provisioner list

# Show all VM passwords (for console access if needed)
./target/release/vm-provisioner passwords

# Connect to VM console (if needed)
./target/release/vm-provisioner console media-vm
# Use credentials: user / [generated-password]
# Note: SPICE viewer has auto-login, console requires password

# Stop VM
./target/release/vm-provisioner stop media-vm

# Destroy VM (with comprehensive cleanup)
./target/release/vm-provisioner destroy media-vm
```

## Package Examples

### System Packages (via dnf)
```bash
# Productivity
--system libreoffice gimp inkscape

# Development
--system git gcc rust cargo python3 nodejs npm

# Media
--system vlc mpv audacity

# System tools
--system htop neofetch tree wget curl
```

### Flatpak Packages
```bash
# Browsers
--flatpak org.mozilla.firefox
--flatpak io.gitlab.librewolf-community
--flatpak com.google.Chrome

# Communication
--flatpak com.slack.Slack
--flatpak com.discordapp.Discord
--flatpak org.telegram.desktop

# Media & Entertainment
--flatpak com.spotify.Client
--flatpak org.videolan.VLC
--flatpak org.kde.kdenlive

# Development
--flatpak com.visualstudio.code
--flatpak org.kde.kdevelop
--flatpak com.jetbrains.IntelliJ-IDEA-Community
```

## Configuration

VM configurations and passwords are automatically stored:

### Individual VM Config
```toml
# ~/.config/vm-provisioner/firefox-vm.toml
name = "firefox-vm"
memory_mb = 4096
vcpus = 2
disk_size_gb = 20
vm_dir = "/var/lib/libvirt/images"

# Package installation
system_packages = ["@base-x", "gdm", "xorg-x11-server-Xorg", "wmctrl", "xwininfo", "pipewire", "wl-clipboard", "kitty"]
flatpak_packages = ["org.mozilla.firefox"]
# Legacy auto-launch hooks (not populated by CLI)
auto_launch_apps = ["flatpak run org.mozilla.firefox"]

# Graphics and features
graphics_backend = "VirtioGpu"
enable_clipboard = true
enable_audio = true
enable_usb_passthrough = false
enable_auto_login = true

# Security
network_mode = "Nat"
firewall_rules = ["OUTPUT -p udp --dport 53 -j ACCEPT", "OUTPUT -p tcp --dport 443 -j ACCEPT"]
user_password = "vm-abc123def456"
```

### Centralized Password Storage
```toml
# ~/.config/vm-provisioner/vm-passwords.toml
[vms]
firefox-vm = "vm-abc123def456"
office-vm = "vm-789xyz012abc"
dev-vm = "vm-456def789ghi"
```

## Architecture

```
┌─────────────────┐    ┌─────────────────┐
│     Host OS     │    │   VM (Fedora)   │
│                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │ Window      │◄┼────┼─┤ Guest       │ │
│ │ Proxy       │ │    │ │ Agent       │ │
│ │ TCP:9999    │ │    │ │             │ │
│ │ ┌─────────┐ │ │    │ │ ┌─────────┐ │ │
│ │ │ Wayland │ │ │    │ │ │LibreWolf│ │ │
│ │ │ Client  │ │ │    │ │ │ + X11   │ │ │
│ │ │Framework│ │ │    │ │ │         │ │ │
│ │ └─────────┘ │ │    │ │ └─────────┘ │ │
│ └─────────────┘ │    │ └─────────────┘ │
└─────────────────┘    └─────────────────┘
         ▲                       │
         │ TCP Binary Protocol   │
         └───────────────────────┘
```

## Seamless Window Integration via Waypipe

**Architecture** (WinApps/Qubes inspired, Wayland-native):
1. VM boots directly into Sway (Wayland compositor) with auto-login.
2. SSH server is configured with passwordless authentication and firewall rules.
3. Waypipe on the host launches per-application sessions via `waypipe --compress zstd ssh ...`.
4. Each .desktop file in `~/.local/share/applications/vm-provisioner/` wraps the appropriate Waypipe command.
5. Waypipe mirrors Wayland buffers so application windows appear on the host as native windows.
6. Clipboard sharing, pointer events, and keyboard focus are delivered by the Wayland protocol.
7. PulseAudio/PipeWire audio is forwarded by reverse SSH socket tunneling.

**How It Works:**
```
Host Application Menu         Guest VM (Sway + Waypipe)
┌─────────────────┐           ┌──────────────────────┐
│ Firefox (VM)    │─Waypipe──▶│  Sway + Firefox       │
│ LibreWolf (VM)  │─SSH+PA───▶│  Sway + LibreWolf     │
│ VSCode (VM)     │──────────▶│  Sway + VSCode        │
└─────────────────┘           └──────────────────────┘
      ▲                                  │
      └──── .desktop files ──────────────┘
```

**Workflow:**
```
1. Create a VM with apps:
   vm-provisioner create --flatpak org.mozilla.firefox --name browser-vm

2. Start the VM (SSH + Sway auto-configured):
   vm-provisioner start browser-vm
   → SSH public key injected automatically
   → VM host key trusted via ssh-keyscan
   → tty1 auto-login launches Sway immediately

3. Generate Waypipe shortcuts:
   vm-provisioner generate-shortcuts browser-vm
   → Creates ~/.local/share/applications/vm-provisioner/browser-vm-firefox.desktop

4. Launch from the host application launcher:
   Click "Firefox (VM: browser-vm)"
   → Runs: waypipe --compress zstd ssh -R /run/user/1000/pulse/native:/run/user/1000/pulse/native user@${VM_IP} flatpak run org.mozilla.firefox

5. Or launch manually:
   vm-provisioner launch browser-vm "flatpak run org.mozilla.firefox"
```

**Why Waypipe?**
- ✅ Wayland-native protocol forwarding with minimal overhead
- ✅ SSH transport with key-based auth plus optional Zstd/LZ4 compression
- ✅ Clipboard sync piggybacks on Wayland; no extra daemons required
- ✅ Audio stays on the host via a PulseAudio/PipeWire socket tunnel
- ✅ Simpler than X2Go/Xpra—no server daemons or NX stack inside the VM
- ✅ Future-proof: aligned with modern Wayland desktops on host and guest

### Audio Configuration

Waypipe focuses on graphics, so audio travels over SSH:

```bash
waypipe --compress zstd ssh   -R /run/user/1000/pulse/native:/run/user/$(id -u)/pulse/native   user@${VM_IP} flatpak run org.mozilla.firefox
```

- The reverse tunnel exposes the host PulseAudio socket inside the VM.
- Applications in the VM connect to that socket and play audio through the host speakers.
- PipeWire/PulseAudio helper scripts inside the VM ensure the sound server is available.
- Latency stays ~50–100 ms on the LAN without buffer growth.

## Memory Management

VMs run with **2GB RAM by default** (configurable with `--memory`). During installation, the system temporarily uses **4GB RAM** (required for Fedora network installation), then automatically reduces to the configured amount after the VM is created.

**This happens transparently:**
1. **Installation**: VM uses 4GB RAM for package download and installation
2. **First boot**: Automatic reduction to configured amount (default 2GB)
3. **Runtime**: VM runs with configured memory allocation

**Why this works:**
- Fedora network installation requires 4GB to download packages during install
- Once installed, VMs run efficiently with 2GB (or your custom amount)
- Smart memory management happens automatically without user intervention

**Manual configuration:**
```bash
# 2GB VM (uses 4GB during install, then 2GB)
vm-provisioner create --flatpak org.mozilla.firefox --memory 2048

# 1GB VM (uses 4GB during install, then 1GB)
vm-provisioner create --system git vim --headless --memory 1024

# 8GB VM (no temporary increase needed)
vm-provisioner create --flatpak com.visualstudio.code --memory 8192
```

## Sway (i3-Compatible) Usage

GUI VMs now use the lightweight Sway compositor (i3-compatible) for optimal Wayland performance:

### Key Shortcuts
- `Mod+Enter` - Open kitty terminal
- `Mod+d` - Open rofi (application launcher with Flatpak support)
- `Mod+Shift+d` - Open dmenu (traditional command launcher)
- `Mod+Shift+q` - Close focused window
- `Mod+1,2,3,4,5` - Switch to workspace 1-5
- `Mod+Shift+1,2,3,4,5` - Move window to workspace 1-5
- `Mod+Arrow Keys` - Change window focus
- `Mod+Shift+Arrow Keys` - Move focused window
- `Mod+Shift+r` - Reload Sway config
- `Mod+Shift+e` - Exit Sway

**Note**: `Mod` key is typically the Windows/Super key

### Application Launcher Features
- **rofi** (`Mod+d`): Shows all applications including Flatpak packages with icons
- **Legacy auto-launch hooks**: You can add commands to `auto_launch_apps` manually if needed
- **Flatpak Integration**: Proper XDG_DATA_DIRS configuration for app discovery
- **Terminal**: Access via `Mod+Enter` for kitty terminal

### Launching Applications Manually
```bash
# Via rofi (Mod+d) - recommended, shows all apps with icons
# Via dmenu (Mod+Shift+d) - traditional text-based launcher
# Via terminal (Mod+Enter, then type):
qbittorrent                            # System package
flatpak run io.gitlab.librewolf-community  # Flatpak package
```

### Window Management
- Sway automatically tiles windows (same workflow as i3)
- Drag windows while holding `Mod` key to make them floating
- Windows are organized in workspaces (1-5 by default)
- Status bar shows current workspace and system information

## Security

- **VM Isolation**: Hardware virtualization prevents application breakout
- **SELinux**: Mandatory access control enabled in guest
- **Minimal Attack Surface**: Guest OS has only necessary packages
- **Network Isolation**: VMs use NAT by default
- **Clipboard Security**: Controlled sharing via SPICE protocol

## Commands

- `create` - Create new VM with dynamic packages
- `start` - Start VM and launch viewer
- `stop` - Stop running VM
- `list` - Show all VMs and their status
- `passwords` - Show login credentials for all VMs
- `destroy` - Remove VM and cleanup
- `console` - Connect to VM console
- `generate-shortcuts` - Create .desktop files for VM applications
- `launch` - Launch specific application in VM via Waypipe
- `apps` - List all applications available in a VM

### Command Options

**VM Creation:**
- `--name <name>` - Custom VM name (auto-generated if not provided)
- `--system <pkg>` - System packages to install (can be used multiple times)
- `--flatpak <pkg>` - Flatpak packages to install (can be used multiple times)
- `--memory <mb>` - Memory allocation in MB (default: 2048)
- `--vcpus <n>` - Number of virtual CPUs (default: 2)
- `--disk <gb>` - Disk size in GB (default: 20)
- `--headless` - Headless mode - no GUI/desktop environment (CLI only)
- `--pci <address>` - PCI device to passthrough (format: 0000:01:00.0, repeatable)
- `--pci-hotplug` - Enable PCI hot-plug mode (attach on start, detach on stop)
- `--config <path>` - Use custom configuration file
- `--yes, -y` - Skip confirmation prompts

## Examples

### Browser VMs for Different Use Cases
```bash
# Personal Firefox with high resources
vm-provisioner create --flatpak org.mozilla.firefox --memory 8192 --vcpus 4 --name personal-browser

# Work browser with Slack
vm-provisioner create --flatpak io.gitlab.librewolf-community --flatpak com.slack.Slack --name work-browser

# Banking browser (isolated)
vm-provisioner create --flatpak org.mozilla.firefox --name banking-browser
```

### Development Environment
```bash
# Full development setup with GUI
vm-provisioner create --flatpak com.visualstudio.code --system git gcc rust cargo python3 nodejs npm --name dev-env --memory 8192 --disk 40

# Headless Python development (CLI only, no X11/desktop)
vm-provisioner create --system python3 python3-pip git vim tmux --name python-dev --headless

# Headless Rust toolchain
vm-provisioner create --system rust cargo git --name rust-dev --headless --memory 2048
```

### Media & Productivity
```bash
# Media editing suite
vm-provisioner create --flatpak org.kde.kdenlive --flatpak org.gimp.GIMP --system audacity --memory 8192 --name media-vm

# Office suite with extras
vm-provisioner create --system libreoffice --flatpak com.slack.Slack --flatpak org.telegram.desktop --name office-vm
```

### PCI Passthrough Examples
```bash
# GPU passthrough with hot-plug (dynamic - gives GPU back to host when VM stops)
vm-provisioner create --flatpak org.mozilla.firefox --pci 0000:01:00.0 --pci-hotplug --name gpu-browser

# Permanent GPU passthrough (GPU reserved for VM)
vm-provisioner create --flatpak org.mozilla.firefox --pci 0000:01:00.0 --name gpu-browser

# Multiple PCI devices
vm-provisioner create --pci 0000:01:00.0 --pci 0000:02:00.0 --pci-hotplug --name multi-device-vm

# Find your PCI devices
lspci -nn  # List all PCI devices with addresses

# Steam + Wine gaming VM with dedicated GPU (permanent passthrough)
vm-provisioner create \
  --flatpak com.valvesoftware.Steam \
  --system wine winetricks vulkan-loader mesa-dri-drivers \
  --name steam-gaming-vm \
  --memory 16384 \
  --vcpus 6 \
  --disk 80 \
  --pci 0000:01:00.0 \
  --pci 0000:01:00.1

# Same VM but keep the GPU on the host when the VM is off (hot-plug)
vm-provisioner create \
  --flatpak com.valvesoftware.Steam \
  --system wine winetricks vulkan-loader mesa-dri-drivers \
  --name steam-gaming-vm \
  --memory 16384 \
  --vcpus 6 \
  --disk 80 \
  --pci 0000:01:00.0 \
  --pci 0000:01:00.1 \
  --pci-hotplug
```
In the Steam/Wine example, `0000:01:00.0` is the GPU itself and `0000:01:00.1` is its HDMI/DP audio function (both must move together). Replace the addresses with the ones reported by `lspci -nn` on your system. The `--system wine winetricks vulkan-loader mesa-dri-drivers` bundle installs Wine, helper tooling, and the Vulkan userspace bits that Steam Proton expects; feel free to add other packages (e.g., `gamemode`, `steam-devices`) if your distro provides them.

### Auto-Generated VM Names
```bash
# VM will be named "org-mozilla-firefox-vm"
vm-provisioner create --flatpak org.mozilla.firefox

# VM will be named "git-vm"
vm-provisioner create --system git

# VM will be named "app-vm-[timestamp]"
vm-provisioner create
```

## Seamless Window Usage

### Setting Up Seamless Applications

1. **Create VM with your desired applications:**
```bash
# Browser VM
vm-provisioner create --flatpak org.mozilla.firefox --name browser-vm

# Development VM
vm-provisioner create --flatpak com.visualstudio.code --system git --name dev-vm

# Media VM
vm-provisioner create --flatpak org.videolan.VLC --system qbittorrent --name media-vm
```

2. **Start the VM:**
```bash
vm-provisioner start browser-vm
# SSH server and Waypipe auto-login configured with passwordless authentication
```

3. **Generate desktop shortcuts (makes apps appear in your application menu):**
```bash
vm-provisioner generate-shortcuts browser-vm
# Creates .desktop files in ~/.local/share/applications/vm-provisioner/
```

4. **Launch applications:**
```bash
# From application menu: Just click the app!

# Or manually from command line:
vm-provisioner launch browser-vm "flatpak run org.mozilla.firefox"
vm-provisioner launch dev-vm "flatpak run com.visualstudio.code"
```

5. **List available applications:**
```bash
vm-provisioner apps browser-vm
```

### Generated .desktop Files

Each application gets a .desktop file that launches it seamlessly:

```desktop
[Desktop Entry]
Name=Firefox (VM: browser-vm)
Exec=waypipe --compress zstd ssh -R /run/user/1000/pulse/native:/run/user/1000/pulse/native user@192.168.122.X flatpak run org.mozilla.firefox
Icon=firefox
Type=Application
Categories=Network;WebBrowser;
```

**Features enabled in .desktop files:**
- Passwordless SSH authentication (using host SSH keys)
- Seamless window mode (applications appear as native windows)
- Clipboard sharing (copy/paste between host and VM)
- Audio passthrough (PulseAudio/PipeWire via NX protocol)
- Dynamic resolution (adapts to your display)
- Session suspend/resume support

### Prerequisites on Host

Install Waypipe on the host:
```bash
# Fedora/RHEL
sudo dnf install waypipe

# Ubuntu/Debian
sudo apt install waypipe

# Arch
sudo pacman -S waypipe
```

Waypipe relies on native SSH:
- Host's SSH public key (`~/.ssh/id_rsa.pub`, `id_ed25519.pub`, etc.) is injected into the VM automatically
- VM host keys are accepted via `ssh-keyscan` so there are no prompts
- PulseAudio/PipeWire travels over SSH via a reverse socket (`-R /run/user/.../pulse/native`)
- Generate a key first if needed: `ssh-keygen -t ed25519`

## Troubleshooting

### VM Creation Fails
- Check libvirt status: `sudo systemctl status libvirtd`
- Verify KVM support: `lsmod | grep kvm`
- Check disk space: `df -h /var/lib/libvirt/images/`

### Auto-Resize Not Working
- **Enable in virt-manager**: Go to View menu → "Auto resize VM with window"
- Check spice-autorandr service: `systemctl status spice-autorandr.service`
- Start if needed: `sudo systemctl start spice-autorandr.service`
- For ARM64: Uses spice-autorandr instead of QXL (QXL not supported on ARM64)

### Flatpak Apps Not in Launcher
- Fixed automatically with rofi and proper XDG_DATA_DIRS configuration
- Use `Mod+d` for rofi launcher (shows all Flatpak apps)

### Applications Not Auto-Starting
- Applications should start automatically via Sway `exec` directives
- Check Sway config: `cat ~/.config/sway/config | grep "^exec"`
- Manual start (inside VM terminal): `flatpak run <app-id>` or `<system-app>`

### SPICE Connection Issues
- Ensure VM is running: `virsh list`
- Check spice-vdagentd: `sudo systemctl status spice-vdagentd`
- Verify clipboard sharing: SPICE protocol handles this automatically

### PCI Passthrough Issues
- **IOMMU not enabled**: Enable VT-d/AMD-Vi in BIOS and add kernel parameter
  ```bash
  # Check if IOMMU enabled
  dmesg | grep -e IOMMU -e DMAR

  # Add to GRUB: /etc/default/grub
  GRUB_CMDLINE_LINUX="intel_iommu=on"  # or amd_iommu=on
  sudo grub2-mkconfig -o /boot/grub2/grub.cfg
  ```
- **Device not found**: Use `lspci -nn -D` to find correct address
- **IOMMU group warnings**: All devices in same group must be passed through together
- **Permission denied**: Commands use sudo for driver binding
- **Hot-plug fails**: Check `dmesg` and `virsh dumpxml <vm>` for errors

### Waypipe Connection Issues
- **waypipe not found**: Install it on the host (`sudo dnf install waypipe`, `sudo apt install waypipe`, etc.)
- **SSH host key verification failed**: Provisioning adds the VM key via `ssh-keyscan`, but you can rerun `ssh-keyscan -H <vm-ip> >> ~/.ssh/known_hosts`
- **Permission denied**: Ensure a public key exists in `~/.ssh` (generate one with `ssh-keygen -t ed25519`)
- **No window appears**: Confirm the VM is running and Sway is active (`vm-provisioner start <vm>` then `vm-provisioner apps <vm>`)
- **.desktop file launches nothing**: Regenerate shortcuts after installing Waypipe and verify the Waypipe binary is on `PATH`

### Audio Issues with Waypipe
- **No audio**: Waypipe needs the SSH reverse tunnel. Verify `-R /run/user/1000/pulse/native:/run/user/<host_uid>/pulse/native` is present in the Exec line.
- **Unknown PulseAudio socket**: Run `pactl info | grep 'Server String'` on the host and use that path for the second half of the `-R` flag.
- **Choppy audio**: Check network latency and ensure PipeWire/PulseAudio is running on the host (`pactl info`).
- **VM apps still silent**: Confirm PipeWire helper script in the VM is executable (`/home/user/.local/bin/start-pipewire.sh`) and restart the VM.

### Performance Issues
- Enable KVM acceleration: Check `kvm-ok` or `/proc/cpuinfo`
- Increase VM memory: Use `--memory` option
- VirtIO-GPU provides good performance on both x86_64 and ARM64

## Development

### Building from Source
```bash
git clone <repository-url>
cd vm-provisioner
cargo build --release
cargo test
```

### Adding New Templates
1. Create template function in `src/config.rs`
2. Add to template matching in `src/main.rs`
3. Update documentation and tests


---

**Status**: This project provides a **fully functional VM isolation system** with complete auto-login and seamless window integration. VMs display via SPICE viewer for console access, and **per-application seamless windows via Waypipe** for true application isolation (WinApps/Qubes-style). Clipboard sharing, audio passthrough, and dynamic resolution all work seamlessly.
