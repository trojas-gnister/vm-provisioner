# VM Provisioner - Qubes-like Application Isolation System

A Rust-based application isolation system inspired by Qubes OS. Creates lightweight VMs with dynamic package installation (system + Flatpak) featuring auto-login, auto-launch applications, i3 window manager, SPICE integration with auto-resize functionality, and comprehensive Flatpak support.

## Features

- 🔒 **Application Isolation**: Each application runs in its own VM for security
- 📦 **Dynamic Package Installation**: Install any system (dnf) or Flatpak packages on-demand
- 🚀 **Auto-Launch Applications**: Specified applications start automatically when VM boots
- 🔓 **Auto-Login**: Passwordless login with i3 window manager and desktop access
- 🖥️ **kitty Terminal**: Default terminal emulator included in every VM
- 🪟 **SPICE Integration**: Auto-resize functionality with comprehensive window management
- 📋 **Clipboard Sharing**: Bidirectional clipboard sharing between host and VMs via SPICE
- 🖥️ **i3 Window Manager**: Lightweight tiling window manager with full X11 compatibility
- 🚀 **Application Launcher**: rofi with complete Flatpak integration and discovery
- 💾 **Password Management**: Centralized storage and individual VM credential management
- 🔧 **Cross-Architecture**: Full support for x86_64 and aarch64 (ARM64)
- 📏 **Auto-Resize**: Dynamic resolution adjustment using spice-autorandr (ARM64 compatible)

## Current Status

**✅ Fully Working System:**
- **Complete VM isolation**: Each application runs in its own secure VM
- **Dynamic package installation**: Install any system (dnf) or Flatpak packages on-demand
- **Auto-login & auto-launch**: Passwordless login with applications starting automatically
- **SPICE integration**: Full clipboard sharing and auto-resize functionality working on ARM64/x86_64
- **Robust package management**: All critical packages (xset, i3, kitty, etc.) install correctly
- **Cross-architecture support**: ARM64 and x86_64 compatibility verified and working
- **Advanced CLI**: --system and --flatpak flags for dynamic VM creation
- **Application launcher**: rofi with full Flatpak integration and discovery
- **Comprehensive logging**: Detailed installation logs for troubleshooting
- **VM lifecycle management**: create/start/stop/destroy/list/passwords all working
- **Centralized password storage**: Secure credential management system

**🚧 Future Enhancements:**
- Seamless window integration (framework complete, needs surface instantiation)
- VirtIO channels for improved performance
- GPU passthrough for hardware acceleration

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
./target/release/vm-provisioner start media-vm
# SPICE viewer launches automatically with i3 window manager
# Auto-login enabled - no password required  
# Applications auto-launch on boot (LibreWolf + qBittorrent)
# Auto-resize works when you resize virt-viewer window
# Clipboard sharing enabled between host and VM
# Use Mod+d for rofi launcher, Mod+Enter for terminal
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

## Current Implementation: Window Proxy System

**How it works:**
1. VM runs applications in pure X11 environment with i3 window manager and auto-login
2. SPICE agent provides automatic resolution adjustment and clipboard sharing
3. kitty terminal and specified packages (system + Flatpak) are auto-installed
4. Auto-launch systemd services start specified applications on boot
5. Guest agent monitors X11 windows using xwininfo/wmctrl
6. Window events (8 types: create/destroy/resize/move/focus/title) sent to host via TCP
7. Host window proxy receives events with length-prefixed binary protocol
8. Wayland client framework processes events and creates native windows
9. Clipboard synchronized bidirectionally with SPICE and wl-clipboard integration

**Window Detection Flow:**
```
VM Boot: Auto-login + auto-launch applications → X11 windows created
    ↓
Guest Agent: xwininfo detects new windows
    ↓  
Guest Agent: Serializes WindowMessage::WindowCreated for each
    ↓
TCP:9999: Sends length-prefixed binary data to host
    ↓
Host Proxy: Receives and deserializes messages  
    ↓
Wayland Client: Processes events and creates native windows
```

**Commands:**
```bash
# Start VM with window proxy
vm-provisioner start firefox-vm
# SPICE viewer opens with i3 window manager
# Auto-login enabled - no password needed
# Applications auto-launch on boot
# Use Mod+Enter for terminal, Mod+d for app launcher

# Manual guest agent (inside VM) - connects to host TCP:9999
/usr/local/bin/guest-agent
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

### Command Options

**VM Creation:**
- `--name <name>` - Custom VM name (auto-generated if not provided)
- `--system <pkg>` - System packages to install (can be used multiple times)
- `--flatpak <pkg>` - Flatpak packages to install (can be used multiple times)
- `--memory <mb>` - Memory allocation in MB (default: 4096)
- `--vcpus <n>` - Number of virtual CPUs (default: 2)
- `--disk <gb>` - Disk size in GB (default: 20)
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
# Full development setup
vm-provisioner create --flatpak com.visualstudio.code --system git gcc rust cargo python3 nodejs npm --name dev-env --memory 8192 --disk 40

# Quick Python development
vm-provisioner create --system python3 python3-pip git --flatpak com.visualstudio.code --name python-dev
```

### Media & Productivity
```bash
# Media editing suite
vm-provisioner create --flatpak org.kde.kdenlive --flatpak org.gimp.GIMP --system audacity --memory 8192 --name media-vm

# Office suite with extras
vm-provisioner create --system libreoffice --flatpak com.slack.Slack --flatpak org.telegram.desktop --name office-vm
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

### Contributing to Window Proxy
See `CLAUDE.md` for detailed development tasks and architecture decisions.

---

**Status**: This project provides a **fully functional VM isolation system** with complete auto-login, auto-launch, and auto-resize capabilities. VMs currently display via SPICE viewer with seamless clipboard sharing and dynamic resolution adjustment. Future seamless window integration framework is in place.
