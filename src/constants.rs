//! Application-wide constants
//!
//! This module centralizes static configuration values used throughout
//! the vm-provisioner codebase. Dynamic values (like Fedora version)
//! remain in their respective modules.

// ============================================================================
// Paths
// ============================================================================

/// Default directory for VM disk images
pub const DEFAULT_VM_DIR: &str = "/var/lib/libvirt/images";

/// Config directory name relative to $HOME
pub const CONFIG_DIR_NAME: &str = ".config/vm-provisioner";

/// Password file name
pub const PASSWORD_FILE_NAME: &str = "vm-passwords.toml";

// ============================================================================
// Libvirt
// ============================================================================

/// Default libvirt connection URI for QEMU/KVM system VMs
/// Note: Also defined in virsh.rs - kept here for non-virsh usage
pub const QEMU_URI: &str = "qemu:///system";

/// Default VNC port for fallback graphics
pub const DEFAULT_VNC_PORT: u16 = 5900;

/// Default SPICE port
pub const DEFAULT_SPICE_PORT: u16 = 5900;

// ============================================================================
// Installation
// ============================================================================

/// Minimum RAM (MB) required during installation
/// Can be reduced after first boot
pub const MIN_INSTALL_MEMORY_MB: u64 = 4096;

/// Minimum disk size (MB) to consider installation successful
pub const MIN_DISK_SIZE_MB: u64 = 500;

/// Default disk size for new VMs (GB)
pub const DEFAULT_DISK_SIZE_GB: u64 = 20;

/// Default memory for new VMs (MB)
pub const DEFAULT_MEMORY_MB: u64 = 2048;

/// Default vCPU count for new VMs
pub const DEFAULT_VCPUS: u32 = 2;

// ============================================================================
// Retry Configuration
// ============================================================================

/// Number of retries when waiting for SSH to become available
pub const SSH_RETRY_COUNT: u32 = 30;

/// Delay between SSH retry attempts (seconds)
pub const SSH_RETRY_DELAY_SECS: u64 = 2;

/// Number of retries when waiting for VM to boot
pub const VM_BOOT_RETRY_COUNT: u32 = 6;

/// Delay between VM boot checks (seconds)
pub const VM_BOOT_RETRY_DELAY_SECS: u64 = 5;

/// General wait time for VM to stabilize (seconds)
pub const VM_BOOT_WAIT_SECS: u64 = 5;

/// Wait time after installation reboot (seconds)
pub const POST_INSTALL_WAIT_SECS: u64 = 10;

/// Wait time for graceful shutdown (seconds)
pub const SHUTDOWN_WAIT_SECS: u64 = 30;

// ============================================================================
// Security
// ============================================================================

/// Length of generated passwords
pub const PASSWORD_LENGTH: usize = 16;

/// Character set for password generation
pub const PASSWORD_CHARSET: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Maximum VM name length
pub const MAX_VM_NAME_LENGTH: usize = 64;

// ============================================================================
// Default User
// ============================================================================

/// Default username inside VMs
pub const DEFAULT_USER_NAME: &str = "user";

/// Default user UID inside VMs
pub const DEFAULT_USER_UID: u32 = 1000;

/// Default PulseAudio socket path (using default UID)
pub const DEFAULT_PULSE_SOCKET: &str = "/run/user/1000/pulse/native";

// ============================================================================
// Hot-plug Timing
// ============================================================================

/// Delay after unbinding device before rebinding (milliseconds)
pub const DEVICE_UNBIND_DELAY_MS: u64 = 500;

/// Delay after device detach before driver rebind (milliseconds)
pub const DEVICE_DETACH_DELAY_MS: u64 = 500;
