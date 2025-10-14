use std::fs;
use std::path::Path;
use std::process::Command;
use crate::config::AppVMConfig;

/// Xpra Manager for seamless window integration
/// Generates .desktop files and manages Xpra connections for per-application seamless windows
pub struct XpraManager {
    vm_name: String,
    vm_ip: String,
    system_packages: Vec<String>,
    flatpak_packages: Vec<String>,
}

impl XpraManager {
    pub fn new(config: &AppVMConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let vm_ip = Self::get_vm_ip_static(&config.name)?;

        Ok(Self {
            vm_name: config.name.clone(),
            vm_ip,
            system_packages: config.system_packages.clone(),
            flatpak_packages: config.flatpak_packages.clone(),
        })
    }

    /// Get VM IP address from libvirt
    fn get_vm_ip_static(vm_name: &str) -> Result<String, Box<dyn std::error::Error>> {
        // Get IP from virsh domifaddr using system connection (requires sudo)
        let output = Command::new("sudo")
            .args(&["virsh", "-c", "qemu:///system", "domifaddr", vm_name])
            .output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);

            // Parse IP from output like:
            // vnet0      52:54:00:12:34:56    ipv4         192.168.122.100/24
            for line in output_str.lines() {
                if line.contains("ipv4") {
                    if let Some(ip_part) = line.split_whitespace().nth(3) {
                        // Remove /24 suffix
                        if let Some(ip) = ip_part.split('/').next() {
                            return Ok(ip.to_string());
                        }
                    }
                }
            }
        }

        Err("Could not determine VM IP address. VM may not be running.".into())
    }

    /// Generate .desktop files for all installed applications
    pub fn generate_desktop_files(&self) -> Result<(), Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);
        fs::create_dir_all(&desktop_dir)?;

        println!("📝 Generating .desktop files in: {}", desktop_dir);

        // Generate desktop files for system packages
        for package in &self.system_packages {
            // Skip base packages that aren't applications
            if self.is_application_package(package) {
                self.create_desktop_file(package, &desktop_dir, false)?;
            }
        }

        // Generate desktop files for Flatpak packages
        for package in &self.flatpak_packages {
            self.create_desktop_file(package, &desktop_dir, true)?;
        }

        println!("✅ Generated .desktop files for {} applications",
                 self.system_packages.len() + self.flatpak_packages.len());

        Ok(())
    }

    /// Check if a package name is an application (not a library or dev package)
    fn is_application_package(&self, package: &str) -> bool {
        // Skip base system packages, libraries, and development packages
        let skip_patterns = [
            "i3", "xorg", "xset", "xrandr", "wmctrl", "xwininfo",
            "pipewire", "wl-clipboard", "spice-vdagent", "git",
            "xpra", "openssh-server", "dmenu", "rofi", "kitty",
            "-devel", "-libs", "lib"
        ];

        for pattern in &skip_patterns {
            if package.contains(pattern) {
                return false;
            }
        }

        true
    }

    /// Create a .desktop file for a specific application
    fn create_desktop_file(
        &self,
        package: &str,
        desktop_dir: &str,
        is_flatpak: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let app_name = if is_flatpak {
            // Extract nice name from flatpak ID: org.mozilla.firefox -> Firefox
            package.split('.').last().unwrap_or(package)
        } else {
            package
        };

        let exec_command = if is_flatpak {
            format!("flatpak run {}", package)
        } else {
            package.to_string()
        };

        let desktop_content = self.generate_desktop_content(
            app_name,
            &exec_command,
            package,
            is_flatpak,
        );

        let desktop_file = format!(
            "{}/{}-{}.desktop",
            desktop_dir,
            self.vm_name,
            app_name.to_lowercase()
        );

        fs::write(&desktop_file, desktop_content)?;

        // Make executable
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

    /// Generate .desktop file content with Xpra configuration
    fn generate_desktop_content(
        &self,
        app_name: &str,
        exec_command: &str,
        package: &str,
        is_flatpak: bool,
    ) -> String {
        let icon = if is_flatpak {
            // Try to use the flatpak ID as icon name
            package.to_lowercase()
        } else {
            app_name.to_lowercase()
        };

        let categories = self.guess_categories(package);

        // Build Xpra command to attach to persistent server via SSH
        // The Xpra server is running on :10 in the VM
        // Format: ssh://user@host/display (slash not colon!)
        let xpra_command = format!(
            "xpra attach ssh://user@{}/10 --start-child=\"{}\"",
            self.vm_ip,
            exec_command
        );

        format!(
            r#"[Desktop Entry]
Version=1.0
Type=Application
Name={app_name} (VM: {vm_name})
Comment=Running in isolated VM via Xpra
Icon={icon}
Exec={xpra_command}
Terminal=false
Categories={categories}
StartupWMClass={app_name}
Keywords=vm;isolated;sandbox;{package};
"#,
            app_name = app_name,
            vm_name = self.vm_name,
            icon = icon,
            xpra_command = xpra_command,
            categories = categories,
            package = package,
        )
    }

    /// Guess application categories based on package name
    fn guess_categories(&self, package: &str) -> &'static str {
        let lower = package.to_lowercase();

        if lower.contains("firefox") || lower.contains("chrome") ||
           lower.contains("browser") || lower.contains("librewolf") {
            "Network;WebBrowser;"
        } else if lower.contains("code") || lower.contains("editor") ||
                  lower.contains("ide") || lower.contains("vim") {
            "Development;IDE;"
        } else if lower.contains("gimp") || lower.contains("inkscape") ||
                  lower.contains("krita") {
            "Graphics;"
        } else if lower.contains("vlc") || lower.contains("media") ||
                  lower.contains("player") {
            "AudioVideo;Player;"
        } else if lower.contains("slack") || lower.contains("discord") ||
                  lower.contains("telegram") {
            "Network;InstantMessaging;"
        } else if lower.contains("torrent") || lower.contains("qbittorrent") {
            "Network;FileTransfer;"
        } else {
            "Utility;"
        }
    }

    /// Launch a specific application via Xpra seamless mode
    pub fn launch_app(&self, app_command: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Launching {} in VM {}", app_command, self.vm_name);

        // Attach to persistent Xpra server via SSH and launch the application
        // The Xpra server is running on :10 in the VM
        // Multiple apps can be launched in the same session, each appearing as separate windows
        // Format: ssh://user@host/display (slash not colon!)
        Command::new("xpra")
            .args(&[
                "attach",
                &format!("ssh://user@{}/10", self.vm_ip),
                &format!("--start-child={}", app_command),
                "--speaker-codec=opus",     // Opus has lowest latency
                "--microphone-codec=opus",  // Opus for mic too
                "--speaker=on",
                "--microphone=off",         // Disable mic by default
            ])
            .spawn()?;

        println!("✅ Application launched via Xpra");

        Ok(())
    }

    /// List all available applications in the VM
    pub fn list_applications(&self) -> Vec<String> {
        let mut apps = Vec::new();

        // Add system packages
        for package in &self.system_packages {
            if self.is_application_package(package) {
                apps.push(package.clone());
            }
        }

        // Add flatpak packages
        for package in &self.flatpak_packages {
            apps.push(format!("flatpak run {}", package));
        }

        apps
    }

    /// Remove all generated .desktop files for this VM
    pub fn remove_desktop_files(&self) -> Result<(), Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")?;
        let desktop_dir = format!("{}/.local/share/applications/vm-provisioner", home);

        if !Path::new(&desktop_dir).exists() {
            return Ok(());
        }

        // Remove all .desktop files matching this VM name
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
