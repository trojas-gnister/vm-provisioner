# Changelog

All notable changes to this project will be documented in this file.

## [1.4.0] - 2026-02-08

### Added
- **NixOS-based provisioning**: Replaced Fedora kickstart pipeline with declarative NixOS configuration generation and `nixos-generators` qcow2 image building
  - `src/nixos/config_gen.rs` generates a complete `configuration.nix` from `AppVMConfig`
  - `src/nixos/image_builder.rs` builds qcow2 images via `nixos-generate` with auto-resizing
  - `src/nixos/packages.rs` maps Fedora/RPM package names to nixpkgs equivalents
  - Nix expression validation via `rnix` crate before building
  - SHA-512 password hashing via `mkpasswd`/`openssl` for NixOS `hashedPassword`
- **Interactive TUI**: Running `vm-provisioner` with no subcommand launches a ratatui-based terminal UI
  - VM dashboard with status, IP, and configuration overview
  - Create form with all configuration options (name, memory, vCPUs, disk, packages, graphics, network)
  - Start/stop/destroy operations with confirmation dialogs
  - Live provisioning log viewer
  - VM detail view
  - New dependencies: `ratatui` 0.30
- **Automated Venus Vulkan setup**: GUI VMs now automatically get hardware-accelerated Vulkan via virtio-gpu Venus
  - Host GPU render node detection via sysfs (`/sys/class/drm/renderD*`)
  - Automatic GPU selection for Venus (AMD > Intel; NVIDIA skipped — no Venus support)
  - Stable `/dev/dri/by-path/` render node paths for SPICE configuration
  - `VK_ICD_FILENAMES` environment variable set on QEMU process for correct host Vulkan ICD selection
  - QEMU `qemu:commandline` post-processing: `venus=true`, `blob=true`, memory-backend-memfd
  - `libvirt-qemu` user group membership check for `render` and `video` groups
- **NixOS guest Venus configuration**: Generated `configuration.nix` now includes:
  - `boot.kernelPackages = pkgs.linuxPackages_latest` (kernel >= 6.13 required for Venus)
  - `hardware.graphics` block with Mesa, vulkan-loader, and 32-bit support
  - `environment.variables` with `MESA_LOADER_DRIVER_OVERRIDE=virtio_gpu` and `VK_DRIVER_FILES` pointing to Venus ICD
- **NixOS guest PipeWire audio**: GUI VMs get PipeWire with ALSA and PulseAudio compatibility
- **NixOS guest QEMU agent**: `services.qemuGuest.enable = true` for host-guest communication
- **New library exports**: `detect_gpu_render_nodes()`, `select_gpu_for_venus()`, `GpuVendor`, `GpuRenderNode`
- **GPU vendor constants**: `GPU_VENDOR_AMD`, `GPU_VENDOR_INTEL`, `GPU_VENDOR_NVIDIA`, `VULKAN_ICD_DIR` in `constants.rs`
- **NixOS channel constant**: `NIXOS_CHANNEL` in `constants.rs`
- **virsh improvements**:
  - `get_vm_ip()` now falls back to QEMU guest agent for bridged networks (`--source agent`)
  - `parse_ip_from_domifaddr_agent()` parses agent output, skipping loopback and link-local
  - `define()` function for defining VMs from XML files
- **New error variants**: `ProvisioningError::NixBuildFailed`, `ProvisioningError::NixConfigInvalid`

### Changed
- **CLI subcommand is now optional**: No subcommand launches the TUI; all existing commands still work as subcommands
- **Prerequisites check**: Now checks for `nixos-generate` instead of `qemu-img`; install hints are distribution-agnostic
- **GraphicsBackend**: Removed `QxlSpice` variant (migrated to `VirtioGpu` on config load); only `VirtioGpu` and `VncOnly` remain
- **`custom_kickstart` field renamed to `custom_nix_config`**: Now accepts Nix configuration snippets instead of shell scripts
- **`--no-network` mode**: No longer implies vsock display forwarding; now creates a headless CLI-only VM accessible via `virsh console`
- **Dependencies**: Added `ratatui` 0.30, `rnix` 0.11, `tempfile` 3 (moved from dev-dependencies); removed `mockall`, `regex` dev-dependencies

### Fixed
- **Memory-backend-memfd sizing**: Now uses KiB (`memory_mb * 1024`) to match libvirt's `<memory>` unit (was incorrectly using MB)

### Removed
- **Fedora/kickstart provisioning**: Removed `src/provisioner/kickstart.rs`, ISO download, `fedora_version` config field
- **PCI passthrough**: Removed `src/provisioner/pci.rs`, `PciDevice` struct, `PciError`, `PciPassthrough` trait, IOMMU helpers, `--pci` and `--pci-hotplug` CLI flags
- **Xpra display bridge**: Removed `src/display_bridge.rs`, `src/xpra_manager.rs`, `XpraManager`, `DisplayBridge` trait
- **Display protocol system**: Removed `DisplayProtocol` enum (Xpra/Waypipe/Selkies migration logic)
- **Shell templates**: Removed entire `src/templates/` directory (audio, vsock relay, virtiofs, Selkies scripts)
- **CLI shortcuts**: Removed `src/cli/shortcuts.rs`, `generate-shortcuts`, `launch`, and `apps` commands
- **`--web-port` flag**: Selkies-GStreamer web streaming removed
- **Tests**: Removed `tests/display_bridge_tests.rs`, `tests/e2e_testing_guide.rs`, `tests/template_placeholder_tests.rs`, PCI/XML unit tests
- **`test-xpra.sh`**: Removed Xpra testing script
- **Timing constants**: Removed `DEVICE_UNBIND_DELAY_MS`, `DEVICE_DETACH_DELAY_MS` (PCI-related)
- **Installation constants**: Removed `MIN_INSTALL_MEMORY_MB`, `POST_INSTALL_WAIT_SECS`, `SHUTDOWN_WAIT_SECS` and related retry constants (no longer needed with NixOS image-based provisioning)

### Known Limitations
- **D3D12 Feature Level 12_2**: Not yet supported by Venus. Games requiring FL 12_2 (e.g. FF7 Rebirth) will fail. Waiting on Mesa 26.0 (expected late Feb 2026) which adds mesh shader support to Venus.
- **NVIDIA GPUs**: Venus requires open-source Vulkan drivers (RADV or ANV). NVIDIA is not supported.

## [1.3.1] - 2025-12-28

### Fixed
- **Fixed post-install validation race condition**: VM validation now handles the automatic reboot after installation
  - Added 10s initial wait for VM to stabilize after installation
  - Retry loop (up to 6 attempts with 5s delays) checks VM state
  - Properly handles transitional states during reboot
  - Prevents false "VM won't boot" errors when VM is actually booting

- **Fixed VM shortcuts with dynamic IP resolution**: Launch scripts now resolve VM IP at runtime
  - Shortcuts work even when created before VM is fully booted
  - Script auto-starts VM if not running when shortcut is clicked
  - Uses `notify-send` to display user-friendly errors
  - Removes hardcoded IP addresses from generated scripts

### Internal Improvements (Major Refactoring)

- **New CLI module structure** (`src/cli/`): Extracted all CLI handling from main.rs
  - `mod.rs` - CLI struct, Commands enum, main dispatch logic
  - `create.rs` - VM creation command with CreateOptions, validation, config building
  - `vm_ops.rs` - start, stop, destroy, list, passwords, console commands
  - `shortcuts.rs` - Desktop shortcut generation and app launching
  - `usb.rs` - USB attach/detach command handlers
  - **main.rs reduced from ~847 lines to ~21 lines**

- **New `src/virsh.rs` module**: Centralized all virsh/libvirt interactions
  - Command builders: `virsh_command()`, `virsh_sudo_command()`
  - Checked execution: `run_checked()`, `run_sudo_checked()` with proper error handling
  - Unchecked variants: `run_sudo_unchecked()`, `destroy_unchecked()`
  - High-level operations: `attach_device()`, `detach_device()`, `dumpxml()`, `domain_exists()`
  - VM state helpers: `get_vm_ip()`, `get_vm_state()`, `is_vm_running()`, `get_display()`
  - Eliminated ~60+ duplicated virsh command patterns across codebase

- **New `src/libvirt_xml.rs` module**: XML generation for libvirt devices
  - `hostdev_pci()` / `hostdev_pci_from_address()` - PCI passthrough XML
  - `hostdev_usb()` / `hostdev_usb_from_ids()` - USB passthrough XML
  - `interface_network()` / `interface_bridge()` - Network interface XML
  - `PciAddress` struct with parsing from "0000:01:00.0" format
  - Unit tests for all XML generation functions

- **New `src/constants.rs` module**: Centralized static configuration values
  - Path constants: `DEFAULT_VM_DIR`, `CONFIG_DIR_NAME`, `PASSWORD_FILE_NAME`
  - Timing constants: `SSH_RETRY_COUNT`, `SSH_RETRY_DELAY_SECS`, `VM_BOOT_*` values
  - Installation: `MIN_INSTALL_MEMORY_MB`, `POST_INSTALL_WAIT_SECS`
  - Device delays: `DEVICE_UNBIND_DELAY_MS`, `DEVICE_DETACH_DELAY_MS`

- **New `src/passwords.rs` module**: Extracted VMPasswords from main.rs
  - `VMPasswords` struct with HashMap storage
  - CRUD operations: `add_vm()`, `get()`, `remove()`, `contains()`
  - File persistence: `load_or_create()`, `save()`
  - Iterator support: `iter()`, `is_empty()`

- **Provisioner modules updated to use centralized helpers**:
  - `installation.rs` - Uses virsh helpers and constants for retries/timeouts
  - `lifecycle.rs` - Uses `virsh::start()`, `virsh::shutdown()`, `virsh::get_vm_ip()`
  - `pci.rs` - Uses `libvirt_xml::hostdev_pci()`, `virsh::attach_device()`
  - `usb.rs` - Uses `libvirt_xml::hostdev_usb()`, `virsh::attach_device()`
  - `network.rs` - Uses `virsh::dumpxml()`, `libvirt_xml::interface_network()`
  - `device_detection.rs` - Uses virsh helpers for vsock CID retrieval

- **Path construction improvements**: Replaced `format!()` string concatenation with `Path::join()`
  - `config.rs`: `config_path()`, `config_dir()`
  - `installation.rs`: ISO path, disk path construction
  - `passwords.rs`: `get_config_dir()`

- **New test file**: `tests/template_placeholder_tests.rs`
  - Tests for template placeholder substitution
  - Verifies all placeholders are replaced correctly

- **DRY improvements**: Extracted `SSH_BASE_OPTIONS` constant for SSH connection parameters
  - Removes duplicate SSH option strings across shortcut generation

---

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
