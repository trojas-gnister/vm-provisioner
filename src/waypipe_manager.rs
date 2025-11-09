use crate::config::AppVMConfig;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// Waypipe manager handles desktop integration and per-app launching.
pub struct WaypipeManager {
    vm_name: String,
    vm_ip: String,
    system_packages: Vec<String>,
    flatpak_packages: Vec<String>,
    host_pulse_socket: String,
    remote_pulse_socket: String,
}

impl WaypipeManager {
    pub fn new(config: &AppVMConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Self::ensure_waypipe_available()?;
        let vm_ip = Self::get_vm_ip_static(&config.name)?;

        Ok(Self {
            vm_name: config.name.clone(),
            vm_ip,
            system_packages: config.system_packages.clone(),
            flatpak_packages: config.flatpak_packages.clone(),
            host_pulse_socket: Self::detect_host_pulse_socket(),
            remote_pulse_socket: "/run/user/1000/pulse/native".to_string(),
        })
    }

    fn ensure_waypipe_available() -> Result<(), Box<dyn std::error::Error>> {
        match Command::new("waypipe").arg("--version").output() {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(
                "waypipe binary not found on host. Install it (e.g., `sudo dnf install waypipe` or `sudo apt install waypipe`) before generating shortcuts or launching apps."
                    .into(),
            ),
            Err(e) => Err(format!("failed to execute waypipe: {}", e).into()),
        }
    }

    fn detect_host_pulse_socket() -> String {
        if let Ok(socket) = std::env::var("WAYPIPE_PULSE_SOCKET") {
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

    /// Get VM IP address from libvirt
    fn get_vm_ip_static(vm_name: &str) -> Result<String, Box<dyn std::error::Error>> {
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

    pub fn generate_desktop_files(&self) -> Result<(), Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);
        fs::create_dir_all(&desktop_dir)?;

        println!("📝 Generating .desktop files in: {}", desktop_dir);

        for package in &self.system_packages {
            if self.is_application_package(package) {
                self.create_desktop_file(package, &desktop_dir, false)?;
            }
        }

        for package in &self.flatpak_packages {
            self.create_desktop_file(package, &desktop_dir, true)?;
        }

        println!(
            "✅ Generated .desktop files for {} applications",
            self.system_packages.len() + self.flatpak_packages.len()
        );

        Ok(())
    }

    fn is_application_package(&self, package: &str) -> bool {
        let skip_patterns = [
            "sway",
            "waybar",
            "swaylock",
            "swayidle",
            "wl-clipboard",
            "pipewire",
            "git",
            "waypipe",
            "openssh-server",
            "seatd",
            "dmenu",
            "-devel",
            "-libs",
            "lib",
        ];

        for pattern in &skip_patterns {
            if package.contains(pattern) {
                return false;
            }
        }

        true
    }

    fn create_desktop_file(
        &self,
        package: &str,
        desktop_dir: &str,
        is_flatpak: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let desktop_content =
            self.generate_desktop_content(app_name, &exec_command, package, is_flatpak);

        let desktop_file = format!(
            "{}/{}-{}.desktop",
            desktop_dir,
            self.vm_name,
            app_name.to_lowercase()
        );

        fs::write(&desktop_file, desktop_content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&desktop_file)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&desktop_file, perms)?;
        }

        println!("   ✓ {}", desktop_file);

        Ok(())
    }

    fn generate_desktop_content(
        &self,
        app_name: &str,
        exec_command: &str,
        package: &str,
        _is_flatpak: bool,
    ) -> String {
        let icon = app_name.to_lowercase();
        let categories = self.guess_categories(package);
        let exec_line = format!(
            "waypipe --compress zstd ssh -R {remote}:{host} user@{ip} {cmd}",
            remote = self.remote_pulse_socket,
            host = self.host_pulse_socket,
            ip = self.vm_ip,
            cmd = exec_command
        );

        format!(
            r#"[Desktop Entry]
Version=1.0
Type=Application
Name={app_name} (VM: {vm_name})
Comment=Run in isolated VM via Waypipe
Icon={icon}
Exec={exec_line}
Terminal=false
Categories={categories}
StartupWMClass={app_name}
Keywords=vm;isolated;sandbox;{package};
"#,
            app_name = app_name,
            vm_name = self.vm_name,
            icon = icon,
            exec_line = exec_line,
            categories = categories,
            package = package,
        )
    }

    fn guess_categories(&self, package: &str) -> &'static str {
        let lower = package.to_lowercase();

        if lower.contains("firefox")
            || lower.contains("chrome")
            || lower.contains("browser")
            || lower.contains("librewolf")
        {
            "Network;WebBrowser;"
        } else if lower.contains("code")
            || lower.contains("editor")
            || lower.contains("ide")
            || lower.contains("vim")
        {
            "Development;IDE;"
        } else if lower.contains("gimp") || lower.contains("inkscape") || lower.contains("krita") {
            "Graphics;"
        } else if lower.contains("vlc") || lower.contains("media") || lower.contains("player") {
            "AudioVideo;Player;"
        } else if lower.contains("slack") || lower.contains("discord") || lower.contains("telegram")
        {
            "Network;InstantMessaging;"
        } else if lower.contains("torrent") || lower.contains("qbittorrent") {
            "Network;FileTransfer;"
        } else {
            "Utility;"
        }
    }

    pub fn launch_app(&self, app_command: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!(
            "🚀 Launching {} in VM {} via Waypipe",
            app_command, self.vm_name
        );

        Command::new("waypipe")
            .args(&[
                "--compress",
                "zstd",
                "ssh",
                "-R",
                &format!("{}:{}", self.remote_pulse_socket, self.host_pulse_socket),
                &format!("user@{}", self.vm_ip),
                app_command,
            ])
            .spawn()?;

        println!("✅ Application launched via Waypipe");

        Ok(())
    }

    pub fn list_applications(&self) -> Vec<String> {
        let mut apps = Vec::new();

        for package in &self.system_packages {
            if self.is_application_package(package) {
                apps.push(package.clone());
            }
        }

        for package in &self.flatpak_packages {
            apps.push(format!("flatpak run {}", package));
        }

        apps
    }

    pub fn remove_desktop_files(&self) -> Result<(), Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);

        if !Path::new(&desktop_dir).exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&desktop_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with(&format!("{}-", self.vm_name)) {
                    fs::remove_file(&path)?;
                    println!("   Removed {}", filename);
                }
            }
        }

        Ok(())
    }
}
