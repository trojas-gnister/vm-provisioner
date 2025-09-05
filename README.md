# Container VM Provisioner 

A Rust-based tool that automatically provisions secure virtual machines running minimal Fedora Server with containerized applications. Provides enterprise-grade security through VM isolation, SELinux, and container sandboxing.

## Features

- 🔒 **Multi-layered Security**: KVM virtualization + SELinux + container sandboxing
- 🚀 **Automated Provisioning**: Unattended VM creation using Fedora kickstart
- 🐳 **Flexible Container Support**: Any container registry (Docker Hub, LinuxServer, etc.)
- 🔍 **Container Validation**: Validates container images exist before VM creation
- 🔥 **Automatic Firewall**: Dynamically configures firewall based on your ports
- 🖥️ **VNC Monitoring**: Monitor installation progress via VNC
- ⚙️ **Cross-Architecture**: Supports x86_64 and aarch64

## Quick Start

### Prerequisites

Install required tools:
```bash
# Fedora/RHEL
sudo dnf install libvirt virt-install qemu-img genisoimage

# Start libvirt service
sudo systemctl enable --now libvirtd
```

### Installation

```bash
git clone <repository-url>
cd container-vm-provisioner
cargo build --release
```

### Basic Usage

1. **Configure your VM** (creates `vm.env`):
```bash
./target/release/container-vm-provisioner configure
```

2. **Provision the VM**:
```bash
./target/release/container-vm-provisioner provision
```

3. **Monitor installation** via VNC:
```bash
vncviewer localhost:5900
```

4. **Check VM status**:
```bash
./target/release/container-vm-provisioner status
```

## Configuration

Edit `vm.env` to customize your setup:

```bash
# VM Hardware
VM_NAME=container-vm
VM_MEMORY_MB=4096
VM_VCPUS=2
VM_DISK_SIZE_GB=20

# Container Configuration
CONTAINER_REGISTRY=docker.io/linuxserver
CONTAINERS=librewolf:latest,qbittorrent:latest
CONTAINER_PORTS=3000:3000,3001:3001,8080:8080
FIREWALL_PORTS=22,3000,3001,8080,5900

# Server packages
DNF_PACKAGES=podman,podman-compose,qemu-guest-agent,git,curl,wget,htop,vim
```

## Container Validation

The tool validates containers before VM creation:

- **LinuxServer containers**: Validates against LinuxServer.io API
- **Docker Hub**: Validates against Docker Hub API  
- **Other registries**: Gracefully skips validation to avoid blocking

Example validation output:
```
🔍 Validating container images...
  ✓ docker.io/linuxserver/librewolf:latest
  ✓ docker.io/linuxserver/qbittorrent:latest
✅ All containers validated successfully
```

## Accessing Containers

### Via SSH Port Forwarding (Recommended)
```bash
# Forward container ports through SSH
ssh -L 3000:localhost:3000 -L 3001:localhost:3001 user@VM_IP

# Access in browser
firefox http://localhost:3000   # HTTP
firefox https://localhost:3001  # HTTPS (accept self-signed cert)
```

### Direct Access (if firewall configured)
```bash
# Access directly (requires firewall ports open)
firefox https://VM_IP:3001
```

## Commands

- `configure` - Interactive configuration setup
- `provision` - Create and provision new VM
- `start` - Start existing VM
- `stop` - Stop running VM  
- `status` - Show VM status and connection info
- `destroy` - Remove VM and cleanup

### Command Options

- `--env <path>` - Use specific .env file (default: ./vm.env)
- `--interactive` - Force interactive configuration
- `--yes, -y` - Skip confirmation prompts

## Examples

### LibreWolf Browser VM
```bash
# vm.env
CONTAINERS=librewolf:latest
CONTAINER_PORTS=3000:3000,3001:3001
FIREWALL_PORTS=22,3000,3001,5900
```

### Media Server VM  
```bash
# vm.env
CONTAINERS=plex:latest,qbittorrent:latest
CONTAINER_PORTS=32400:32400,8080:8080
FIREWALL_PORTS=22,32400,8080,5900
```

### Multi-Registry Setup
```bash
# Mix different registries
CONTAINER_REGISTRY=docker.io
CONTAINERS=nginx:latest,portainer/portainer-ce:latest
```

## Security

- **VM Isolation**: Hardware virtualization prevents container breakout
- **SELinux**: Mandatory access control enabled by default
- **Container Sandboxing**: Podman provides additional isolation
- **Minimal Attack Surface**: Server-only install, no desktop environment
- **Network Isolation**: VNC bound to localhost only

## Troubleshooting

### Container Not Loading
- Check firewall: `sudo firewall-cmd --list-ports`
- Verify container logs: `podman logs container-name`
- Try HTTPS instead of HTTP (port 3001 vs 3000)

### VM Won't Start
- Check libvirt status: `sudo systemctl status libvirtd`
- Verify VM definition: `virsh list --all`

### Installation Monitoring
- Connect via VNC: `vncviewer localhost:5900`
- Console access: `virsh console VM_NAME`

## Contributing

1. Fork the repository
2. Create feature branch
3. Make changes following Rust best practices
4. Test with `cargo test` and `cargo clippy`
5. Submit pull request

## License

[Add your license here]
