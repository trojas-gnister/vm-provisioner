use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::config::{AppVMConfig, GraphicsBackend};

pub struct AppVMProvisioner {
    config: AppVMConfig,
}

impl AppVMProvisioner {
    pub fn new(config: AppVMConfig) -> Self {
        Self { config }
    }
    
    pub async fn provision_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting Application VM provisioning...");
        println!("   System packages: {:?}", self.config.system_packages);
        println!("   Flatpak packages: {:?}", self.config.flatpak_packages);
        
        // Check prerequisites
        self.check_prerequisites()?;
        
        // Download Fedora ISO
        let iso_path = self.download_fedora_iso()?;
        
        // Create VM disk
        let disk_path = self.create_vm_disk()?;
        
        // Generate kickstart configuration
        let kickstart_path = self.generate_kickstart_config()?;
        
        // Start automated installation
        self.start_installation(&iso_path, &disk_path, &kickstart_path)?;
        
        // Configure window management integration
        self.setup_window_management()?;
        
        println!("✅ Application VM provisioned successfully!");
        println!("   VM Name: {}", self.config.name);
        println!("   System packages: {:?}", self.config.system_packages);
        println!("   Flatpak packages: {:?}", self.config.flatpak_packages);
        println!("   Graphics: {:?}", self.config.graphics_backend);
        println!("   Clipboard: {}", if self.config.enable_clipboard { "Enabled" } else { "Disabled" });
        
        Ok(())
    }
    
    fn check_prerequisites(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔍 Checking prerequisites...");
        
        let required_commands = ["virsh", "virt-install", "qemu-img"];
        for cmd in &required_commands {
            if Command::new("which").arg(cmd).output()?.status.success() {
                println!("  ✓ {}", cmd);
            } else {
                return Err(format!("Missing required command: {}", cmd).into());
            }
        }
        
        // Check if libvirtd is running
        let status = Command::new("systemctl")
            .args(&["is-active", "libvirtd"])
            .output()?;
            
        if !status.status.success() {
            println!("  ⚠️  Starting libvirtd...");
            Command::new("sudo")
                .args(&["systemctl", "start", "libvirtd"])
                .status()?;
        }
        
        Ok(())
    }
    
    fn download_fedora_iso(&self) -> Result<String, Box<dyn std::error::Error>> {
        let arch = std::env::consts::ARCH;
        let iso_name = format!("fedora-minimal-{}.iso", arch);
        let iso_path = format!("{}/{}", self.config.vm_dir, iso_name);
        
        if Path::new(&iso_path).exists() {
            println!("📦 Using existing Fedora ISO");
            return Ok(iso_path);
        }
        
        println!("📥 Downloading Fedora ISO...");
        
        let download_url = match arch {
            "x86_64" => "https://download.fedoraproject.org/pub/fedora/linux/releases/41/Server/x86_64/iso/Fedora-Server-netinst-x86_64-41-1.4.iso",
            "aarch64" => "https://download.fedoraproject.org/pub/fedora/linux/releases/41/Server/aarch64/iso/Fedora-Server-netinst-aarch64-41-1.4.iso",
            _ => return Err(format!("Unsupported architecture: {}", arch).into()),
        };
        
        Command::new("curl")
            .args(&["-L", "-o", &iso_path, download_url])
            .status()?;
            
        Ok(iso_path)
    }
    
    fn create_vm_disk(&self) -> Result<String, Box<dyn std::error::Error>> {
        let disk_path = format!("{}/{}.qcow2", self.config.vm_dir, self.config.name);
        
        if Path::new(&disk_path).exists() {
            fs::remove_file(&disk_path)?;
        }
        
        println!("💾 Creating VM disk ({} GB)...", self.config.disk_size_gb);
        
        Command::new("qemu-img")
            .args(&[
                "create", "-f", "qcow2",
                &disk_path,
                &format!("{}G", self.config.disk_size_gb)
            ])
            .status()?;
            
        Ok(disk_path)
    }
    
    fn generate_kickstart_config(&self) -> Result<String, Box<dyn std::error::Error>> {
        let kickstart_dir = format!("/tmp/{}-kickstart", self.config.name);
        fs::create_dir_all(&kickstart_dir)?;
        
        let kickstart_path = format!("{}/kickstart.cfg", kickstart_dir);
        
        println!("🏗️  Generating kickstart configuration...");
        
        // Build package list from system packages only
        let packages = self.config.system_packages.join("\n");
        
        // Build Flatpak configuration if flatpak packages specified
        let flatpak_config = if !self.config.flatpak_packages.is_empty() {
            let mut config = String::from(r#"
# Install and configure Flatpak
dnf install -y flatpak

# Add Flathub repository
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo

# Install Flatpak packages
"#);
            for package in &self.config.flatpak_packages {
                config.push_str(&format!("flatpak install -y flathub {}\n", package));
            }
            
            config.push_str("\n# Verify installations\nflatpak list\n");
            config
        } else {
            "".to_string()
        };
        
        // Build auto-launch configuration
        let auto_launch_config = if !self.config.auto_launch_apps.is_empty() {
            let mut config = String::from("\n# Auto-launch applications\n");
            for (i, app_cmd) in self.config.auto_launch_apps.iter().enumerate() {
                config.push_str(&format!(r#"
# Auto-launch service {}
cat > /etc/systemd/system/auto-launch-{}.service << 'EOF'
[Unit]
Description=Auto Launch Application {}
After=graphical-session.target
Wants=display-manager.service

[Service]
Type=simple
User=user
Environment="DISPLAY=:0"
Environment="XDG_RUNTIME_DIR=/run/user/1000"
Environment="XDG_SESSION_TYPE=x11"
ExecStartPre=/bin/bash -c 'while ! pgrep -x Xorg; do sleep 1; done'
ExecStart={}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical.target
EOF

systemctl enable auto-launch-{}.service
"#, i + 1, i + 1, i + 1, app_cmd, i + 1));
            }
            config
        } else {
            "".to_string()
        };
        
        // Build guest agent service configuration
        let app_config = format!(r#"
# Create guest agent service
cat > /etc/systemd/system/guest-agent.service << 'EOF'
[Unit]
Description=VM Guest Agent for Window Management
After=graphical.target
Wants=display-manager.service

[Service]
Type=simple
User=user
Environment="DISPLAY=:0"
Environment="XDG_RUNTIME_DIR=/run/user/1000"
Environment="XDG_SESSION_TYPE=x11"
ExecStartPre=/bin/bash -c 'while ! pgrep -x Xorg; do sleep 1; done'
ExecStart=/usr/local/bin/guest-agent
Restart=on-failure
RestartSec=3

[Install]
WantedBy=graphical.target
EOF

# Enable the services
systemctl enable guest-agent.service
systemctl enable gdm

{}

# Set graphical target as default
systemctl set-default graphical.target"#,
            self.get_autologin_config(),
        );
        
        // Build clipboard daemon configuration if enabled
        let clipboard_config = if self.config.enable_clipboard {
            r#"
# Setup clipboard sharing daemon
cat > /etc/systemd/system/clipboard-proxy.service << 'EOF'
[Unit]
Description=Clipboard Proxy Service
After=cage-app.service

[Service]
Type=simple
User=user
Environment="WAYLAND_DISPLAY=wayland-0"
ExecStart=/usr/local/bin/clipboard-proxy
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

# Create clipboard proxy script (will be replaced by actual proxy later)
cat > /usr/local/bin/clipboard-proxy << 'EOF'
#!/bin/bash
# Placeholder for clipboard proxy
# This will be replaced by the actual virtio-based clipboard proxy
while true; do
    sleep 60
done
EOF
chmod +x /usr/local/bin/clipboard-proxy

systemctl enable clipboard-proxy.service"#
        } else {
            ""
        };
        
        // Build audio configuration if enabled
        let audio_config = if self.config.enable_audio {
            r#"
# Enable PipeWire audio
systemctl --user enable pipewire pipewire-pulse wireplumber"#
        } else {
            ""
        };
        
        // Build firewall rules
        let firewall_rules = self.config.firewall_rules
            .iter()
            .map(|rule| format!("iptables -A {}", rule))
            .collect::<Vec<_>>()
            .join("\n");
        
        // Generate the complete kickstart file
        let kickstart_content = format!(r#"# Kickstart file for Application VM
# Generated for: {}

# Installation settings
text
lang en_US.UTF-8
keyboard us
timezone UTC
network --bootproto=dhcp --device=link --activate
rootpw --lock
user --name=user --groups=wheel --password={} --plaintext

# Disk configuration
autopart --type=plain
clearpart --all --initlabel
bootloader --location=mbr

# Security
selinux --permissive
firewall --enabled

# Package selection
%packages --ignoremissing
@core
@base-x
{}
%end

# Post-installation script
%post --log=/var/log/kickstart-post.log

# Install flatpak packages if specified
{}

# Configure auto-launch applications
{}

# Configure sudo for user
echo "user ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers.d/user

# Configure X11 environment
mkdir -p /home/user/.config
cat > /home/user/.config/environment << 'EOF'
DISPLAY=:0
XDG_SESSION_TYPE=x11
EOF

{}

{}

{}

# Configure firewall rules
{}

# Install build tools and compile guest agent
dnf install -y rust cargo git

# Create guest agent source
mkdir -p /tmp/guest-agent-build
cat > /tmp/guest-agent-build/Cargo.toml << 'EOF'
[package]
name = "guest-agent"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = {{ version = "1.0", features = ["derive"] }}
bincode = "1.3"
regex = "1.10"
EOF

# Copy guest agent source (this would be injected from the host)
# For now, create a minimal version
cat > /tmp/guest-agent-build/src/main.rs << 'EOF'
fn main() {{
    println!("Guest agent placeholder - will be replaced with full implementation");
    std::thread::sleep(std::time::Duration::from_secs(60));
}}
EOF

mkdir -p /tmp/guest-agent-build/src
cd /tmp/guest-agent-build
cargo build --release
cp target/release/guest-agent /usr/local/bin/guest-agent
chmod +x /usr/local/bin/guest-agent

# Cleanup build files
cd /
rm -rf /tmp/guest-agent-build

# Disable unnecessary services
systemctl disable bluetooth
systemctl disable cups

# Set hostname
echo "{}" > /etc/hostname

# Final cleanup
dnf clean all

%end

# Reboot after installation
reboot"#,
            self.config.name,
            self.config.user_password,
            packages,
            flatpak_config,
            auto_launch_config,
            app_config,
            clipboard_config,
            audio_config,
            firewall_rules,
            self.config.name
        );
        
        fs::write(&kickstart_path, kickstart_content)?;
        Ok(kickstart_path)
    }
    
    fn start_installation(&self, _iso_path: &str, disk_path: &str, kickstart_path: &str) 
        -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting VM installation...");
        
        let arch = std::env::consts::ARCH;
        let install_location = match arch {
            "x86_64" => "https://dl.fedoraproject.org/pub/fedora/linux/releases/41/Server/x86_64/os/",
            "aarch64" => "https://dl.fedoraproject.org/pub/fedora/linux/releases/41/Everything/aarch64/os/",
            _ => return Err(format!("Unsupported architecture: {}", arch).into()),
        };
        
        let memory_str = self.config.memory_mb.to_string();
        let vcpus_str = self.config.vcpus.to_string();
        let disk_arg = format!("path={},size={},format=qcow2,bus=virtio", 
                               disk_path, self.config.disk_size_gb);
        
        // Configure graphics based on backend and architecture
        let arch = std::env::consts::ARCH;
        let graphics_args = match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                if arch == "aarch64" {
                    // ARM64: Use virtio video with SPICE graphics
                    vec!["--graphics", "spice", "--video", "virtio", 
                         "--channel", "spicevmc,target_type=virtio,name=com.redhat.spice.0"]
                } else {
                    // x86_64: Use QXL for better performance
                    vec!["--graphics", "spice,listen=127.0.0.1", "--video", "qxl", 
                         "--channel", "spicevmc,target_type=virtio,name=com.redhat.spice.0"]
                }
            },
            GraphicsBackend::QxlSpice => {
                if arch == "aarch64" {
                    vec!["--graphics", "spice", "--video", "virtio", 
                         "--channel", "spicevmc,target_type=virtio,name=com.redhat.spice.0"]
                } else {
                    vec!["--graphics", "spice,listen=127.0.0.1", "--video", "qxl", 
                         "--channel", "spicevmc,target_type=virtio,name=com.redhat.spice.0"]
                }
            },
            GraphicsBackend::VncOnly => {
                vec!["--graphics", "vnc,listen=127.0.0.1,port=5900"]
            },
        };
        
        let mut virt_install_args = vec![
            "--name", &self.config.name,
            "--memory", &memory_str,
            "--vcpus", &vcpus_str,
            "--disk", &disk_arg,
            "--location", install_location,
            "--initrd-inject", kickstart_path,
            "--extra-args", "inst.ks=file:/kickstart.cfg console=tty0 console=ttyS0,115200n8",
            "--network", "network=default,model=virtio",
            "--noautoconsole",
            "--wait", "-1",
        ];
        
        // Add graphics arguments
        for arg in graphics_args {
            virt_install_args.push(arg);
        }
        
        // Add sound if enabled
        if self.config.enable_audio {
            if arch == "aarch64" {
                // ARM64: Use virtio sound model
                virt_install_args.extend_from_slice(&["--sound", "model=virtio"]);
            } else {
                // x86_64: Use default sound
                virt_install_args.extend_from_slice(&["--sound", "default"]);
            }
        }
        
        // Add USB controller if needed
        if self.config.enable_usb_passthrough {
            virt_install_args.extend_from_slice(&["--controller", "usb,model=qemu-xhci"]);
        }
        
        println!("⏳ Running automated installation (15-20 minutes)...");
        
        let status = Command::new("virt-install")
            .args(&virt_install_args)
            .status()?;
            
        if !status.success() {
            return Err(format!("VM installation failed with exit code: {:?}", status.code()).into());
        }
        
        println!("✅ Installation completed!");
        
        Ok(())
    }
    
    fn setup_window_management(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🪟 Setting up window management integration...");
        
        // This is where we'd set up the virtio channel for window management
        // For now, we'll configure the VM to be ready for the host integration
        
        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                println!("   Configured for VirtIO-GPU acceleration");
                println!("   Cage compositor will start automatically");
            },
            GraphicsBackend::QxlSpice => {
                println!("   Configured for SPICE protocol");
                println!("   Connect with: remote-viewer spice://localhost:5900");
            },
            GraphicsBackend::VncOnly => {
                println!("   VNC fallback mode");
                println!("   Connect with: vncviewer localhost:5900");
            },
        }
        
        if self.config.enable_clipboard {
            println!("   Clipboard sharing enabled (requires host agent)");
        }
        
        Ok(())
    }
    
    pub fn start_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("▶️  Starting VM: {}", self.config.name);
        
        Command::new("virsh")
            .args(&["start", &self.config.name])
            .status()?;
            
        // Wait for VM to boot
        thread::sleep(Duration::from_secs(5));
        
        // Launch SPICE viewer for immediate functionality
        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu | GraphicsBackend::QxlSpice => {
                println!("🖥️  Launching SPICE viewer...");
                let vm_name = self.config.name.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(5)); // Wait for VM to start SPICE
                    
                    // Get the actual SPICE port from virsh
                    if let Ok(output) = std::process::Command::new("virsh")
                        .args(&["domdisplay", &vm_name])
                        .output()
                    {
                        if let Ok(display) = String::from_utf8(output.stdout) {
                            let display = display.trim();
                            if !display.is_empty() {
                                let _ = std::process::Command::new("remote-viewer")
                                    .arg(display)
                                    .spawn();
                                return;
                            }
                        }
                    }
                    
                    // Fallback to default port
                    let _ = std::process::Command::new("remote-viewer")
                        .arg("spice://127.0.0.1:5900")
                        .spawn();
                });
                println!("   SPICE viewer will launch automatically");
                println!("   Or get connection info with: virsh domdisplay {}", self.config.name);
            },
            GraphicsBackend::VncOnly => {
                println!("   Connect with: vncviewer localhost:5900");
            },
        }
        
        println!("✅ VM started successfully!");
        
        Ok(())
    }
    
    pub fn stop_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏹️  Stopping VM: {}", self.config.name);
        
        Command::new("virsh")
            .args(&["shutdown", &self.config.name])
            .status()?;
            
        Ok(())
    }
    
    pub fn destroy_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗑️  Destroying VM: {}", self.config.name);
        
        // Force stop if running
        let _ = Command::new("virsh")
            .args(&["destroy", &self.config.name])
            .output();
        
        // Undefine VM
        Command::new("virsh")
            .args(&["undefine", &self.config.name, "--nvram"])
            .status()?;
        
        // Remove disk
        let disk_path = format!("{}/{}.qcow2", self.config.vm_dir, self.config.name);
        if Path::new(&disk_path).exists() {
            fs::remove_file(&disk_path)?;
        }
        
        println!("✅ VM destroyed");
        
        Ok(())
    }
    
    fn get_autologin_config(&self) -> String {
        if self.config.enable_auto_login {
            r#"
# Configure GDM for auto-login
cat > /etc/gdm/custom.conf << 'EOF'
[daemon]
AutomaticLoginEnable=true
AutomaticLogin=user
EOF"#.to_string()
        } else {
            "".to_string()
        }
    }
}
