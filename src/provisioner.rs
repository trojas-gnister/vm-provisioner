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
        
        // Remove existing disk if it exists (with sudo)
        Command::new("sudo")
            .args(&["rm", "-f", &disk_path])
            .status()?;
        
        println!("💾 Creating VM disk ({} GB)...", self.config.disk_size_gb);
        
        Command::new("sudo")
            .args(&[
                "qemu-img", "create", "-f", "qcow2",
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
        
        // Build package list from system packages, separating build deps from runtime deps
        let mut base_packages = vec![
            "@core".to_string(),
            "@base-x".to_string(),
            "i3".to_string(),
            "i3status".to_string(),
            "i3lock".to_string(),
            "dmenu".to_string(),
            "rofi".to_string(),
            "xorg-x11-server-Xorg".to_string(),
            "xorg-x11-xinit".to_string(),
            "xset".to_string(),  // This is critical for X11 readiness check
            "xrandr".to_string(),
            "wmctrl".to_string(),
            "xwininfo".to_string(),
            "pipewire".to_string(),
            "wl-clipboard".to_string(),
            "spice-vdagent".to_string(),
            "kitty".to_string(),
            "git".to_string(), // Needed for cloning spice-autorandr
        ];
        
        // Add user-specified system packages (filter out build deps)
        for pkg in &self.config.system_packages {
            if !pkg.contains("-devel") && !pkg.contains("autoconf") && 
               !pkg.contains("automake") && !pkg.contains("libtool") &&
               !pkg.contains("pkgconfig") && !pkg.contains("gcc") && !pkg.contains("make") {
                base_packages.push(pkg.clone());
            }
        }
        
        let packages = base_packages.join("\n");
        
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
Wants=autologin@tty1.service

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

{}

# Set multi-user target as default (since we're using auto-login)
systemctl set-default multi-user.target"#,
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

# Enable comprehensive logging for debugging
set -x
exec > >(tee -a /var/log/kickstart-post-detailed.log) 2>&1
echo "=== Post-installation script started at $(date) ==="

# Check what packages were actually installed in the base install
echo "=== Checking installed packages ==="
rpm -qa | grep -E "(i3|xset|xrandr|kitty|git|rofi)" | sort

# Verify critical packages and install if missing
echo "=== Verifying critical packages ==="
MISSING_PACKAGES=()
for pkg in i3 xset xrandr kitty git rofi wmctrl xwininfo spice-vdagent; do
    if ! rpm -q $pkg &>/dev/null; then
        echo "Missing package: $pkg"
        MISSING_PACKAGES+=($pkg)
    else
        echo "Package installed: $pkg"
    fi
done

# Install any missing critical packages
if [ ${{#MISSING_PACKAGES[@]}} -gt 0 ]; then
    echo "=== Installing missing packages ==="
    dnf install -y "${{MISSING_PACKAGES[@]}}"
fi

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

# Final verification and status report
echo "=== FINAL VERIFICATION ==="
echo "Date: $(date)"
echo ""

echo "Critical packages status:"
for pkg in i3 xset xrandr kitty git rofi wmctrl xwininfo spice-vdagent; do
    if rpm -q $pkg &>/dev/null; then
        echo "✓ $pkg: INSTALLED"
    else
        echo "✗ $pkg: MISSING"
    fi
done

echo ""
echo "spice-autorandr status:"
if [ -f /usr/local/bin/spice-autorandr ]; then
    echo "✓ spice-autorandr: INSTALLED"
    ls -la /usr/local/bin/spice-autorandr
else
    echo "✗ spice-autorandr: MISSING"
fi

echo ""
echo "Auto-login service status:"
if [ -f /etc/systemd/system/autologin@.service ]; then
    echo "✓ autologin@.service: CONFIGURED"
else
    echo "✗ autologin@.service: MISSING"
fi

echo ""
echo "User configuration status:"
echo "User home directory contents:"
ls -la /home/user/
echo ""
echo "User .xinitrc exists:"
if [ -f /home/user/.xinitrc ]; then
    echo "✓ .xinitrc: EXISTS"
    echo "Owner: $(stat -c '%U:%G' /home/user/.xinitrc)"
else
    echo "✗ .xinitrc: MISSING"
fi

echo ""
echo "i3 config exists:"
if [ -f /home/user/.config/i3/config ]; then
    echo "✓ i3 config: EXISTS"
    echo "Auto-start apps in config:"
    grep -c "exec --no-startup-id" /home/user/.config/i3/config || echo "0"
else
    echo "✗ i3 config: MISSING"
fi

echo ""
echo "=== POST-INSTALL SCRIPT COMPLETED ==="
echo "Check logs at /var/log/kickstart-post.log and /var/log/kickstart-post-detailed.log"

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
                    // ARM64: Use virtio video with spice-autorandr for auto-resize
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
        
        let status = Command::new("sudo")
            .arg("virt-install")
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
        
        // Check if VM exists first
        let list_output = Command::new("virsh")
            .args(&["list", "--all"])
            .output()?;
        
        if !String::from_utf8_lossy(&list_output.stdout).contains(&self.config.name) {
            println!("   VM {} not found in virsh list", self.config.name);
            // Still try to clean up disk
        } else {
            // Force stop if running
            println!("   Force stopping VM...");
            let destroy_output = Command::new("virsh")
                .args(&["destroy", &self.config.name])
                .output();
            
            match destroy_output {
                Ok(output) => {
                    if output.status.success() {
                        println!("   VM stopped successfully");
                    } else {
                        println!("   VM stop failed or already stopped: {}", 
                                String::from_utf8_lossy(&output.stderr));
                    }
                }
                Err(e) => println!("   Error stopping VM: {}", e),
            }
            
            std::thread::sleep(std::time::Duration::from_secs(3));
            
            // Undefine VM (remove from libvirt)
            println!("   Removing VM definition...");
            let undefine_output = Command::new("virsh")
                .args(&["undefine", &self.config.name, "--remove-all-storage", "--nvram"])
                .output();
            
            match undefine_output {
                Ok(output) => {
                    if output.status.success() {
                        println!("   VM definition removed with storage");
                    } else {
                        println!("   Undefine with storage failed: {}", 
                                String::from_utf8_lossy(&output.stderr));
                        println!("   Trying without storage flags...");
                        
                        // Try simpler undefine
                        let simple_undefine = Command::new("virsh")
                            .args(&["undefine", &self.config.name])
                            .output()?;
                        
                        if simple_undefine.status.success() {
                            println!("   VM definition removed (without storage)");
                        } else {
                            println!("   Simple undefine also failed: {}", 
                                    String::from_utf8_lossy(&simple_undefine.stderr));
                        }
                    }
                }
                Err(e) => {
                    println!("   Error running undefine: {}", e);
                }
            }
        }
        
        // Remove disk manually
        let disk_path = format!("{}/{}.qcow2", self.config.vm_dir, self.config.name);
        if Path::new(&disk_path).exists() {
            println!("   Removing disk image: {}", disk_path);
            match fs::remove_file(&disk_path) {
                Ok(_) => println!("   ✅ Disk removed successfully"),
                Err(e) => {
                    println!("   Permission denied ({}), trying with sudo...", e);
                    let sudo_result = Command::new("sudo")
                        .args(&["rm", "-f", &disk_path])
                        .output();
                        
                    match sudo_result {
                        Ok(output) => {
                            if output.status.success() {
                                println!("   ✅ Disk removed with sudo");
                            } else {
                                println!("   ❌ Failed to remove disk even with sudo: {}", 
                                        String::from_utf8_lossy(&output.stderr));
                            }
                        }
                        Err(e) => println!("   ❌ Sudo command failed: {}", e),
                    }
                }
            }
        } else {
            println!("   Disk image not found at: {}", disk_path);
        }
        
        // Final verification
        let final_check = Command::new("virsh")
            .args(&["list", "--all"])
            .output()?;
        
        if String::from_utf8_lossy(&final_check.stdout).contains(&self.config.name) {
            println!("   ⚠️  Warning: VM still appears in virsh list");
            println!("   You may need to manually run: virsh undefine {}", self.config.name);
        } else {
            println!("   ✅ VM successfully removed from libvirt");
        }
        
        println!("✅ VM destruction completed");
        
        Ok(())
    }
    
    fn get_autologin_config(&self) -> String {
        if self.config.enable_auto_login {
            let mut result = r#"
# Configure auto-login with i3 via systemd
# Create auto-login service that starts X11 with i3
cat > /etc/systemd/system/autologin@.service << 'EOF'
[Unit]
Description=Auto Login for %i
After=systemd-user-sessions.service plymouth-quit-wait.service
After=plymouth-quit.service gdm.service
Before=getty@tty1.service

[Service]
ExecStart=-/sbin/agetty -o '-p -f user' --noclear --autologin user %i $TERM
Type=idle
Restart=always
RestartSec=0
UtmpIdentifier=%I
TTYPath=/dev/%i
TTYReset=yes
TTYVHangup=yes
TTYVTDisallocate=yes
KillMode=process
IgnoreSIGPIPE=no
SendSIGHUP=yes

[Install]
WantedBy=getty.target
EOF

# Enable auto-login on tty1
systemctl enable autologin@tty1.service

# Enable spice-vdagentd socket for auto-resize (starts daemon on demand)
systemctl enable spice-vdagentd.socket

# Create .xinitrc for user to start i3
cat > /home/user/.xinitrc << 'EOF'
#!/bin/bash

# Comprehensive logging for debugging
exec > /tmp/xinitrc.log 2>&1
echo "=== .xinitrc started at $(date) ==="
set -x

# Set up X11 environment
export DISPLAY=:0
export XDG_RUNTIME_DIR="/run/user/$(id -u)"

# Add Flatpak paths to XDG_DATA_DIRS for dmenu integration
export XDG_DATA_DIRS="/usr/local/share:/usr/share:/var/lib/flatpak/exports/share:$HOME/.local/share/flatpak/exports/share:$XDG_DATA_DIRS"

echo "Environment: DISPLAY=$DISPLAY, XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
echo "XDG_DATA_DIRS: $XDG_DATA_DIRS"

# Wait for X11 to be ready with timeout
echo "Waiting for X11 to be ready..."
timeout=30
count=0
while ! DISPLAY=:0 xset q &>/dev/null; do
    if [ $count -ge $timeout ]; then
        echo "X11 timeout after ${timeout}s, proceeding anyway..."
        break
    fi
    echo "X11 not ready, waiting... ($count/$timeout)"
    sleep 1
    count=$((count + 1))
done
echo "X11 check completed (timeout: $count/$timeout)"

# Ensure X11 authority is properly set
echo "Setting X11 authority..."
xauth generate :0 . trusted
echo "X11 authority set"

# Start SPICE agent user session (system daemon should already be running)
if command -v spice-vdagent >/dev/null 2>&1; then
    echo "Starting spice-vdagent..."
    DISPLAY=:0 XDG_RUNTIME_DIR="/run/user/$(id -u)" spice-vdagent &
    sleep 1
    echo "spice-vdagent started"
else
    echo "spice-vdagent not found!"
fi

# Check i3 before starting
echo "Checking i3 installation..."
which i3
i3 --version

# Start i3 window manager
echo "About to exec i3..."
exec i3
EOF
chmod +x /home/user/.xinitrc
chown user:user /home/user/.xinitrc

# Auto-start X11 when user logs into tty1
cat > /home/user/.bash_profile << 'EOF'
# Debug autologin
echo "bash_profile executed at $(date)" >> /tmp/autologin.log
echo "Current tty: $(tty)" >> /tmp/autologin.log
echo "DISPLAY: $DISPLAY" >> /tmp/autologin.log
echo "XDG_VTNR: $XDG_VTNR" >> /tmp/autologin.log

# Auto-start X11 on tty1 login
if [[ -z $DISPLAY ]]; then
    # Check if we're on tty1 (multiple ways to detect)
    if [[ $(tty) == "/dev/tty1" ]] || [[ "$XDG_VTNR" -eq 1 ]] || [[ $(fgconsole 2>/dev/null) -eq 1 ]]; then
        echo "Starting X11 on tty1..." | tee -a /tmp/autologin.log
        exec startx -- vt1
    else
        echo "Not on tty1, not starting X11" >> /tmp/autologin.log
    fi
else
    echo "DISPLAY already set, not starting X11" >> /tmp/autologin.log
fi
EOF
chown user:user /home/user/.bash_profile

# Create systemd user service as fallback for X11 startup
mkdir -p /home/user/.config/systemd/user
cat > /home/user/.config/systemd/user/startx.service << 'EOF'
[Unit]
Description=Start X11 session
After=graphical-session-pre.target

[Service]
Type=oneshot
ExecStart=/usr/bin/startx
Environment=DISPLAY=:0
Restart=no

[Install]
WantedBy=default.target
EOF

# Create user cache directory and fix permissions
mkdir -p /home/user/.cache
mkdir -p /home/user/.local/share
mkdir -p /home/user/.local/bin

# Fix ownership of all user directories
chown -R user:user /home/user/.config
chown -R user:user /home/user/.cache
chown -R user:user /home/user/.local
chown -R user:user /home/user/.*

# Enable the user service (will be activated when user session starts)
sudo -u user systemctl --user enable startx.service

# Create default i3 config
mkdir -p /home/user/.config/i3
cat > /home/user/.config/i3/config << 'EOF'
# i3 config file
set $mod Mod4

# Font for window titles
font pango:DejaVu Sans Mono 8

# Use Mouse+$mod to drag floating windows
floating_modifier $mod

# Start a terminal
bindsym $mod+Return exec kitty

# Kill focused window
bindsym $mod+Shift+q kill

# Start rofi (app launcher) - better Flatpak support than dmenu
bindsym $mod+d exec rofi -show drun -p "Applications"

# Alternative: traditional dmenu 
bindsym $mod+Shift+d exec dmenu_run -p "Run:" -fn "DejaVu Sans Mono-10"

# Change focus
bindsym $mod+j focus left
bindsym $mod+k focus down
bindsym $mod+l focus up
bindsym $mod+semicolon focus right
bindsym $mod+Left focus left
bindsym $mod+Down focus down
bindsym $mod+Up focus up
bindsym $mod+Right focus right

# Move focused window
bindsym $mod+Shift+j move left
bindsym $mod+Shift+k move down
bindsym $mod+Shift+l move up
bindsym $mod+Shift+semicolon move right
bindsym $mod+Shift+Left move left
bindsym $mod+Shift+Down move down
bindsym $mod+Shift+Up move up
bindsym $mod+Shift+Right move right

# Workspaces
bindsym $mod+1 workspace 1
bindsym $mod+2 workspace 2
bindsym $mod+3 workspace 3
bindsym $mod+4 workspace 4
bindsym $mod+5 workspace 5

# Move container to workspace
bindsym $mod+Shift+1 move container to workspace 1
bindsym $mod+Shift+2 move container to workspace 2
bindsym $mod+Shift+3 move container to workspace 3
bindsym $mod+Shift+4 move container to workspace 4
bindsym $mod+Shift+5 move container to workspace 5

# Restart i3
bindsym $mod+Shift+r restart

# Exit i3
bindsym $mod+Shift+e exec "i3-nagbar -t warning -m 'Exit i3?' -b 'Yes' 'i3-msg exit'"

# Status bar
bar {
    status_command i3status
}

# Auto-start applications
EOF

# Add auto-start commands for installed applications"#.to_string();

            // Add auto-start commands for each application
            for app_command in &self.config.auto_launch_apps {
                result.push_str(&format!("\necho \"exec --no-startup-id {}\" >> /home/user/.config/i3/config", app_command));
            }

            result.push_str(r#"

# Final comprehensive ownership fix for all user directories
chown -R user:user /home/user/.config
chown -R user:user /home/user/.cache
chown -R user:user /home/user/.local
chown -R user:user /home/user/.xinitrc
chown -R user:user /home/user/.bash_profile

# Ensure proper permissions for user directories
chmod 755 /home/user/.config
chmod 755 /home/user/.cache
chmod 755 /home/user/.local

# Install build dependencies for spice-autorandr (must be done in post-install)
echo "Installing build dependencies for spice-autorandr..."
dnf install -y gcc make autoconf automake libtool libXrandr-devel libX11-devel systemd-devel pkgconfig xorg-x11-proto-devel xorg-x11-util-macros

# Install and configure spice-autorandr for automatic resolution adjustment
echo "Building spice-autorandr..."
cd /tmp
if git clone https://github.com/seife/spice-autorandr.git; then
    cd spice-autorandr
    if autoreconf -is; then
        if ./configure; then
            if make; then
                cp spice-autorandr /usr/local/bin/
                chmod +x /usr/local/bin/spice-autorandr
                echo "spice-autorandr installed successfully"
            else
                echo "ERROR: spice-autorandr make failed"
            fi
        else
            echo "ERROR: spice-autorandr configure failed"
        fi
    else
        echo "ERROR: spice-autorandr autoreconf failed"
    fi
else
    echo "ERROR: spice-autorandr git clone failed"
fi

# Create systemd service for spice-autorandr
cat > /etc/systemd/system/spice-autorandr.service << 'EOF'
[Unit]
Description=SPICE Auto Resolution Adjustment
After=multi-user.target
Wants=multi-user.target

[Service]
Type=simple
ExecStart=/usr/local/bin/spice-autorandr
Restart=always
RestartSec=5
Environment=DISPLAY=:0
Environment=XDG_RUNTIME_DIR=/run/user/1000
User=user
Group=user
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

# Enable the spice-autorandr service
systemctl enable spice-autorandr.service"#);

            result
        } else {
            "".to_string()
        }
    }
}
