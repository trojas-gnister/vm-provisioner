# VM Provisioner - Qubes-like Application Isolation System

A Rust-based application isolation system inspired by Qubes OS. Creates lightweight VMs with dynamic package installation (system + Flatpak) featuring auto-login, auto-launch applications, i3 window manager, SPICE integration with auto-resize functionality, and comprehensive Flatpak support.

## Features

- 🔒 **Application Isolation**: Each application runs in its own VM for security
- 📦 **Dynamic Package Installation**: Install any system (dnf) or Flatpak packages on-demand
- 🚀 **Auto-Launch Applications**: Specified applications start automatically when VM boots
- 🔓 **Auto-Login**: Passwordless login with i3 window manager and desktop access
- 🖥️ **kitty Terminal**: Default terminal emulator included in every VM
- 🪟 **Seamless Window Integration**: VM applications appear as native host windows via Xpra
- 📋 **Clipboard Sharing**: Bidirectional clipboard sharing between host and VMs via Xpra
- 🖥️ **i3 Window Manager**: Lightweight tiling window manager with full X11 compatibility
- 🚀 **Application Launcher**: rofi with complete Flatpak integration + native host integration
- 💾 **Password Management**: Centralized storage and individual VM credential management
- 🔧 **Cross-Architecture**: Full support for x86_64 and aarch64 (ARM64)
- 📏 **Dynamic Resolution**: Applications adapt to host display resolution automatically
- 🔗 **Desktop Integration**: Generated .desktop files make VM apps appear in host application menu

## Current Status

**✅ Fully Working System:**
- **Complete VM isolation**: Each application runs in its own secure VM
- **Dynamic package installation**: Install any system (dnf) or Flatpak packages on-demand
- **Auto-login & auto-launch**: Passwordless login with applications starting automatically
- **Seamless window integration**: VM applications appear as native host windows via Xpra (WinApps-style)
- **Desktop integration**: .desktop files make VM apps appear in host application menu
- **Xpra seamless mode**: Individual applications launch seamlessly with passwordless SSH
- **Clipboard sharing**: Bidirectional clipboard between host and VMs via Xpra
- **Robust package management**: All critical packages (xset, i3, kitty, xpra, SSH) install correctly
- **Cross-architecture support**: ARM64 and x86_64 compatibility verified and working
- **Advanced CLI**: --system and --flatpak flags for dynamic VM creation
- **Application launcher**: rofi with full Flatpak integration and discovery
- **Comprehensive logging**: Detailed installation logs for troubleshooting
- **VM lifecycle management**: create/start/stop/destroy/list/passwords/apps/launch/generate-shortcuts
- **Centralized password storage**: Secure credential management system
- **Smart memory management**: 2GB default, temporarily uses 4GB during installation
- **Low-latency audio**: Optimized Xpra configuration for minimal delay (<500ms)
- **Automatic SSH key acceptance**: Seamless Xpra connections without manual setup

**🚧 Future Enhancements:**
- GPU passthrough for hardware acceleration (virgl/Venus)
- VirtIO channels for improved performance
- Pre-configured VM templates (Firefox, VS Code, development, media, etc)
- Shared folders via 9p/virtiofs

## VM Modes

### GUI Mode (Default)
Full desktop environment with i3 window manager, SPICE viewer, auto-login, and application auto-launch. Ideal for browser isolation, graphical applications, and desktop productivity tools.

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
# SPICE viewer launches automatically with i3 window manager
# Auto-login enabled - no password required
# Applications auto-launch on boot (LibreWolf + qBittorrent)
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

## Seamless Window Integration via Xpra

**Architecture** (inspired by WinApps):
1. VM runs applications in X11 environment with i3 window manager and auto-login
2. SSH server configured with passwordless key-based authentication
3. Xpra server configured for seamless window mode in the VM
4. Xpra client on host connects via SSH and launches individual applications
5. Applications appear as native host windows with full integration
6. .desktop files make VM apps appear in host application menu
7. Clipboard, audio, and window management via Xpra protocol

**How It Works:**
```
Host Application Menu         Guest VM (Xpra + SSH)
┌─────────────────┐           ┌──────────────────────┐
│ Firefox (VM)    │─SSH+Xpra─▶│  i3 + Firefox         │
│ LibreWolf (VM)  │─Seamless─▶│  i3 + LibreWolf       │
│ VSCode (VM)     │───────────▶│  i3 + VSCode          │
└─────────────────┘           └──────────────────────┘
      ▲                                  │
      └──── .desktop files ──────────────┘
```

**Workflow:**
```
1. Create VM with applications:
   vm-provisioner create --flatpak org.mozilla.firefox --name browser-vm

2. Start VM (SSH keys & Xpra auto-configured):
   vm-provisioner start browser-vm
   → SSH public key injected into VM during provisioning
   → VM's SSH host key automatically added to ~/.ssh/known_hosts
   → Passwordless authentication ready immediately

3. Generate desktop shortcuts:
   vm-provisioner generate-shortcuts browser-vm
   → Creates ~/.local/share/applications/vm-provisioner/browser-vm-firefox.desktop

4. Launch from host application menu:
   Click "Firefox (VM: browser-vm)" in application menu
   → Opens Firefox in seamless window via Xpra

5. Or launch manually:
   vm-provisioner launch browser-vm "flatpak run org.mozilla.firefox"
```

**Benefits of Xpra:**
- ✅ Designed specifically for seamless remote applications on Linux
- ✅ SSH-based security with passwordless key authentication
- ✅ True per-application window integration (like Qubes OS)
- ✅ Clipboard, audio, window management built-in
- ✅ Dynamic resolution adaptation
- ✅ No complex RDP setup or RemoteApp limitations
- ✅ Native Linux solution with excellent X11 integration

### Audio Configuration

Xpra is configured for low-latency audio with minimal delay (<500ms):

**Client-side optimizations:**
- Buffer time: 32ms (reduced from 200ms default)
- Latency time: 16ms (reduced from 10ms default)
- Codec: Opus (lowest latency audio codec)
- Environment variables set automatically

**Server-side optimizations:**
- Matching buffer/latency configuration in VM
- Opus codec for both speaker and microphone
- PulseAudio integration enabled
- Configuration applied during VM provisioning

These settings are automatically applied when launching applications via Xpra, providing smooth audio playback for videos, music, and communication applications.

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

## i3 Window Manager Usage

VMs use the lightweight i3 window manager for optimal X11 compatibility and performance:

### Key Shortcuts
- `Mod+Enter` - Open kitty terminal
- `Mod+d` - Open rofi (application launcher with Flatpak support)
- `Mod+Shift+d` - Open dmenu (traditional command launcher)
- `Mod+Shift+q` - Close focused window
- `Mod+1,2,3,4,5` - Switch to workspace 1-5
- `Mod+Shift+1,2,3,4,5` - Move window to workspace 1-5
- `Mod+Arrow Keys` - Change window focus
- `Mod+Shift+Arrow Keys` - Move focused window
- `Mod+Shift+r` - Restart i3
- `Mod+Shift+e` - Exit i3

**Note**: `Mod` key is typically the Windows/Super key

### Application Launcher Features
- **rofi** (`Mod+d`): Shows all applications including Flatpak packages with icons
- **Auto-launch**: Installed packages start automatically on VM boot
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
- i3 automatically tiles windows
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
- `launch` - Launch specific application in VM via Xpra
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
```

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
# SSH server and Xpra automatically configured with passwordless authentication
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
Exec=xpra start ssh://user@192.168.122.X --start-child="flatpak run org.mozilla.firefox" --exit-with-children
Icon=firefox
Type=Application
Categories=Network;WebBrowser;
```

**Features enabled in .desktop files:**
- Passwordless SSH authentication (using host SSH keys)
- Seamless window mode (applications appear as native windows)
- Clipboard sharing (copy/paste between host and VM)
- Audio passthrough (PulseAudio/PipeWire)
- Dynamic resolution (adapts to your display)
- Exit-with-children (closes when application closes)

### Prerequisites on Host

Install Xpra client:
```bash
# Fedora/RHEL
sudo dnf install xpra

# Ubuntu/Debian
sudo apt install xpra

# Arch
sudo pacman -S xpra
```

**SSH Configuration:**
SSH authentication is configured automatically during VM provisioning:
- Host's SSH public key (`~/.ssh/id_rsa.pub`) is injected into VM
- VM's SSH host key is automatically added to `~/.ssh/known_hosts`
- No manual SSH setup or password entry required
- If you don't have SSH keys, generate them: `ssh-keygen -t rsa -b 4096`

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
- Applications should start automatically via i3 exec commands
- Check i3 config: `cat ~/.config/i3/config | grep "exec --no-startup-id"`
- Manual start: `DISPLAY=:0 flatpak run <app-id>` or `DISPLAY=:0 <system-app>`

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

### Xpra Connection Issues
- **SSH host key verification failed**: Automatically resolved during provisioning, but if you see this:
  ```bash
  # Manually accept the host key
  ssh-keyscan -H <vm-ip> >> ~/.ssh/known_hosts
  ```
- **Permission denied**: Ensure your SSH public key exists at `~/.ssh/id_rsa.pub`
- **Connection refused**: VM may still be booting, wait 30 seconds and retry

### Audio Issues with Xpra
- **Audio delay or sync problems**: Configuration is optimized automatically for <500ms latency
- **No audio**: Ensure PulseAudio/PipeWire is running on host: `pactl info`
- **Choppy audio**: Check network latency if using remote host
- **Manual tuning**: Audio settings are in `/etc/xpra/conf.d/60_seamless.conf` in VM

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

**Status**: This project provides a **fully functional VM isolation system** with complete auto-login, auto-launch, and seamless window integration. VMs display via SPICE viewer for console access, and **per-application seamless windows via Xpra** for true application isolation (WinApps/Qubes-style). Clipboard sharing, audio passthrough, and dynamic resolution all work seamlessly.
