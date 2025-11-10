use crate::config::AppVMConfig;
use crate::display_bridge::DisplayBridge;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct X2GoManager {
    config: AppVMConfig,
}

impl X2GoManager {
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

    fn get_ssh_key_path() -> Result<PathBuf, Box<dyn Error>> {
        let home_dir = std::env::var("HOME")?;
        let ssh_dir = Path::new(&home_dir).join(".ssh");
        let key_files = ["id_ed25519", "id_rsa", "id_ecdsa"];
        for key_file in &key_files {
            let key_path = ssh_dir.join(key_file);
            if key_path.exists() {
                return Ok(key_path);
            }
        }
        Err("No suitable SSH private key found in ~/.ssh/".into())
    }

    fn is_application_package(&self, package: &str) -> bool {
        let skip_patterns = [
            "xorg-x11-server-Xorg", "xorg-x11-xinit", "i3", "i3status", "dmenu", "rofi",
            "x2goserver", "x2goserver-xsession", "pulseaudio", "pulseaudio-utils", "xclip",
            "git", "openssh-server", "-devel", "-libs", "lib",
        ];
        !skip_patterns.iter().any(|p| package.contains(p))
    }

    fn guess_categories(&self, package: &str) -> &'static str {
        let lower = package.to_lowercase();
        if lower.contains("firefox") || lower.contains("chrome") || lower.contains("browser") {
            "Network;WebBrowser;"
        } else if lower.contains("code") || lower.contains("editor") || lower.contains("ide") {
            "Development;IDE;"
        } else if lower.contains("gimp") || lower.contains("inkscape") {
            "Graphics;"
        } else if lower.contains("vlc") || lower.contains("media") {
            "AudioVideo;Player;"
        } else if lower.contains("slack") || lower.contains("discord") || lower.contains("telegram") {
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
Comment=Run in isolated VM via X2Go
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

    fn ensure_x2goclient_available() -> Result<(), Box<dyn Error>> {
        match Command::new("x2goclient").arg("--version").output() {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(
                    "x2goclient binary not found on host.\n\n\
                     Install it first:\n\
                     - Fedora/RHEL: sudo dnf install x2goclient\n\
                     - Debian/Ubuntu: sudo apt install x2goclient\n\
                     - Arch Linux: sudo pacman -S x2goclient\n\n\
                     Or use Waypipe instead (default protocol)."
                        .into(),
                )
            }
            Err(e) => Err(format!("Failed to execute x2goclient: {}", e).into()),
        }
    }

    pub fn remove_desktop_files(&self) -> Result<(), Box<dyn Error>> {
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
}

impl DisplayBridge for X2GoManager {
    fn new(config: &AppVMConfig) -> Result<Self, Box<dyn Error>> {
        Self::ensure_x2goclient_available()?;

        Ok(Self {
            config: config.clone(),
        })
    }

    fn generate_exec_command(&self, app_command: &str) -> String {
        let vm_ip = match Self::get_vm_ip_static(&self.config.name) {
            Ok(ip) => ip,
            Err(_) => return String::from("echo 'Error: VM IP not found'"),
        };
        let ssh_key_path = match Self::get_ssh_key_path() {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => return String::from("echo 'Error: SSH key not found'"),
        };

        // Store session in x2go's config directory
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return String::from("echo 'Error: HOME not set'"),
        };
        let session_dir = Path::new(&home).join(".x2go").join("sessions");
        fs::create_dir_all(&session_dir).unwrap_or_default();

        let app_name = app_command
            .split_whitespace()
            .next()
            .unwrap_or("app")
            .replace('/', "-")
            .replace(' ', "-");
        let session_name = format!("{}-{}", self.config.name, app_name);
        let session_file_path = session_dir.join(format!("{}", session_name));

        let session_content = format!(
            r#"[{0}]
host={1}
user=user
sshport=22
useproxy=false
autologin=true
key={2}
command={3}
name={0}
rootless=true
sound=true
soundsystem=pulse
startsoundsystem=true
clipboard=both
"#,
            session_name, vm_ip, ssh_key_path, app_command
        );

        fs::write(&session_file_path, session_content).unwrap_or_default();

        format!("x2goclient --session={}", session_name)
    }

    fn generate_desktop_files(&self) -> Result<(), Box<dyn Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);
        fs::create_dir_all(&desktop_dir)?;

        println!("📝 Generating .desktop files in: {}", desktop_dir);

        for package in &self.config.system_packages {
            if self.is_application_package(package) {
                self.create_desktop_file(package, &desktop_dir, false)?;
            }
        }

        for package in &self.config.flatpak_packages {
            self.create_desktop_file(package, &desktop_dir, true)?;
        }

        println!(
            "✅ Generated .desktop files for {} applications",
            self.config.system_packages.len() + self.config.flatpak_packages.len()
        );

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

    fn launch_app(&self, app_command: &str) -> Result<(), Box<dyn Error>> {
        let exec_command = self.generate_exec_command(app_command);
        println!(
            "🚀 Launching {} in VM {} via X2Go",
            app_command, self.config.name
        );

        let mut parts = exec_command.split_whitespace();
        let client = parts.next().unwrap_or("x2goclient");
        let args: Vec<&str> = parts.collect();

        Command::new(client).args(&args).spawn()?;

        println!("✅ Application launched via X2Go");
        Ok(())
    }

    fn guest_packages(&self) -> Vec<String> {
        vec![
            "xorg-x11-server-Xorg".to_string(),
            "xorg-x11-xinit".to_string(),
            "i3".to_string(),
            "i3status".to_string(),
            "dmenu".to_string(),
            "rofi".to_string(),
            "x2goserver".to_string(),
            "x2goserver-xsession".to_string(),
            "pulseaudio".to_string(),
            "pulseaudio-utils".to_string(),
            "xclip".to_string(),
            "kitty".to_string(),
            "git".to_string(),
            "openssh-server".to_string(),
        ]
    }

    fn kickstart_config(&self, ssh_public_key: &str) -> String {
        format!(
            r#"
# Configure SSH, X2Go, and i3 for seamless window mode
echo "=== Configuring SSH, X2Go, and i3 for seamless mode ==="

# SSH key for passwordless authentication
mkdir -p /home/user/.ssh
chmod 700 /home/user/.ssh
cat > /home/user/.ssh/authorized_keys << 'SSH_KEY_EOF'
{0}
SSH_KEY_EOF
chmod 600 /home/user/.ssh/authorized_keys
chown -R user:user /home/user/.ssh

# Enable and start SSH and X2Go servers
systemctl enable sshd
systemctl start sshd
systemctl enable x2goserver
systemctl start x2goserver

# Allow SSH through firewall
firewall-cmd --permanent --add-service=ssh
firewall-cmd --reload

# Configure auto-login on tty1
mkdir -p /etc/systemd/system/getty@tty1.service.d
cat > /etc/systemd/system/getty@tty1.service.d/override.conf << 'AUTOLOGIN_EOF'
[Service]
ExecStart=
ExecStart=-/usr/sbin/agetty --autologin user --noclear %I $TERM
Type=idle
Restart=always
RestartSec=0
TTYVTDisallocate=yes
AUTOLOGIN_EOF
systemctl enable getty@tty1.service

# Auto-start X11 on tty1 login
cat >> /home/user/.bash_profile << 'X11_EOF'

# Auto-start X11 on tty1
if [ -z "$DISPLAY" ] && [ "$(tty)" = "/dev/tty1" ]; then
    exec startx
fi
X11_EOF

# Create .xinitrc to start i3
cat > /home/user/.xinitrc << 'XINIT_EOF'
#!/bin/sh
exec i3
XINIT_EOF

chown user:user /home/user/.bash_profile /home/user/.xinitrc
chmod +x /home/user/.xinitrc

# Enable user-level pulseaudio
sudo -u user systemctl --user enable pulseaudio

systemctl set-default multi-user.target
"#,
            ssh_public_key
        )
    }
}
