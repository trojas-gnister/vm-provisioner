use crate::config::AppVMConfig;
use crate::display_bridge::DisplayBridge;
use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// Xpra manager handles desktop integration with Xpra display + SSH PulseAudio forwarding.
pub struct XpraManager {
    config: AppVMConfig,
    host_pulse_socket: String,
    remote_pulse_socket: String,
    vm_ip_cache: RefCell<Option<String>>,
}

impl XpraManager {
    fn ensure_xpra_available() -> Result<(), Box<dyn Error>> {
        match Command::new("xpra").arg("--version").output() {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(
                "xpra binary not found on host. Install it (e.g., `sudo dnf install xpra` or `sudo apt install xpra`)".into(),
            ),
            Err(e) => Err(format!("failed to execute xpra: {}", e).into()),
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
                    return candidate;
                }
            }
        }

        "/run/user/1000/pulse/native".to_string()
    }

    fn get_vm_ip_static(vm_name: &str) -> Result<String, Box<dyn Error>> {
        let output = Command::new("sudo")
            .args(&["virsh", "-c", "qemu:///system", "domifaddr", vm_name])
            .output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("ipv4") {
                    if let Some(ip_part) = line.split_whitespace().nth(3) {
                        if let Some(ip) = ip_part.split('/').next() {
                            return Ok(ip.to_string());
                        }
                    }
                }
            }
        }

        Err("Could not determine VM IP address. VM may not be running.".into())
    }

    fn resolve_vm_ip(&self) -> Result<String, Box<dyn Error>> {
        if let Some(ip) = self.vm_ip_cache.borrow().clone() {
            return Ok(ip);
        }
        let ip = Self::get_vm_ip_static(&self.config.name)?;
        *self.vm_ip_cache.borrow_mut() = Some(ip.clone());
        Ok(ip)
    }

    fn detect_source_ip(destination: &str) -> Option<String> {
        let output = Command::new("ip")
            .args(&["route", "get", destination])
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

    fn is_application_package(&self, package: &str) -> bool {
        let skip_patterns = [
            "xpra",
            "xorg-x11-server-Xvfb",
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
    ) -> Result<(), Box<dyn Error>> {
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

        let exec_line = self.generate_exec_command(&exec_command);
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
            "{}/{}-{}.desktop",
            desktop_dir,
            self.config.name,
            app_name.to_lowercase()
        );

        fs::write(&desktop_file, desktop_content)?;
        Ok(())
    }
}

impl DisplayBridge for XpraManager {
    fn new(config: &AppVMConfig) -> Result<Self, Box<dyn Error>> {
        Self::ensure_xpra_available()?;

        Ok(Self {
            config: config.clone(),
            host_pulse_socket: Self::detect_host_pulse_socket(),
            remote_pulse_socket: "/run/user/1000/pulse/native".to_string(),
            vm_ip_cache: RefCell::new(None),
        })
    }

    fn generate_exec_command(&self, app_command: &str) -> String {
        match self.resolve_vm_ip() {
            Ok(vm_ip) => {
                let ssh_cmd = self.build_ssh_invocation(&vm_ip);
                format!(
                    "xpra start ssh://user@{ip}/ --ssh=\"{ssh}\" --speaker=disabled --microphone=disabled --start-child=\"{cmd}\" --exit-with-children",
                    ip = vm_ip,
                    ssh = ssh_cmd,
                    cmd = app_command
                )
            }
            Err(err) => {
                eprintln!(
                    "⚠️  Unable to resolve VM IP for {}: {}",
                    self.config.name, err
                );
                "echo 'Error: VM IP not found; start the VM before launching apps'".to_string()
            }
        }
    }

    fn launch_app(&self, app_command: &str) -> Result<(), Box<dyn Error>> {
        let exec_command = self.generate_exec_command(app_command);
        println!(
            "🚀 Launching {} in VM {} via Xpra",
            app_command, self.config.name
        );
        println!("   Command: {}", exec_command);

        if exec_command.trim().is_empty() {
            return Err("Empty command".into());
        }

        Command::new("sh").arg("-c").arg(&exec_command).spawn()?;

        println!("✅ Application launched via Xpra");
        Ok(())
    }

    fn generate_desktop_files(&self) -> Result<(), Box<dyn Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);
        fs::create_dir_all(&desktop_dir)?;

        println!("📝 Generating .desktop files in: {}", desktop_dir);

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

        println!("✅ Generated .desktop files for {} applications", count);
        Ok(())
    }

    fn remove_desktop_files(&self) -> Result<(), Box<dyn Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);

        if !Path::new(&desktop_dir).exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&desktop_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with(&format!("{}-", self.config.name)) {
                    fs::remove_file(&path)?;
                    println!("   Removed {}", filename);
                }
            }
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
            "pulseaudio-libs".to_string(),
            "openssh-server".to_string(),
            "git".to_string(),
        ]
    }

    fn kickstart_config(&self, ssh_public_key: &str) -> String {
        format!(
            r#"
# Configure SSH and Xpra for seamless window mode with SSH audio forwarding
echo "=== Configuring SSH, Xpra, and audio forwarding ==="

# SSH key for passwordless authentication
mkdir -p /home/user/.ssh
chmod 700 /home/user/.ssh
cat > /home/user/.ssh/authorized_keys << 'SSH_KEY_EOF'
{0}
SSH_KEY_EOF
chmod 600 /home/user/.ssh/authorized_keys
chown -R user:user /home/user/.ssh

# Configure SSH server for Unix socket forwarding
cat >> /etc/ssh/sshd_config << 'SSHD_CONFIG_EOF'

# Enable Unix socket forwarding for PulseAudio
StreamLocalBindUnlink yes
AllowStreamLocalForwarding yes
SSHD_CONFIG_EOF

# Enable and start SSH server
systemctl enable sshd
systemctl start sshd

# Allow SSH through firewall
firewall-cmd --permanent --add-service=ssh
firewall-cmd --reload

# Disable PipeWire/PulseAudio auto-start (use SSH-forwarded audio instead)
cat > /usr/lib/systemd/user-preset/99-disable-audio.preset << 'AUDIO_PRESET_EOF'
# Disable local audio services - using SSH socket forwarding instead
disable pipewire.service
disable pipewire.socket
disable pipewire-pulse.service
disable pipewire-pulse.socket
disable wireplumber.service
AUDIO_PRESET_EOF

# Set PULSE_SERVER for explicit socket discovery
cat >> /home/user/.bash_profile << 'PULSE_ENV_EOF'

# Use SSH-forwarded PulseAudio socket
export PULSE_SERVER=unix:/run/user/1000/pulse/native
PULSE_ENV_EOF
chown user:user /home/user/.bash_profile

# No auto-login needed - Xpra starts its own X server on demand
systemctl set-default multi-user.target
"#,
            ssh_public_key
        )
    }
}
