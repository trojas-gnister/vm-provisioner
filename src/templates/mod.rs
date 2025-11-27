//! Kickstart configuration templates for VM provisioning.
//!
//! This module provides shell script templates used during VM installation
//! via kickstart. Templates use `{{PLACEHOLDER}}` syntax for variable substitution.

/// Vsock relay configuration for network-disabled VMs
pub const VSOCK_RELAY: &str = include_str!("vsock_relay.sh");

/// Audio configuration for web streaming mode (PipeWire/PulseAudio enabled)
pub const AUDIO_WEB: &str = include_str!("audio_web.sh");

/// Audio configuration for SSH/Xpra mode (one-way tunnel for playback)
pub const AUDIO_SSH: &str = include_str!("audio_ssh.sh");

/// Base SSH and Xpra configuration template
/// Placeholders: {{SSH_KEY}}, {{AUDIO_CONFIG}}, {{WEB_STREAMING_CONFIG}}, {{VIRTIOFS_CONFIG}}, {{VSOCK_CONFIG}}
pub const SSH_XPRA_BASE: &str = include_str!("ssh_xpra_base.sh");

/// Selkies-GStreamer header configuration
/// Placeholders: {{PORT}}, {{SYSTEMD_SERVICES}}, {{SYSTEMD_ENABLE_COMMANDS}}
pub const SELKIES_HEADER: &str = include_str!("selkies_header.sh");

/// Selkies wrapper script template
/// Placeholders: {{PORT}}
pub const SELKIES_WRAPPER: &str = include_str!("selkies_wrapper.sh");

/// Selkies systemd service and Openbox configuration
/// Placeholders: {{PORT}}, {{PASSWORD}}, {{MENU_ITEMS}}
pub const SELKIES_SERVICE: &str = include_str!("selkies_service.sh");

/// Virtiofs shared folders header
pub const VIRTIOFS_HEADER: &str = include_str!("virtiofs_header.sh");

/// Virtiofs shared folders footer
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
        assert!(SSH_XPRA_BASE.contains("{{SSH_KEY}}"));
        assert!(SSH_XPRA_BASE.contains("{{AUDIO_CONFIG}}"));
        assert!(SSH_XPRA_BASE.contains("{{WEB_STREAMING_CONFIG}}"));
        assert!(SSH_XPRA_BASE.contains("{{VIRTIOFS_CONFIG}}"));
        assert!(SSH_XPRA_BASE.contains("{{VSOCK_CONFIG}}"));
    }

    #[test]
    fn selkies_templates_have_port_placeholder() {
        assert!(SELKIES_HEADER.contains("{{PORT}}"));
        assert!(SELKIES_WRAPPER.contains("{{PORT}}"));
        assert!(SELKIES_SERVICE.contains("{{PORT}}"));
    }
}
