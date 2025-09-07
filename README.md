# VM Provisioner - Qubes-like Application Isolation System

A Rust-based application isolation system inspired by Qubes OS. Creates lightweight VMs for individual applications (browser, office, development) with seamless window integration where VM windows appear as native host windows.

## Features

- 🔒 **Application Isolation**: Each application runs in its own VM for security
- 📦 **Dynamic Package Installation**: Install any system or Flatpak packages on-demand
- 🚀 **Auto-Launch Applications**: Specified applications start automatically when VM boots
- 🔓 **Auto-Login**: Passwordless login with direct desktop access
- 🖥️ **kitty Terminal**: Always available in every VM for CLI tasks
- 🪟 **Window Proxy System**: TCP-based communication with comprehensive window event handling
- 📋 **Clipboard Sharing**: Secure clipboard sharing between host and VMs
- 🖥️ **i3 Window Manager**: Pure X11 environment with lightweight i3 for optimal compatibility
- 💾 **Password Management**: Centralized storage of VM credentials
- 🔧 **Cross-Architecture**: Supports x86_64 and aarch64

## Current Status

**✅ Implemented:**
- VM provisioning with Fedora + dynamic package installation
- System package installation (dnf-based) and Flatpak package support
- Auto-launch system with systemd services for specified applications
- Auto-login with i3 window manager (passwordless X11 access)
- kitty terminal emulator included by default in all VMs
- Advanced window proxy architecture with TCP communication (port 9999)
- X11 window detection using xwininfo/wmctrl
- Length-prefixed binary protocol for window events
- Comprehensive window event handling (8 message types)
- Wayland client framework with compositor integration
- Clipboard proxy with bidirectional sharing
- Centralized password storage and management

**🚧 In Progress:**
- Wayland surface instantiation (framework complete)
- Buffer sharing and graphics acceleration
- Input event forwarding from host to VM
- VirtIO channels for improved performance

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
# Firefox browser VM
./target/release/vm-provisioner create --flatpak org.mozilla.firefox

# LibreOffice + development tools
./target/release/vm-provisioner create --system libreoffice git --name office-vm

# Multiple applications with custom resources
./target/release/vm-provisioner create --flatpak com.spotify.Client --flatpak com.slack.Slack --memory 8192 --vcpus 4

# Custom VM name and packages
./target/release/vm-provisioner create --flatpak org.mozilla.firefox --system htop --name my-browser-vm
```

2. **Start the VM**:
```bash
./target/release/vm-provisioner start firefox-vm
# SPICE viewer will launch automatically
# Auto-login enabled - no password required
# Specified applications will launch automatically
```

3. **Manage VMs**:
```bash
# List all VMs
./target/release/vm-provisioner list

# Show all VM passwords (for console access if needed)
./target/release/vm-provisioner passwords

# Connect to VM console
./target/release/vm-provisioner console firefox-vm
# Use credentials: user / [generated-password]
# Note: SPICE viewer has auto-login, console needs password

# Stop VM
./target/release/vm-provisioner stop firefox-vm

# Destroy VM
./target/release/vm-provisioner destroy firefox-vm
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

### Basic i3 Shortcuts
- `Mod+Enter` - Open kitty terminal
- `Mod+d` - Open dmenu (application launcher)  
- `Mod+Shift+q` - Close focused window
- `Mod+1,2,3,4,5` - Switch to workspace 1-5
- `Mod+Shift+1,2,3,4,5` - Move window to workspace 1-5
- `Mod+Arrow Keys` - Change window focus
- `Mod+Shift+Arrow Keys` - Move focused window
- `Mod+Shift+r` - Restart i3
- `Mod+Shift+e` - Exit i3

**Note**: `Mod` key is typically the Windows/Super key

### Launching Applications
```bash
# Via dmenu (Mod+d)
# Type application name and press Enter

# Via terminal (Mod+Enter, then type):
firefox  # If installed as system package
flatpak run org.mozilla.firefox  # If installed as Flatpak
qbittorrent  # System package example
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

### SPICE Connection Issues
- Ensure VM is running: `virsh list`
- Check SPICE port: `virsh domdisplay VM_NAME`
- Install virt-viewer: `sudo dnf install virt-viewer`

### Clipboard Not Working
- Install wl-clipboard on host: `dnf install wl-clipboard`
- Verify SPICE agent in VM: `systemctl status spice-vdagentd`

### Performance Issues
- Enable KVM acceleration: Check `kvm-ok` or `/proc/cpuinfo`
- Increase VM memory: Use `--memory` option
- Enable hardware acceleration: Coming in future updates

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

**Note**: This project is under active development. The seamless window integration is not yet complete - VMs currently display via SPICE viewer. See `CLAUDE.md` for detailed development roadmap.