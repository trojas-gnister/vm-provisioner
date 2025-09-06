# VM Provisioner - Qubes-like Application Isolation System

A Rust-based application isolation system inspired by Qubes OS. Creates lightweight VMs for individual applications (browser, office, development) with seamless window integration where VM windows appear as native host windows.

## Features

- 🔒 **Application Isolation**: Each application runs in its own VM for security
- 🪟 **Seamless Windows**: VM application windows appear as native host windows *(in development)*
- 📋 **Clipboard Sharing**: Secure clipboard sharing between host and VMs via SPICE
- 🚀 **Direct Installation**: Applications install directly (no containers)
- 🖥️ **SPICE Integration**: Full graphics, audio, and input support
- ⚙️ **Minimal Overhead**: Uses Cage compositor for single-app focus
- 🔧 **Cross-Architecture**: Supports x86_64 and aarch64

## Current Status

**✅ Working Now:**
- VM provisioning with Fedora + application installation
- SPICE viewer with clipboard sharing
- LibreWolf browser template
- Audio passthrough

**🚧 In Development:**
- Seamless window proxy (VM windows → native host windows)
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

1. **Create a LibreWolf VM**:
```bash
# Interactive mode
./target/release/vm-provisioner create

# From template
./target/release/vm-provisioner create --template librewolf

# Non-interactive
./target/release/vm-provisioner create --template librewolf -y
```

2. **Start the VM**:
```bash
./target/release/vm-provisioner start librewolf-vm
# SPICE viewer will launch automatically
# Login credentials will be displayed
```

3. **Manage VMs**:
```bash
# List all VMs
./target/release/vm-provisioner list

# Show all VM passwords
./target/release/vm-provisioner passwords

# Connect to VM console
./target/release/vm-provisioner console librewolf-vm
# Use credentials: user / [generated-password]

# Stop VM
./target/release/vm-provisioner stop librewolf-vm

# Destroy VM
./target/release/vm-provisioner destroy librewolf-vm
```

## VM Templates

### LibreWolf Browser
- **Application**: LibreWolf (privacy-focused Firefox)
- **Graphics**: Wayland with hardware acceleration  
- **Features**: Clipboard sharing, audio passthrough
- **Use case**: Isolated web browsing

### Office (Planned)
- **Application**: LibreOffice suite
- **Features**: Document editing in isolation
- **Use case**: Secure document handling

### Development (Planned)
- **Applications**: VS Code, Rust/Python toolchain
- **Features**: Isolated development environment
- **Use case**: Secure code development

## Configuration

VM configurations and passwords are automatically stored:

### Individual VM Config
```toml
# ~/.config/vm-provisioner/librewolf-vm.toml
[vm]
name = "librewolf-vm"
memory_mb = 4096
vcpus = 2
disk_size_gb = 20
graphics_backend = "VirtioGpu"
enable_clipboard = true
enable_audio = true
user_password = "abc123generated"

[app]
type = "Browser"
command = "/usr/bin/librewolf"
profile_path = "/home/user/.librewolf"

[packages]
system = ["cage", "wayland", "pipewire", "wl-clipboard"]
app = ["librewolf"]
```

### Centralized Password Storage
```toml
# ~/.config/vm-provisioner/vm-passwords.toml
[vms]
librewolf-vm = "abc123generated"
work-browser = "def456different"
banking-vm = "xyz789another"
```

## Architecture

```
┌─────────────────┐    ┌─────────────────┐
│     Host OS     │    │   VM (Fedora)   │
│                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │ Window      │◄┼────┼─┤ Cage        │ │
│ │ Proxy       │ │    │ │ Compositor  │ │
│ │             │ │    │ │             │ │
│ │ ┌─────────┐ │ │    │ │ ┌─────────┐ │ │
│ │ │ Native  │ │ │    │ │ │LibreWolf│ │ │
│ │ │ Window  │ │ │    │ │ │         │ │ │
│ │ └─────────┘ │ │    │ │ └─────────┘ │ │
│ └─────────────┘ │    │ └─────────────┘ │
└─────────────────┘    └─────────────────┘
         ▲                       │
         │      SPICE Protocol   │
         └───────────────────────┘
```

## Current Implementation: SPICE Viewer

**How it works now:**
1. VM runs LibreWolf in Cage compositor (single-app mode)
2. SPICE server streams VM display to host
3. `remote-viewer` shows VM in separate window
4. Clipboard automatically syncs between host and VM
5. Audio passes through to host speakers

**Commands:**
```bash
# Start VM and launch SPICE viewer
vm-provisioner start librewolf-vm

# Manual connection
remote-viewer spice://127.0.0.1:5900
```

## Future: Seamless Window Integration

**Development roadmap:**
1. **Window Proxy** (`src/window_proxy.rs`): Intercept SPICE graphics and create native Wayland windows
2. **Guest Agent** (`src/guest_agent.rs`): Detect application windows within VM
3. **SPICE Bridge**: Parse SPICE display commands to extract window regions

**Result**: LibreWolf windows appear as native host windows, indistinguishable from host applications.

## Security

- **VM Isolation**: Hardware virtualization prevents application breakout
- **SELinux**: Mandatory access control enabled in guest
- **Minimal Attack Surface**: Guest OS has only necessary packages
- **Network Isolation**: VMs use NAT by default
- **Clipboard Security**: Controlled sharing via SPICE protocol

## Commands

- `create` - Create new application VM
- `start` - Start VM and launch viewer  
- `stop` - Stop running VM
- `list` - Show all VMs and their status
- `passwords` - Show login credentials for all VMs
- `destroy` - Remove VM and cleanup
- `console` - Connect to VM console

### Command Options

- `--template <name>` - Use predefined template (librewolf, office, dev)
- `--config <path>` - Use custom configuration file
- `--yes, -y` - Skip confirmation prompts
- `--memory <mb>` - Override memory allocation
- `--vcpus <n>` - Override CPU allocation

## Examples

### Create Browser VM with Custom Resources
```bash
vm-provisioner create --template librewolf --memory 8192 --vcpus 4 -y
# Displays: Username: user, Password: [generated]
```

### Multiple Isolated Browsers
```bash
# Personal browsing
vm-provisioner create --template librewolf --name personal-browser

# Work browsing  
vm-provisioner create --template librewolf --name work-browser

# Banking (separate VM for financial sites)
vm-provisioner create --template librewolf --name banking-browser

# View all passwords
vm-provisioner passwords
```

### Password Management
```bash
# Show all VM credentials
vm-provisioner passwords

# Manual password lookup
cat ~/.config/vm-provisioner/vm-passwords.toml

# Start VM (displays password)
vm-provisioner start librewolf-vm
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