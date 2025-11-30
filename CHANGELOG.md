# Changelog

All notable changes to this project will be documented in this file.

## [1.3.0] - 2025-11-29

### Added
- **Library support**: vm-provisioner can now be used as a Rust library dependency via GitHub
  - Add to your project: `vm-provisioner = { git = "https://github.com/trojas-gnister/vm-provisioner" }`
- **`AppVMConfigBuilder`**: Builder pattern for ergonomic VM configuration
  - Fluent API with sensible defaults
  - Methods: `memory_mb()`, `vcpus()`, `add_system_package()`, `headless()`, `no_network()`, etc.
- **`custom_kickstart` field**: Inject custom setup scripts into VM provisioning
  - Scripts inserted before the final cleanup/reboot in kickstart files
  - Useful for custom service configuration, package setup, etc.
- **IOMMU helper functions**: Standalone functions for PCI passthrough preparation
  - `check_iommu_enabled()` - Check if IOMMU is enabled on the system
  - `get_iommu_group(address)` - Get IOMMU group for a PCI device
  - `list_iommu_group_devices(group)` - List all devices in an IOMMU group
  - `is_clean_iommu_group(group)` - Check if group has single device (safe for passthrough)
- **Re-exported traits**: `Installation`, `Lifecycle`, `PciPassthrough`, `UsbPassthrough` now available from crate root
- **Cargo.toml metadata**: Added `repository`, `readme`, `keywords`, and `categories` fields

### Changed
- **`AppVMProvisioner.config` is now public**: Library consumers can access/modify VM configuration
- **lib.rs documentation**: Added library usage example in module docs

## [1.2.0] - 2025-11-27

### Security
- **Fixed insecure password generation**: Replaced `DefaultHasher` (predictable) with cryptographically secure `rand::thread_rng()` for VM user passwords

### Fixed
- **Fixed Result double-consume bug**: Corrected `if result.is_err() || !result?.success()` pattern in PCI/USB passthrough code that could cause panics

### Added
- **VM name validation**: New `AppVMConfig::validate_vm_name()` function prevents path traversal, shell injection, and invalid characters
- **Config loading helpers**: New `AppVMConfig::load()`, `config_path()`, and `config_dir()` methods reduce code duplication
- **IP address validation**: Added validation before shell interpolation in xpra_manager.rs
- **Template-based kickstart generation**: Extracted 11 shell script templates to `src/templates/` module
  - `vsock_relay.sh` - Vsock configuration for network-disabled VMs
  - `audio_ssh.sh` / `audio_web.sh` - Audio configurations
  - `ssh_xpra_base.sh` - Main SSH and Xpra setup
  - `selkies_*.sh` - Selkies-GStreamer web streaming components
  - `virtiofs_*.sh` - Shared folder mounting
- **Unit tests**: Added 9 new tests for VM name validation, password generation, and network modes

### Changed
- **Refactored `create_vm()`**: Split 500+ line function into focused helpers:
  - `CreateOptions` struct for parameter grouping
  - `validate_create_options()` for input validation
  - `build_config()` for configuration construction
  - `display_config_summary()` for output formatting
  - `save_config_and_passwords()` for persistence
  - `handle_post_provisioning()` for post-install tasks
- **Improved share path validation**: Enhanced directory existence and absolute path checks
- **Reduced xpra_manager.rs**: Removed ~300 lines by extracting templates

### Removed
- **Dead VPN code**: Removed unused `VpnConfig` struct, `vpn_config` field, and `NetworkMode::VpnOnly` variant

## [1.1.0] - 2025-11-26

### Changed
- **Major codebase refactoring**: Split monolithic `provisioner.rs` (1900+ lines) into focused modules:
  - `provisioner/device_detection.rs` - PCI/USB device detection
  - `provisioner/pci.rs` - PCI passthrough management
  - `provisioner/usb.rs` - USB passthrough management
  - `provisioner/network.rs` - Network interface management
  - `provisioner/lifecycle.rs` - VM start/stop/destroy operations
  - `provisioner/kickstart.rs` - Fedora kickstart generation
  - `provisioner/installation.rs` - VM provisioning orchestration
- **Improved error handling**: Replaced generic `Box<dyn Error>` with typed error hierarchy using `thiserror`
  - `VmProvisionerError` top-level enum with module-specific error types
  - `ConfigError`, `ProvisioningError`, `PciError`, `UsbError`, `DisplayError`, `NetworkError`
- **Structured logging**: Migrated from `println!`/`eprintln!` to `log` + `env_logger` with emoji-prefixed output
- **Thread safety**: Replaced `RefCell` with `OnceLock` in `XpraManager` for lazy initialization
- **README rewrite**: Reorganized for clarity with quick start guide, command reference table, and troubleshooting table

### Removed
- Async/await code (no actual async operations were being performed)
- Unused dependencies: `anyhow`, `regex`, `hostname`, `bincode`
- `tokio` runtime dependency

## [1.0.0] - 2025-11-25

### Added
- **Network-disabled VMs with virtio-vsock**: New `--no-network` flag creates airgapped VMs that use virtio-vsock for display forwarding instead of TCP/IP networking
  - VM has no network interface for maximum isolation
  - Display forwarding via xpra over SSH tunneled through vsock
  - Audio forwarding via PulseAudio SSH tunnel (playback only)
  - Requires `vhost_vsock` kernel module on host and `socat` on both host and guest
  - Auto-assigns vsock CID via libvirt and stores in VM config
- Validation to prevent incompatible flag combinations (`--no-network` + `--web-port`, `--no-network` + `--network-bridge`)
- Network mode displayed in VM configuration output during creation
- Automatic SSH key cleanup on VM destroy to prevent host key conflicts

### Changed
- Xpra commands now use `GDK_BACKEND=x11` for better Wayland host compatibility
- Added `--modal-windows=yes` to xpra for improved input handling on Wayland

### Removed
- Microphone forwarding via SSH tunnel (caused audio feedback). Use USB passthrough for audio input devices instead.

## [0.9.0] - 2025-11-22

### Added
- **Selkies-GStreamer web streaming**: New `--web-port` flag enables browser-based access via WebRTC
  - H.264 video encoding with x264enc
  - Opus audio codec
  - Built-in clipboard support
  - Basic HTTP authentication
- **Bridged networking**: New `--network-bridge` flag for LAN-accessible VMs
  - VM gets IP directly from LAN DHCP
  - Useful for accessing web streaming from mobile devices

### Changed
- Improved Selkies integration with auto-launching flatpak applications

## [0.8.0] - 2025-11-20

### Added
- **Shared folders via virtiofs**: New `--share` flag for host-guest filesystem sharing
  - Format: `--share /host/path:/guest/mount/path`
  - Read-only option: `--share-readonly`
  - Multiple folders supported (repeatable flag)
- **USB device hot-plugging**: New `--usb-hotplug` flag
  - Devices attached only while VM runs
  - Automatically restored to host on VM stop

### Changed
- Virtiofs requires memory backing (`memfd`) for shared memory

## [0.7.0] - 2025-11-18

### Added
- **USB device passthrough**: New `--usb` flag
  - Format: `--usb vendor:product` (e.g., `--usb 046d:c52b`)
  - Supports multiple devices (repeatable flag)
  - Auto-detection of device description from lsusb

### Changed
- USB controller uses `qemu-xhci` model for better compatibility

## [0.6.0] - 2025-11-15

### Added
- **PCI device passthrough**: New `--pci` flag for GPU and other PCI devices
  - IOMMU group detection
  - Driver unbinding/rebinding support
  - Hot-plug option with `--pci-hotplug`

## [0.5.0] - 2025-11-12

### Added
- **Xpra native display forwarding**: Seamless window integration via SSH
  - Individual application windows appear on host desktop
  - No persistent compositor required
  - X server starts on-demand per session
- **SSH PulseAudio tunnel**: One-way audio forwarding for speaker output
  - Uses `module-tunnel-sink-new` in guest
  - Low-latency playback via SSH reverse tunnel

### Removed
- Waypipe support (deprecated due to provisioning bugs and Getty service crashes)

## [0.4.0] - 2025-11-08

### Added
- **Automatic desktop file generation**: `vm-provisioner generate-shortcuts <vm>`
  - Creates `.desktop` files for installed applications
  - Integrates with host application menus
  - Wrapper scripts for clean xpra session management

## [0.3.0] - 2025-11-05

### Added
- **Flatpak support**: New `--flatpak` flag for sandboxed application installation
  - Flathub repository auto-configured
  - `--grant-device-access` for webcam/audio device access

### Changed
- Package installation split between system packages and flatpaks

## [0.2.0] - 2025-11-02

### Added
- **Headless mode**: New `--headless` flag for CLI-only VMs
  - No GUI packages installed
  - Serial console access only
- **VM lifecycle commands**: `start`, `stop`, `destroy`, `list`, `console`
- **Password management**: `passwords` command to retrieve VM credentials

## [0.1.0] - 2025-10-28

### Added
- Initial release
- Fedora 41 guest VM provisioning via kickstart
- libvirt/KVM backend with virt-install
- Configurable memory, vCPUs, and disk size
- NAT networking on libvirt default bridge
- SSH key injection for passwordless authentication
- TOML-based VM configuration storage
