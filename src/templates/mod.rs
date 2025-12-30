//! Kickstart configuration templates for VM provisioning.
//!
//! This module provides shell script templates used during VM installation
//! via kickstart. Templates use `{placeholder}` syntax for variable substitution
//! via `.replace()` chains.

/// Vsock relay configuration for network-disabled VMs (no placeholders)
pub const VSOCK_RELAY: &str = include_str!("vsock_relay.sh");

/// Audio configuration for web streaming mode (no placeholders)
pub const AUDIO_WEB: &str = include_str!("audio_web.sh");

/// Audio configuration for SSH/Xpra mode (no placeholders)
pub const AUDIO_SSH: &str = include_str!("audio_ssh.sh");

/// Base SSH and Xpra configuration template
///
/// Placeholders: `{ssh_key}`, `{audio_config}`, `{web_streaming_config}`,
/// `{virtiofs_config}`, `{vsock_config}`
pub const SSH_XPRA_BASE: &str = include_str!("ssh_xpra_base.sh");

/// Selkies-GStreamer header configuration
///
/// Placeholders: `{port}`, `{systemd_services}`, `{systemd_enable_commands}`
pub const SELKIES_HEADER: &str = include_str!("selkies_header.sh");

/// Selkies wrapper script template
///
/// Placeholders: `{port}`
pub const SELKIES_WRAPPER: &str = include_str!("selkies_wrapper.sh");

/// Selkies systemd service and Openbox configuration
///
/// Placeholders: `{port}`, `{password}`, `{menu_items}`
pub const SELKIES_SERVICE: &str = include_str!("selkies_service.sh");

/// Virtiofs shared folders header (no placeholders)
pub const VIRTIOFS_HEADER: &str = include_str!("virtiofs_header.sh");

/// Virtiofs shared folders footer (no placeholders)
pub const VIRTIOFS_FOOTER: &str = include_str!("virtiofs_footer.sh");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_not_empty() {
        assert!(!VSOCK_RELAY.is_empty());
        assert!(!AUDIO_WEB.is_empty());
        assert!(!AUDIO_SSH.is_empty());
        assert!(!SSH_XPRA_BASE.is_empty());
        assert!(!SELKIES_HEADER.is_empty());
        assert!(!SELKIES_WRAPPER.is_empty());
        assert!(!SELKIES_SERVICE.is_empty());
        assert!(!VIRTIOFS_HEADER.is_empty());
        assert!(!VIRTIOFS_FOOTER.is_empty());
    }

    #[test]
    fn ssh_xpra_base_has_placeholders() {
        assert!(SSH_XPRA_BASE.contains("{ssh_key}"));
        assert!(SSH_XPRA_BASE.contains("{audio_config}"));
        assert!(SSH_XPRA_BASE.contains("{web_streaming_config}"));
        assert!(SSH_XPRA_BASE.contains("{virtiofs_config}"));
        assert!(SSH_XPRA_BASE.contains("{vsock_config}"));
    }

    #[test]
    fn selkies_templates_have_port_placeholder() {
        assert!(SELKIES_HEADER.contains("{port}"));
        assert!(SELKIES_WRAPPER.contains("{port}"));
        assert!(SELKIES_SERVICE.contains("{port}"));
    }
}
