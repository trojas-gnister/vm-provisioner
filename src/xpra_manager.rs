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

    /// Get the user's SSH public key, generating one if necessary
    pub fn get_ssh_public_key() -> Result<String, Box<dyn Error>> {
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
        println!("🔑 No SSH key found, generating new ed25519 key...");
        fs::create_dir_all(&ssh_dir)?;

        let key_path = format!("{}/id_ed25519", ssh_dir);
        let status = Command::new("ssh-keygen")
            .args(&["-t", "ed25519", "-f", &key_path, "-N", "", "-q"])
            .status()?;

        if !status.success() {
            return Err("Failed to generate SSH key".into());
        }

        let pub_key_path = format!("{}.pub", key_path);
        Ok(fs::read_to_string(&pub_key_path)?.trim().to_string())
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
                let microphone_flag = if self.config.enable_microphone {
                    "--microphone=yes"
                } else {
                    "--microphone=disabled"
                };
                format!(
                    "xpra start ssh://user@{ip}/ --ssh=\"{ssh}\" --speaker=disabled {mic} --start-child=\"{cmd}\" --exit-with-children",
                    ip = vm_ip,
                    ssh = ssh_cmd,
                    mic = microphone_flag,
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
        // Build virtiofs mount configuration if shared folders are configured
        let virtiofs_config = if !self.config.shared_folders.is_empty() {
            let mut config = String::from(r#"
# ===== Virtiofs Shared Folders Configuration =====
echo "=== Configuring virtiofs shared folders ==="
"#);
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
            config.push_str(r#"
echo "Virtiofs shared folders will be available after reboot."
"#);
            config
        } else {
            String::new()
        };

        // Build HTML5 configuration if enabled
        let html5_config = if let Some(port) = self.config.xpra_html_port {
            // Determine bind address: 0.0.0.0 for LAN access, 127.0.0.1 for localhost only
            let bind_addr = if self.config.xpra_html_lan {
                "0.0.0.0"
            } else {
                "127.0.0.1"
            };
            let access_note = if self.config.xpra_html_lan {
                "echo \"HTML5 web access available at http://<vm-ip>:{port}/ (LAN accessible)\""
            } else {
                "echo \"HTML5 web access available at http://127.0.0.1:{port}/ (localhost only - use SSH tunnel for remote access)\""
            };

            // Build --start-child arguments for flatpak apps (auto-launch in HTML5 mode)
            let start_children: String = self.config.flatpak_packages.iter()
                .map(|pkg| format!(" --start-child=\"flatpak run {}\"", pkg))
                .collect();

            format!(
                r#"
# ===== Xpra HTML5 Web Access Configuration =====
echo "=== Configuring Xpra HTML5 web access on port {port} ==="

# Allow HTML5 port through firewall (use offline-cmd since firewalld isn't running in chroot)
firewall-offline-cmd --add-port={port}/tcp || firewall-cmd --permanent --add-port={port}/tcp || true

# Create systemd service for Xpra HTML5 server
cat > /etc/systemd/system/xpra-html5.service << 'XPRA_SERVICE_EOF'
[Unit]
Description=Xpra HTML5 Web Server
After=network.target

[Service]
Type=simple
User=user
Environment=DISPLAY=:100
Environment=PULSE_SERVER=unix:/run/user/1000/pulse/native
# Start Xpra server with HTML5 enabled
# --start-new-commands=yes allows launching additional apps into this session later
# Apps can be launched via: xpra control :100 start "command"
ExecStart=/usr/bin/xpra start :100 --bind-tcp={bind_addr}:{port} --html=on --daemon=no --start-new-commands=yes --exit-with-children=no --speaker=yes --pulseaudio=yes{start_children}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
XPRA_SERVICE_EOF

# Enable the HTML5 service to start on boot
systemctl daemon-reload
systemctl enable xpra-html5.service

# Create helper script to launch apps into the HTML5 session
cat > /usr/local/bin/xpra-launch << 'LAUNCH_SCRIPT_EOF'
#!/bin/bash
# Launch an application into the Xpra HTML5 session
# Usage: xpra-launch "command"
if [ -z "$1" ]; then
    echo "Usage: xpra-launch <command>"
    exit 1
fi
DISPLAY=:100 xpra control :100 start "$@"
LAUNCH_SCRIPT_EOF
chmod +x /usr/local/bin/xpra-launch

{access_note}
echo "Xpra client can attach via: xpra attach tcp://<vm-ip>:{port}/"
"#,
                port = port,
                bind_addr = bind_addr,
                access_note = access_note,
                start_children = start_children
            )
        } else {
            String::new()
        };

        // Audio configuration: disable local audio for native xpra (SSH forwarding), keep enabled for HTML5
        let audio_config = if self.config.xpra_html_port.is_some() {
            // HTML5 mode: keep PulseAudio enabled so xpra can capture audio locally
            r#"# HTML5 mode: Keep PulseAudio/PipeWire enabled for local audio capture
echo "=== Configuring audio for HTML5 mode (local PulseAudio) ==="
# PulseAudio services remain enabled for xpra to capture audio
# The xpra server will encode and stream audio to the browser"#.to_string()
        } else {
            // Native xpra (SSH): disable local audio, use SSH-forwarded socket
            r#"# Disable PipeWire/PulseAudio auto-start (use SSH-forwarded audio instead)
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
chown user:user /home/user/.bash_profile"#.to_string()
        };

        format!(
            r#"
# Configure SSH and Xpra for seamless window mode with SSH audio forwarding
echo "=== Configuring SSH, Xpra, and audio forwarding ==="

# Install xpra from updates repo (not available during kickstart %packages phase)
echo "Installing xpra from updates repository..."
dnf install -y xpra xorg-x11-server-Xvfb git tar || echo "Warning: xpra installation failed"

# Install xpra-html5 web client (required for browser access)
if [ ! -d /usr/share/xpra/www ]; then
    echo "Installing xpra-html5 web client..."
    mkdir -p /usr/share/xpra/www
    cd /tmp && git clone --depth 1 https://github.com/Xpra-org/xpra-html5.git
    cp -r /tmp/xpra-html5/html5/* /usr/share/xpra/www/
    rm -rf /tmp/xpra-html5
fi

# SSH key for passwordless authentication
mkdir -p /home/user/.ssh
chmod 700 /home/user/.ssh
cat > /home/user/.ssh/authorized_keys << 'SSH_KEY_EOF'
{ssh_key}
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

{audio_config}

# No auto-login needed - Xpra starts its own X server on demand
systemctl set-default multi-user.target
{html5_config}
{virtiofs_config}
"#,
            ssh_key = ssh_public_key,
            audio_config = audio_config,
            html5_config = html5_config,
            virtiofs_config = virtiofs_config
        )
    }
}
