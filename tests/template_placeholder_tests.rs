//! Template placeholder validation tests
//!
//! These tests verify that all `{placeholder}` patterns in templates
//! are properly substituted during rendering with format!(). This catches
//! bugs where a placeholder is misspelled or format argument is missing.

use regex::Regex;

/// Find all `{placeholder}` patterns in content (format!() style)
/// Excludes escaped braces `{{` and `}}`
fn find_format_placeholders(content: &str) -> Vec<String> {
    // Simple pattern: match {word} where word is lowercase letters and underscores
    let re = Regex::new(r"\{([a-z_]+)\}").unwrap();
    re.captures_iter(content)
        .filter_map(|c| {
            let m = c.get(0)?;
            let start = m.start();
            let end = m.end();
            // Check it's not escaped (not preceded by { or followed by })
            let is_escaped = (start > 0 && content.as_bytes().get(start - 1) == Some(&b'{'))
                || content.as_bytes().get(end) == Some(&b'}');
            if is_escaped {
                None
            } else {
                Some(m.as_str().to_string())
            }
        })
        .collect()
}

/// Verify that a template has the expected placeholders before substitution
fn verify_template_has_placeholders(template: &str, expected: &[&str]) {
    let found = find_format_placeholders(template);
    for placeholder in expected {
        assert!(
            found.contains(&placeholder.to_string()),
            "Template missing expected placeholder: {}. Found: {:?}",
            placeholder,
            found
        );
    }
}

mod ssh_xpra_base {
    use super::*;
    use vm_provisioner::templates;

    #[test]
    fn has_required_placeholders() {
        let template = templates::SSH_XPRA_BASE;
        verify_template_has_placeholders(
            template,
            &[
                "{ssh_key}",
                "{audio_config}",
                "{web_streaming_config}",
                "{virtiofs_config}",
                "{vsock_config}",
            ],
        );
    }

    #[test]
    fn substitution_removes_all_placeholders() {
        let template = templates::SSH_XPRA_BASE;

        // Test that .replace() chain removes all placeholders
        let rendered = template
            .replace("{ssh_key}", "ssh-ed25519 AAAAC3... test@test")
            .replace("{audio_config}", "# Audio config here")
            .replace("{web_streaming_config}", "# Web streaming config")
            .replace("{virtiofs_config}", "# Virtiofs config")
            .replace("{vsock_config}", "# Vsock config");

        let remaining = find_format_placeholders(&rendered);
        assert!(
            remaining.is_empty(),
            "Found unsubstituted placeholders after rendering: {:?}",
            remaining
        );
    }
}

mod selkies_templates {
    use super::*;
    use vm_provisioner::templates;

    #[test]
    fn selkies_header_has_placeholders() {
        let template = templates::SELKIES_HEADER;
        verify_template_has_placeholders(
            template,
            &["{port}", "{systemd_services}", "{systemd_enable_commands}"],
        );
    }

    #[test]
    fn selkies_wrapper_has_port_placeholder() {
        let template = templates::SELKIES_WRAPPER;
        verify_template_has_placeholders(template, &["{port}"]);
    }

    #[test]
    fn selkies_service_has_placeholders() {
        let template = templates::SELKIES_SERVICE;
        verify_template_has_placeholders(template, &["{port}", "{password}", "{menu_items}"]);
    }

    #[test]
    fn selkies_header_substitution_complete() {
        let template = templates::SELKIES_HEADER;
        let rendered = template
            .replace("{port}", "8080")
            .replace("{systemd_services}", "# services")
            .replace("{systemd_enable_commands}", "# enable commands");

        let remaining = find_format_placeholders(&rendered);
        assert!(
            remaining.is_empty(),
            "Selkies header has unsubstituted placeholders: {:?}",
            remaining
        );
    }
}

mod audio_templates {
    use super::find_format_placeholders;
    use vm_provisioner::templates;

    #[test]
    fn audio_ssh_has_no_placeholders() {
        // AUDIO_SSH should be a complete template with no placeholders
        let remaining = find_format_placeholders(templates::AUDIO_SSH);
        assert!(
            remaining.is_empty(),
            "AUDIO_SSH should have no placeholders, found: {:?}",
            remaining
        );
    }

    #[test]
    fn audio_web_has_no_placeholders() {
        // AUDIO_WEB should be a complete template with no placeholders
        let remaining = find_format_placeholders(templates::AUDIO_WEB);
        assert!(
            remaining.is_empty(),
            "AUDIO_WEB should have no placeholders, found: {:?}",
            remaining
        );
    }
}

mod vsock_template {
    use super::find_format_placeholders;
    use vm_provisioner::templates;

    #[test]
    fn vsock_relay_has_no_placeholders() {
        // VSOCK_RELAY should be a complete template with no placeholders
        let remaining = find_format_placeholders(templates::VSOCK_RELAY);
        assert!(
            remaining.is_empty(),
            "VSOCK_RELAY should have no placeholders, found: {:?}",
            remaining
        );
    }
}

mod virtiofs_templates {
    use super::find_format_placeholders;
    use vm_provisioner::templates;

    #[test]
    fn virtiofs_header_has_no_placeholders() {
        let remaining = find_format_placeholders(templates::VIRTIOFS_HEADER);
        assert!(
            remaining.is_empty(),
            "VIRTIOFS_HEADER should have no placeholders, found: {:?}",
            remaining
        );
    }

    #[test]
    fn virtiofs_footer_has_no_placeholders() {
        let remaining = find_format_placeholders(templates::VIRTIOFS_FOOTER);
        assert!(
            remaining.is_empty(),
            "VIRTIOFS_FOOTER should have no placeholders, found: {:?}",
            remaining
        );
    }
}
