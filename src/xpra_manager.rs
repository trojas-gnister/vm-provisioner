//! Xpra display manager implementation
//!
//! This module provides the `XpraManager` struct which implements the `DisplayBridge`
//! trait for Xpra-based display forwarding with SSH audio tunneling.

use crate::config::{AppVMConfig, NetworkMode};
use crate::display_bridge::DisplayBridge;
use crate::error::{DisplayError, Result};
use crate::templates;
use log::{debug, info, warn};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Transport type for connecting to VMs
#[derive(Debug, Clone)]
enum TransportType {
    /// SSH over TCP/IP (standard networking)
    Ssh { ip: String },
    /// SSH over vsock (for network-disabled VMs)
    Vsock { cid: u32 },
}

/// Xpra manager handles desktop integration with Xpra display + SSH PulseAudio forwarding.
pub struct XpraManager {
    config: AppVMConfig,
    host_pulse_socket: String,
    remote_pulse_socket: String,
    vm_ip_cache: OnceLock<String>,
}

impl XpraManager {
    fn ensure_xpra_available() -> Result<()> {
        match Command::new("xpra").arg("--version").output() {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(DisplayError::XpraNotFound.into())
            }
            Err(e) => Err(DisplayError::XpraExecution(e.to_string()).into()),
        }
    }

    fn detect_host_pulse_socket() -> String {
        if let Ok(socket) = std::env::var("XPRA_PULSE_SOCKET") {
            if !socket.is_empty() {
                return socket;
            }
        }

        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let candidate = format!("{}/pulse/native", runtime_dir);
            if Path::new(&candidate).exists() {
                return candidate;
            }
        }

        if let Ok(output) = Command::new("id").arg("-u").output() {
            if output.status.success() {
                if let Ok(uid) = String::from_utf8(output.stdout) {
                    let uid = uid.trim();
                    let candidate = format!("/run/user/{}/pulse/native", uid);
                    if Path::new(&candidate).exists() {
                        return candidate;
                    }
                    // Return candidate even if it doesn't exist - caller handles missing socket
                    return candidate;
                }
            }
        }

        "/run/user/1000/pulse/native".to_string()
    }

    /// Get the user's SSH public key, generating one if necessary
    pub fn get_ssh_public_key() -> Result<String> {
        let home = std::env::var("HOME")?;
        let ssh_dir = format!("{}/.ssh", home);

        // Check for existing keys in priority order
        let key_types = ["id_ed25519", "id_rsa", "id_ecdsa"];
        for key_type in &key_types {
            let pub_key_path = format!("{}/{}.pub", ssh_dir, key_type);
            if Path::new(&pub_key_path).exists() {
                return Ok(fs::read_to_string(&pub_key_path)?.trim().to_string());
            }
        }

        // No key exists, generate a new ed25519 key
        info!("No SSH key found, generating new ed25519 key...");
        fs::create_dir_all(&ssh_dir)?;

        let key_path = format!("{}/id_ed25519", ssh_dir);
        let status = Command::new("ssh-keygen")
            .args(["-t", "ed25519", "-f", &key_path, "-N", "", "-q"])
            .status()?;

        if !status.success() {
            return Err(DisplayError::ConnectionFailed("Failed to generate SSH key".to_string()).into());
        }

        let pub_key_path = format!("{}.pub", key_path);
        Ok(fs::read_to_string(&pub_key_path)?.trim().to_string())
    }

    fn get_vm_ip_static(vm_name: &str) -> Result<String> {
        let output = Command::new("sudo")
            .args(["virsh", "-c", "qemu:///system", "domifaddr", vm_name])
            .output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("ipv4") {
                    if let Some(ip_part) = line.split_whitespace().nth(3) {
                        if let Some(ip) = ip_part.split('/').next() {
                            // Validate IP address format before returning
                            if Self::validate_ip_address(ip) {
                                return Ok(ip.to_string());
                            } else {
                                warn!("Invalid IP address format from virsh: {}", ip);
                            }
                        }
                    }
                }
            }
        }

        Err(DisplayError::ConnectionFailed(
            "Could not determine VM IP address. VM may not be running.".to_string(),
        )
        .into())
    }

    /// Validate an IP address string
    fn validate_ip_address(ip: &str) -> bool {
        ip.parse::<std::net::Ipv4Addr>().is_ok()
    }

    fn resolve_vm_ip(&self) -> Result<String> {
        if let Some(ip) = self.vm_ip_cache.get() {
            return Ok(ip.clone());
        }
        let ip = Self::get_vm_ip_static(&self.config.name)?;
        let _ = self.vm_ip_cache.set(ip.clone());
        Ok(ip)
    }

    /// Determine transport type based on network configuration
    fn resolve_transport(&self) -> Result<TransportType> {
        match self.config.network_mode {
            NetworkMode::None => {
                let cid = self
                    .config
                    .vsock_cid
                    .ok_or(DisplayError::VsockNotConfigured)?;
                Ok(TransportType::Vsock { cid })
            }
            _ => {
                let ip = self.resolve_vm_ip()?;
                Ok(TransportType::Ssh { ip })
            }
        }
    }

    fn detect_source_ip(destination: &str) -> Option<String> {
        let output = Command::new("ip")
            .args(["route", "get", destination])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut tokens = stdout.split_whitespace();
        while let Some(token) = tokens.next() {
            if token == "src" {
                return tokens.next().map(|s| s.to_string());
            }
        }
        None
    }

    fn build_ssh_invocation(&self, vm_ip: &str) -> String {
        let mut ssh_cmd = format!(
            "ssh -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o TCPKeepAlive=yes -o StrictHostKeyChecking=no -R {remote}:{host}",
            remote = self.remote_pulse_socket,
            host = self.host_pulse_socket
        );

        if let Some(src_ip) = Self::detect_source_ip(vm_ip) {
            ssh_cmd.push_str(&format!(" -b {}", src_ip));
        }

        ssh_cmd
    }

    /// Get path to the vsock SSH wrapper script
    fn get_vsock_ssh_wrapper_path(&self) -> Result<String> {
        let home = std::env::var("HOME")?;
        Ok(format!(
            "{}/.config/vm-provisioner/{}-vsock-ssh",
            home, self.config.name
        ))
    }

    /// Create an SSH wrapper script for vsock transport
    /// This avoids complex quoting issues with xpra's --ssh option
    fn create_vsock_ssh_wrapper(&self, cid: u32) -> Result<String> {
        let wrapper_path = self.get_vsock_ssh_wrapper_path()?;
        let script = format!(
            r#"#!/bin/bash
# Auto-generated vsock SSH wrapper for {} (CID: {})
# Note: UserKnownHostsFile=/dev/null bypasses host key checking entirely
# This is safe for vsock since the CID provides trust (only the specific VM can respond)
exec ssh -o ExitOnForwardFailure=yes \
    -o ServerAliveInterval=15 \
    -o ServerAliveCountMax=4 \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    -o "ProxyCommand=socat - VSOCK-CONNECT:{}:22" \
    -R {}:{} \
    "$@"
"#,
            self.config.name,
            cid,
            cid,
            self.remote_pulse_socket,
            self.host_pulse_socket
        );

        std::fs::write(&wrapper_path, &script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&wrapper_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&wrapper_path, perms)?;
        }

        Ok(wrapper_path)
    }

    /// Build SSH command for vsock transport (network-disabled VMs)
    fn build_ssh_invocation_vsock(&self, cid: u32) -> Result<String> {
        // Create a wrapper script to avoid complex quoting issues with xpra
        let wrapper_path = self.create_vsock_ssh_wrapper(cid)?;
        Ok(wrapper_path)
    }

    fn is_application_package(&self, package: &str) -> bool {
        let skip_patterns = [
            "xpra",
            "xorg-x11-server-Xvfb",
            "openbox",
            "xterm",
            "pulseaudio-libs",
            "git",
            "openssh-server",
            "-devel",
            "-libs",
            "lib",
        ];
        !skip_patterns.iter().any(|p| package.contains(p))
    }

    fn guess_categories(&self, package: &str) -> &'static str {
        let lower = package.to_lowercase();
        if lower.contains("firefox")
            || lower.contains("chrome")
            || lower.contains("browser")
            || lower.contains("librewolf")
        {
            "Network;WebBrowser;"
        } else if lower.contains("code") || lower.contains("editor") || lower.contains("ide") {
            "Development;IDE;"
        } else if lower.contains("gimp") || lower.contains("inkscape") {
            "Graphics;"
        } else if lower.contains("vlc") || lower.contains("media") {
            "AudioVideo;Player;"
        } else if lower.contains("slack") || lower.contains("discord") || lower.contains("telegram")
        {
            "Network;InstantMessaging;"
        } else {
            "Utility;"
        }
    }

    fn create_desktop_file(
        &self,
        package: &str,
        desktop_dir: &str,
        is_flatpak: bool,
    ) -> Result<()> {
        let app_name = if is_flatpak {
            package.split('.').last().unwrap_or(package)
        } else {
            package
        };

        let exec_command = if is_flatpak {
            format!("flatpak run {}", package)
        } else {
            package.to_string()
        };

        let xpra_command = self.generate_exec_command(&exec_command);
        let vm_ip = self.resolve_vm_ip().unwrap_or_else(|_| "UNKNOWN".to_string());

        // Create a wrapper script instead of embedding complex command in .desktop file
        let wrapper_script_path = format!(
            "{}/vm-provisioner-{}-{}-launch.sh",
            desktop_dir,
            self.config.name,
            app_name.to_lowercase()
        );
        let wrapper_content = format!(
            "#!/bin/bash\n# Auto-generated launch script for {} in {}\nssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null user@{} 'pkill -f \"xpra seamless\" 2>/dev/null || true'\n{}",
            app_name, self.config.name, vm_ip, xpra_command
        );
        fs::write(&wrapper_script_path, wrapper_content)?;

        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&wrapper_script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&wrapper_script_path, perms)?;
        }

        let exec_line = wrapper_script_path.clone();
        let icon = app_name.to_lowercase();
        let categories = self.guess_categories(package);

        let desktop_content = format!(
            r#"[Desktop Entry]
Version=1.0
Type=Application
Name={app_name} (VM: {vm_name})
Comment=Run in isolated VM via Xpra
Icon={icon}
Exec={exec_line}
Terminal=false
Categories={categories}
StartupWMClass={app_name}
Keywords=vm;isolated;sandbox;{package};
"#,
            app_name = app_name,
            vm_name = self.config.name,
            icon = icon,
            exec_line = exec_line,
            categories = categories,
            package = package,
        );

        let desktop_file = format!(
            "{}/vm-provisioner-{}-{}.desktop",
            desktop_dir,
            self.config.name,
            app_name.to_lowercase()
        );

        fs::write(&desktop_file, desktop_content)?;
        Ok(())
    }
}

impl DisplayBridge for XpraManager {
    fn new(config: &AppVMConfig) -> Result<Self> {
        Self::ensure_xpra_available()?;

        Ok(Self {
            config: config.clone(),
            host_pulse_socket: Self::detect_host_pulse_socket(),
            remote_pulse_socket: "/run/user/1000/pulse/native".to_string(),
            vm_ip_cache: OnceLock::new(),
        })
    }

    fn generate_exec_command(&self, app_command: &str) -> String {
        match self.resolve_transport() {
            Ok(TransportType::Vsock { cid }) => {
                // Network-disabled VM: use vsock transport with wrapper script
                match self.build_ssh_invocation_vsock(cid) {
                    Ok(ssh_wrapper) => {
                        format!(
                            "env GDK_BACKEND=x11 xpra start ssh://user@localhost/ --ssh=\"{ssh}\" --speaker=disabled --microphone=disabled --min-quality=80 --modal-windows=yes --start-child=\"{cmd}\" --exit-with-children",
                            ssh = ssh_wrapper,
                            cmd = app_command
                        )
                    }
                    Err(e) => {
                        warn!("Failed to create vsock SSH wrapper: {}", e);
                        "echo 'Error: Could not create vsock SSH wrapper'".to_string()
                    }
                }
            }
            Ok(TransportType::Ssh { ip }) => {
                // Standard networked VM: use SSH over TCP/IP
                let ssh_cmd = self.build_ssh_invocation(&ip);
                format!(
                    "env GDK_BACKEND=x11 xpra start ssh://user@{ip}/ --ssh=\"{ssh}\" --speaker=disabled --microphone=disabled --min-quality=80 --modal-windows=yes --start-child=\"{cmd}\" --exit-with-children",
                    ip = ip,
                    ssh = ssh_cmd,
                    cmd = app_command
                )
            }
            Err(err) => {
                warn!(
                    "Unable to resolve transport for {}: {}",
                    self.config.name, err
                );
                "echo 'Error: Cannot connect to VM; check network mode and start the VM'".to_string()
            }
        }
    }

    fn launch_app(&self, app_command: &str) -> Result<()> {
        let exec_command = self.generate_exec_command(app_command);
        info!("Launching {} in VM {} via Xpra", app_command, self.config.name);

        // Kill any stale xpra sessions before launching
        debug!("Cleaning up stale xpra sessions...");
        let cleanup_cmd = match self.resolve_transport() {
            Ok(TransportType::Vsock { cid }) => {
                // Vsock: use socat proxy for SSH
                format!(
                    "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ProxyCommand='socat - VSOCK-CONNECT:{}:22' user@localhost 'pkill -f \\\"xpra seamless\\\" 2>/dev/null || true'",
                    cid
                )
            }
            Ok(TransportType::Ssh { ip }) => {
                // Standard SSH cleanup
                format!(
                    "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null user@{} 'pkill -f \\\"xpra seamless\\\" 2>/dev/null || true'",
                    ip
                )
            }
            Err(_) => String::new(), // Skip cleanup if transport resolution fails
        };
        if !cleanup_cmd.is_empty() {
            let _ = Command::new("bash").arg("-c").arg(&cleanup_cmd).output();
        }

        debug!("Command: {}", exec_command);

        if exec_command.trim().is_empty() {
            return Err(DisplayError::LaunchFailed("Empty command".to_string()).into());
        }

        // Ensure DISPLAY is set for Xpra to work on Wayland compositors (Sway, etc.)
        // The sudo call for virsh can strip environment variables, so we need to
        // explicitly pass DISPLAY to the spawned process
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| {
            // Try to detect XWayland display from running process
            if let Ok(output) = Command::new("pgrep").args(["-a", "Xwayland"]).output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().next() {
                    // Extract :N from "Xwayland :0 -rootless..."
                    if let Some(disp) = line.split_whitespace().find(|s| s.starts_with(':')) {
                        debug!("Detected XWayland display: {}", disp);
                        return disp.to_string();
                    }
                }
            }
            ":0".to_string()
        });

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&exec_command);
        cmd.env("DISPLAY", &display);

        // Pass WAYLAND_DISPLAY if available (for native Wayland fallback)
        if let Ok(wd) = std::env::var("WAYLAND_DISPLAY") {
            cmd.env("WAYLAND_DISPLAY", &wd);
        }

        // Pass XDG_RUNTIME_DIR for Wayland socket access
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            cmd.env("XDG_RUNTIME_DIR", &xdg_runtime);
        }

        debug!("Launching xpra with DISPLAY={}", display);
        cmd.spawn()?;

        info!("Application launched via Xpra");
        Ok(())
    }

    fn generate_desktop_files(&self) -> Result<()> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications", home);
        fs::create_dir_all(&desktop_dir)?;

        info!("Generating .desktop files in: {}", desktop_dir);

        let mut count = 0;
        for package in &self.config.system_packages {
            if self.is_application_package(package) {
                self.create_desktop_file(package, &desktop_dir, false)?;
                count += 1;
            }
        }

        for package in &self.config.flatpak_packages {
            self.create_desktop_file(package, &desktop_dir, true)?;
            count += 1;
        }

        // Update desktop database for app launchers to pick up new entries
        let _ = Command::new("update-desktop-database")
            .arg(&desktop_dir)
            .output();

        info!("Generated .desktop files for {} applications", count);
        Ok(())
    }

    fn remove_desktop_files(&self) -> Result<()> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications", home);

        if !Path::new(&desktop_dir).exists() {
            return Ok(());
        }

        let mut removed = false;
        let prefix = format!("vm-provisioner-{}-", self.config.name);
        for entry in fs::read_dir(&desktop_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // Remove both .desktop files and launch scripts
                if filename.starts_with(&prefix)
                    && (filename.ends_with(".desktop") || filename.ends_with("-launch.sh"))
                {
                    fs::remove_file(&path)?;
                    debug!("Removed {}", filename);
                    removed = true;
                }
            }
        }

        // Update desktop database after removing entries
        if removed {
            let _ = Command::new("update-desktop-database")
                .arg(&desktop_dir)
                .output();
        }

        Ok(())
    }

    fn list_applications(&self) -> Vec<String> {
        let mut apps = Vec::new();

        for package in &self.config.system_packages {
            if self.is_application_package(package) {
                apps.push(package.clone());
            }
        }

        for package in &self.config.flatpak_packages {
            apps.push(format!("flatpak run {}", package));
        }

        apps
    }

    fn guest_packages(&self) -> Vec<String> {
        vec![
            "xpra".to_string(),
            "xorg-x11-server-Xvfb".to_string(),
            "openbox".to_string(),
            "xterm".to_string(),
            "pulseaudio-libs".to_string(),
            "openssh-server".to_string(),
            "git".to_string(),
        ]
    }

    fn kickstart_config(&self, ssh_public_key: &str) -> String {
        // Build vsock relay configuration for network-disabled VMs
        let vsock_config = if self.config.enable_vsock {
            templates::VSOCK_RELAY.to_string()
        } else {
            String::new()
        };

        // Build virtiofs mount configuration if shared folders are configured
        let virtiofs_config = if !self.config.shared_folders.is_empty() {
            let mut config = templates::VIRTIOFS_HEADER.to_string();
            for folder in &self.config.shared_folders {
                let mount_options = if folder.readonly {
                    "defaults,nofail,ro"
                } else {
                    "defaults,nofail"
                };
                config.push_str(&format!(
                    r#"
# Create mount point for shared folder: {host_path} -> {guest_path}
mkdir -p {guest_path}
chown user:user {guest_path}

# Add fstab entry for virtiofs mount
echo "{tag}  {guest_path}  virtiofs  {options}  0  0" >> /etc/fstab
echo "   Configured: {tag} -> {guest_path}"
"#,
                    host_path = folder.host_path,
                    guest_path = folder.guest_path,
                    tag = folder.tag,
                    options = mount_options
                ));
            }
            config.push_str(templates::VIRTIOFS_FOOTER);
            config
        } else {
            String::new()
        };

        // Build Selkies-GStreamer web streaming configuration if enabled
        let web_streaming_config = if let Some(port) = self.config.web_port {
            // Build systemd user services for each flatpak app
            let systemd_services: String = self.config.flatpak_packages.iter()
                .map(|pkg| {
                    let app_name = pkg.split('.').last().unwrap_or(pkg).replace("-", "_");
                    format!(
                        r#"cat > /home/user/.config/systemd/user/{app_name}.service << 'SERVICE_EOF'
[Unit]
Description={pkg} Auto-Restart
After=default.target

[Service]
Type=simple
Environment=DISPLAY=:100
ExecStart=/usr/bin/flatpak run {pkg}
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
SERVICE_EOF
"#,
                        app_name = app_name,
                        pkg = pkg
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            // Build commands to enable and start services
            let systemd_enable_commands: String = self.config.flatpak_packages.iter()
                .map(|pkg| {
                    let app_name = pkg.split('.').last().unwrap_or(pkg).replace("-", "_");
                    format!("runuser -u user -- systemctl --user daemon-reload\nrunuser -u user -- systemctl --user enable {}.service\nrunuser -u user -- systemctl --user start {}.service", app_name, app_name)
                })
                .collect::<Vec<_>>()
                .join("\n");

            // Build Openbox menu items for each flatpak app
            let menu_items: String = self.config.flatpak_packages.iter()
                .map(|pkg| {
                    let app_name = pkg.split('.').last().unwrap_or(pkg);
                    format!(
                        r#"    <item label="{}">
      <action name="Execute">
        <execute>sh -c 'DISPLAY=:100 nohup flatpak run {} &amp;>/dev/null &amp;'</execute>
      </action>
    </item>"#,
                        app_name, pkg
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            // Build selkies config from templates
            let port_str = port.to_string();
            let header = templates::SELKIES_HEADER
                .replace("{{PORT}}", &port_str)
                .replace("{{SYSTEMD_SERVICES}}", &systemd_services)
                .replace("{{SYSTEMD_ENABLE_COMMANDS}}", &systemd_enable_commands);

            let wrapper = format!(
                "\n# Create wrapper script that starts Xvfb and Selkies together\ncat > /home/user/selkies-wrapper.sh << 'WRAPPER_EOF'\n{}\nWRAPPER_EOF\nchmod +x /home/user/selkies-wrapper.sh\nchown user:user /home/user/selkies-wrapper.sh\n",
                templates::SELKIES_WRAPPER.replace("{{PORT}}", &port_str)
            );

            let service = templates::SELKIES_SERVICE
                .replace("{{PORT}}", &port_str)
                .replace("{{PASSWORD}}", &self.config.user_password)
                .replace("{{MENU_ITEMS}}", &menu_items);

            format!("{}{}{}", header, wrapper, service)
        } else {
            String::new()
        };

        // Audio configuration: disable local audio for native xpra (SSH forwarding), keep enabled for web streaming
        let audio_config = if self.config.web_port.is_some() {
            // Web streaming mode: keep PulseAudio enabled so Selkies can capture audio locally
            templates::AUDIO_WEB.to_string()
        } else {
            // Native xpra (SSH): configure one-way PulseAudio tunnel for playback only
            templates::AUDIO_SSH.to_string()
        };

        templates::SSH_XPRA_BASE
            .replace("{{SSH_KEY}}", ssh_public_key)
            .replace("{{AUDIO_CONFIG}}", &audio_config)
            .replace("{{WEB_STREAMING_CONFIG}}", &web_streaming_config)
            .replace("{{VIRTIOFS_CONFIG}}", &virtiofs_config)
            .replace("{{VSOCK_CONFIG}}", &vsock_config)
    }
}
