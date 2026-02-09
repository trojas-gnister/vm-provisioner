//! Fedora-to-NixOS package name mapping
//!
//! Maps common Fedora/RPM package names to their nixpkgs equivalents.

/// Map a Fedora package name to its nixpkgs equivalent.
///
/// Returns `None` for packages that are handled via NixOS service options
/// (e.g. `qemu-guest-agent` → `services.qemuGuest.enable`, `pipewire` →
/// `services.pipewire.enable`) and should not appear in
/// `environment.systemPackages`.
///
/// Unknown packages are passed through as-is since nixpkgs uses similar
/// names for most things.
pub fn map_package(fedora_name: &str) -> Option<&str> {
    match fedora_name {
        // Handled by NixOS service options, not packages
        "qemu-guest-agent" => None,
        "pipewire" | "wireplumber" | "pipewire-pulse" => None,

        // Direct name mappings
        "openssh-server" => Some("openssh"),
        "git" => Some("git"),
        "openbox" => Some("openbox"),
        "xterm" => Some("xterm"),
        "firefox" => Some("firefox"),
        "mesa-vulkan-drivers" => Some("mesa"),
        "vulkan-loader" => Some("vulkan-loader"),

        // GStreamer mappings
        "gstreamer1" => Some("gst_all_1.gstreamer"),
        "gstreamer1-plugins-base" => Some("gst_all_1.gst-plugins-base"),
        "gstreamer1-plugins-good" => Some("gst_all_1.gst-plugins-good"),
        "gstreamer1-plugins-bad-free" => Some("gst_all_1.gst-plugins-bad"),
        "gstreamer1-plugins-ugly-free" => Some("gst_all_1.gst-plugins-ugly"),

        // Pass through as-is
        other => Some(other),
    }
}
