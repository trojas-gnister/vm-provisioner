use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::config::{AppVMConfig, GraphicsBackend, PciDevice};

pub struct AppVMProvisioner {
    config: AppVMConfig,
}

/// Public function to detect and validate a PCI device
pub fn detect_pci_device(address: &str) -> Result<PciDevice, Box<dyn std::error::Error>> {
    // 1. Check device exists
    let lspci_output = Command::new("lspci")
        .args(&["-s", address, "-nn", "-k"])
        .output()?;

    if !lspci_output.status.success() || lspci_output.stdout.is_empty() {
        return Err(format!("PCI device {} not found. Run 'lspci' to see available devices.", address).into());
    }

    let output_str = String::from_utf8_lossy(&lspci_output.stdout);
    let first_line = output_str.lines().next().unwrap_or("");

    // 2. Parse vendor:device IDs from lspci output
    // Format: 01:00.0 VGA compatible controller [0300]: NVIDIA Corporation [10de:1c03] (rev a1)
    let (vendor_id, device_id) = parse_vendor_device_ids(&output_str)?;

    // 3. Extract description
    let description = if let Some(desc_start) = first_line.find(": ") {
        let desc = &first_line[desc_start + 2..];
        // Remove the [vendor:device] part if present
        if let Some(bracket_pos) = desc.find(" [") {
            desc[..bracket_pos].to_string()
        } else {
            desc.to_string()
        }
    } else {
        "Unknown device".to_string()
    };

    // 4. Get current driver
    let original_driver = get_current_driver(address);

    // 5. Get IOMMU group
    let iommu_group = get_iommu_group(address);

    Ok(PciDevice {
        address: address.to_string(),
        vendor_id,
        device_id,
        description,
        original_driver,
        iommu_group,
    })
}

fn parse_vendor_device_ids(lspci_output: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    // Look for pattern like [10de:1c03]
    if let Some(start) = lspci_output.find('[') {
        if let Some(end) = lspci_output[start..].find(']') {
            let ids = &lspci_output[start + 1..start + end];
            if let Some(colon_pos) = ids.find(':') {
                let vendor = ids[..colon_pos].to_string();
                let device = ids[colon_pos + 1..].to_string();
                return Ok((vendor, device));
            }
        }
    }
    Err("Could not parse vendor:device IDs from lspci output".into())
}

fn get_current_driver(address: &str) -> Option<String> {
    let driver_path = format!("/sys/bus/pci/devices/{}/driver", address);
    fs::read_link(&driver_path)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}

fn get_iommu_group(address: &str) -> Option<u32> {
    let iommu_path = format!("/sys/bus/pci/devices/{}/iommu_group", address);
    fs::read_link(&iommu_path)
        .ok()
        .and_then(|p| p.file_name()
            .and_then(|n| n.to_string_lossy().parse::<u32>().ok()))
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

        // Validate PCI passthrough if devices specified
        if !self.config.pci_devices.is_empty() {
            self.validate_pci_passthrough()?;
        }
        
        // Download Alpine ISO
        let iso_path = self.download_alpine_iso()?;
        
        // Create VM disk
        let disk_path = self.create_vm_disk()?;
        
        // Generate kickstart configuration
        let kickstart_path = self.generate_kickstart_config()?;
        
        // Start automated installation
        self.start_installation(&iso_path, &disk_path, &kickstart_path)?;
        
        // Configure window management integration
        self.setup_window_management()?;

        // Setup PCI passthrough if devices specified (permanent mode)
        if !self.config.pci_devices.is_empty() && !self.config.pci_hotplug {
            self.setup_pci_passthrough_permanent()?;
        }

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
    
    fn download_alpine_iso(&self) -> Result<String, Box<dyn std::error::Error>> {
        let arch = std::env::consts::ARCH;
        let version = "3.22.2";
        let iso_name = format!("alpine-virt-{}-{}.iso", version, arch);
        let iso_path = format!("{}/{}", self.config.vm_dir, iso_name);

        if Path::new(&iso_path).exists() {
            println!("📦 Using existing Alpine ISO");
            return Ok(iso_path);
        }

        println!("📥 Downloading Alpine ISO (~50MB)...");

        let download_url = match arch {
            "x86_64" => format!("https://dl-cdn.alpinelinux.org/alpine/v{}/releases/x86_64/alpine-virt-{}-x86_64.iso",
                               "3.22", version),
            "aarch64" => format!("https://dl-cdn.alpinelinux.org/alpine/v{}/releases/aarch64/alpine-virt-{}-aarch64.iso",
                                "3.22", version),
            _ => return Err(format!("Unsupported architecture: {}", arch).into()),
        };

        Command::new("curl")
            .args(&["-L", "-o", &iso_path, &download_url])
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
        let mut base_packages = if self.config.headless {
            // Headless mode: minimal packages only
            vec![
                "@core".to_string(),
                "git".to_string(),
            ]
        } else {
            // GUI mode: full desktop environment
            vec![
                "@core".to_string(),
                "@base-x".to_string(),
                "i3".to_string(),
                "i3status".to_string(),
                "i3lock".to_string(),
                "dmenu".to_string(),
                "rofi".to_string(),
                "xorg-x11-server-Xorg".to_string(),
                "xorg-x11-xinit".to_string(),
                "pipewire".to_string(),
                "spice-vdagent".to_string(),
                "kitty".to_string(),
                "git".to_string(),
            ]
        };
        
        // Add user-specified system packages (filter out build deps)
        for pkg in &self.config.system_packages {
            if !pkg.contains("-devel") && !pkg.contains("autoconf") && 
               !pkg.contains("automake") && !pkg.contains("libtool") &&
               !pkg.contains("pkgconfig") && !pkg.contains("gcc") && !pkg.contains("make") {
                base_packages.push(pkg.clone());
            }
        }
        
        let packages = base_packages.join("\n");
        
        // Build Flatpak configuration if flatpak packages specified (GUI mode only)
        let flatpak_config = if !self.config.headless && !self.config.flatpak_packages.is_empty() {
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
        
        // Build auto-launch configuration (GUI mode only)
        let auto_launch_config = if !self.config.headless && !self.config.auto_launch_apps.is_empty() {
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
        
        // Build guest agent service configuration (GUI mode only)
        let app_config = if !self.config.headless {
            format!(r#"
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
        )
        } else {
            // Headless mode: no GUI services
            "".to_string()
        };
        
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

        // Read SSH public key from host for passwordless authentication
        let ssh_public_key = self.get_ssh_public_key()?;

        // Build SSH and Xpra configuration for seamless window integration (GUI mode only)
        let ssh_xpra_config = if !self.config.headless {
            format!(r#"
# Configure SSH and Xpra for seamless window mode
echo "=== Configuring SSH and Xpra for seamless mode ==="

# Set up SSH key for passwordless authentication
mkdir -p /home/{0}/.ssh
chmod 700 /home/{0}/.ssh
cat > /home/{0}/.ssh/authorized_keys << 'SSH_KEY_EOF'
{1}
SSH_KEY_EOF
chmod 600 /home/{0}/.ssh/authorized_keys
chown -R {0}:{0} /home/{0}/.ssh

# Enable and start SSH server
systemctl enable sshd
systemctl start sshd

# Allow SSH through firewall
firewall-cmd --permanent --add-service=ssh
firewall-cmd --reload

# Create Xpra configuration directory
mkdir -p /etc/xpra/conf.d

# Configure Xpra for seamless window support
cat > /etc/xpra/conf.d/60_seamless.conf << 'XPRA_EOF'
# Seamless window mode configuration
start-new-commands = yes
start = i3
bind-tcp = 0.0.0.0:14500

# Audio support
pulseaudio = yes
speaker = on
microphone = on

# Clipboard support
clipboard = yes

# Performance optimizations
encoding = rgb
compression = 0
speed = 100
min-speed = 50

# Window management
window-close = disconnect
exit-with-children = yes
XPRA_EOF

echo "=== SSH and Xpra configuration complete ==="
"#, "user", ssh_public_key)
        } else {
            String::new()
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
for pkg in i3 kitty git rofi spice-vdagent; do
    if rpm -q $pkg &>/dev/null; then
        echo "✓ $pkg: INSTALLED"
    else
        echo "✗ $pkg: MISSING"
    fi
done

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
            ssh_xpra_config,
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
        
        // Configure graphics based on backend, architecture, and headless mode
        let arch = std::env::consts::ARCH;
        let graphics_args = if self.config.headless {
            // Headless mode: no graphics, serial console only
            vec!["--graphics", "none"]
        } else {
            match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                if arch == "aarch64" {
                    // ARM64: Use virtio video with SPICE
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
            }
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

        // Validate installation actually succeeded
        println!("🔍 Validating installation...");

        // Check 1: Disk should have grown significantly (at least 1GB)
        if let Ok(metadata) = fs::metadata(disk_path) {
            let disk_size_mb = metadata.len() / (1024 * 1024);
            println!("   Disk size: {} MB", disk_size_mb);

            if disk_size_mb < 500 {
                eprintln!("❌ Installation failed: Disk size is only {} MB (expected at least 500 MB)", disk_size_mb);
                eprintln!("   This usually means the installer ran out of memory or disk space.");
                eprintln!("   Try increasing RAM with --memory 4096 (4GB minimum for network install)");
                return Err("Installation validation failed: disk too small".into());
            }
        } else {
            eprintln!("⚠️  Warning: Could not check disk size");
        }

        // Check 2: VM should auto-reboot after installation, check if it's running
        println!("   Checking VM status...");
        let status_check = Command::new("virsh")
            .args(&["-c", "qemu:///system", "domstate", &self.config.name])
            .output()?;

        let vm_state = String::from_utf8_lossy(&status_check.stdout).trim().to_string();

        if vm_state != "running" {
            // VM not running, try to start it
            println!("   VM not running (state: {}), attempting to start...", vm_state);
            let boot_test = Command::new("virsh")
                .args(&["-c", "qemu:///system", "start", &self.config.name])
                .output()?;

            if !boot_test.status.success() {
                eprintln!("❌ Installation failed: VM will not start");
                eprintln!("   Error: {}", String::from_utf8_lossy(&boot_test.stderr));
                return Err("Installation validation failed: VM won't boot".into());
            }

            // Give it a moment to boot
            thread::sleep(Duration::from_secs(5));
        } else {
            println!("   ✓ VM is already running");
        }

        // Stop the VM (user in libvirt group doesn't need sudo)
        Command::new("virsh")
            .args(&["-c", "qemu:///system", "destroy", &self.config.name])
            .output()?;

        println!("✅ Installation completed and validated!");

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
            .args(&["-c", "qemu:///system", "start", &self.config.name])
            .status()?;

        // Wait for VM to boot
        thread::sleep(Duration::from_secs(5));

        // Hot-attach PCI devices if in hot-plug mode
        if self.config.pci_hotplug && !self.config.pci_devices.is_empty() {
            self.attach_pci_devices_hotplug()?;
        }

        // Handle display based on headless mode
        if self.config.headless {
            println!("🖥️  Headless VM started - use serial console to connect");
            println!("   Connect with: virsh console {}", self.config.name);
            return Ok(());
        }

        // Launch SPICE viewer for immediate functionality
        match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu | GraphicsBackend::QxlSpice => {
                println!("🖥️  Launching SPICE viewer...");
                let vm_name = self.config.name.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(5)); // Wait for VM to start SPICE
                    
                    // Get the actual SPICE port from virsh
                    if let Ok(output) = std::process::Command::new("virsh")
                        .args(&["-c", "qemu:///system", "domdisplay", &vm_name])
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

        // Hot-detach PCI devices if in hot-plug mode (before shutdown)
        if self.config.pci_hotplug && !self.config.pci_devices.is_empty() {
            // Wait a bit to ensure VM is responsive
            thread::sleep(Duration::from_secs(2));
            self.detach_pci_devices_hotplug()?;
        }

        Command::new("virsh")
            .args(&["-c", "qemu:///system", "shutdown", &self.config.name])
            .status()?;

        Ok(())
    }

    pub fn destroy_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗑️  Destroying VM: {}", self.config.name);

        // Check if VM exists first
        let list_output = Command::new("virsh")
            .args(&["-c", "qemu:///system", "list", "--all"])
            .output()?;

        if !String::from_utf8_lossy(&list_output.stdout).contains(&self.config.name) {
            println!("   VM {} not found", self.config.name);
            // Still try to clean up disk
        } else {
            // Force stop if running
            println!("   Force stopping VM...");
            let destroy_output = Command::new("virsh")
                .args(&["-c", "qemu:///system", "destroy", &self.config.name])
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
                .args(&["-c", "qemu:///system", "undefine", &self.config.name, "--remove-all-storage", "--nvram"])
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
                            .args(&["-c", "qemu:///system", "undefine", &self.config.name])
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
            .args(&["-c", "qemu:///system", "list", "--all"])
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
chmod 755 /home/user/.local"#);

            result
        } else {
            "".to_string()
        }
    }

    // ===== PCI Passthrough Methods =====

    fn validate_pci_passthrough(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔍 Validating PCI passthrough setup...");

        // 1. Check IOMMU enabled
        let dmesg = Command::new("dmesg").output()?;
        let dmesg_str = String::from_utf8_lossy(&dmesg.stdout);

        if !dmesg_str.contains("IOMMU") && !dmesg_str.contains("DMAR") {
            eprintln!("❌ IOMMU not enabled!");
            eprintln!("   Enable VT-d (Intel) or AMD-Vi (AMD) in BIOS");
            eprintln!("   Add to kernel cmdline: intel_iommu=on (Intel) or amd_iommu=on (AMD)");
            return Err("IOMMU not enabled".into());
        }
        println!("   ✓ IOMMU enabled");

        // 2. Check vfio-pci module available
        let modprobe = Command::new("modprobe")
            .arg("vfio-pci")
            .status();

        if modprobe.is_err() {
            eprintln!("❌ vfio-pci module not available");
            return Err("vfio-pci module not available".into());
        }
        println!("   ✓ vfio-pci module available");

        // 3. Validate each device and check IOMMU groups
        for device in &self.config.pci_devices {
            if let Some(group) = device.iommu_group {
                let group_devices = self.get_iommu_group_devices(group)?;
                if group_devices.len() > 1 {
                    println!("   ⚠️  Warning: IOMMU group {} contains {} devices:", group, group_devices.len());
                    for dev in &group_devices {
                        println!("       {}", dev);
                    }
                    println!("       All devices in the group will be isolated from the host.");
                }
            }
        }

        Ok(())
    }

    fn get_iommu_group_devices(&self, group: u32) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let group_path = format!("/sys/kernel/iommu_groups/{}/devices", group);
        let mut devices = Vec::new();

        if let Ok(entries) = fs::read_dir(&group_path) {
            for entry in entries.flatten() {
                if let Some(device_name) = entry.file_name().to_str() {
                    // Get device description
                    let lspci = Command::new("lspci")
                        .args(&["-s", device_name])
                        .output();

                    if let Ok(output) = lspci {
                        let desc = String::from_utf8_lossy(&output.stdout);
                        devices.push(desc.trim().to_string());
                    } else {
                        devices.push(device_name.to_string());
                    }
                }
            }
        }

        Ok(devices)
    }

    fn setup_pci_passthrough_permanent(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔌 Setting up permanent PCI passthrough...");

        for device in &self.config.pci_devices {
            println!("   Adding {} to VM XML", device.address);

            // Generate XML for PCI device
            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!("/tmp/{}-pci-{}.xml",
                                  self.config.name,
                                  device.address.replace(":", "-"));
            fs::write(&xml_path, xml)?;

            // Attach device to VM configuration (offline)
            let result = Command::new("virsh")
                .args(&["-c", "qemu:///system", "attach-device", &self.config.name, &xml_path, "--config"])
                .status();

            fs::remove_file(xml_path)?;

            if result.is_err() || !result?.success() {
                eprintln!("   ⚠️  Warning: Failed to attach {} to VM XML", device.address);
            } else {
                println!("   ✓ {} attached to VM (permanent)", device.address);
            }
        }

        Ok(())
    }

    fn generate_pci_device_xml(&self, device: &PciDevice) -> Result<String, Box<dyn std::error::Error>> {
        // Parse address: 0000:01:00.0 -> domain:0000, bus:01, slot:00, function:0
        let parts: Vec<&str> = device.address.split(&[':', '.']).collect();

        if parts.len() != 4 {
            return Err(format!("Invalid PCI address format: {}", device.address).into());
        }

        let xml = format!(r#"<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
  </source>
</hostdev>"#, parts[0], parts[1], parts[2], parts[3]);

        Ok(xml)
    }

    // Hot-plug methods

    fn attach_pci_devices_hotplug(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔌 Hot-attaching PCI devices...");

        for device in &self.config.pci_devices {
            println!("   Attaching {} ({})", device.address, device.description);

            // 1. Unbind from current driver
            if device.original_driver.is_some() {
                self.unbind_device(&device.address)?;
                thread::sleep(Duration::from_millis(500));
            }

            // 2. Bind to vfio-pci
            self.bind_to_vfio(device)?;
            thread::sleep(Duration::from_millis(500));

            // 3. Generate device XML
            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!("/tmp/{}-pci-{}.xml",
                                  self.config.name,
                                  device.address.replace(":", "-"));
            fs::write(&xml_path, xml)?;

            // 4. Hot-attach to running VM
            let result = Command::new("virsh")
                .args(&["-c", "qemu:///system", "attach-device", &self.config.name, &xml_path, "--live"])
                .status();

            fs::remove_file(xml_path)?;

            if result.is_err() || !result?.success() {
                eprintln!("   ⚠️  Failed to attach {}", device.address);
            } else {
                println!("   ✓ {} attached successfully", device.address);
            }
        }

        Ok(())
    }

    fn detach_pci_devices_hotplug(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔌 Hot-detaching PCI devices...");

        for device in &self.config.pci_devices {
            println!("   Detaching {} ({})", device.address, device.description);

            // 1. Generate XML for detach
            let xml = self.generate_pci_device_xml(device)?;
            let xml_path = format!("/tmp/{}-pci-{}.xml",
                                  self.config.name,
                                  device.address.replace(":", "-"));
            fs::write(&xml_path, xml)?;

            // 2. Detach from VM
            let result = Command::new("virsh")
                .args(&["-c", "qemu:///system", "detach-device", &self.config.name, &xml_path, "--live"])
                .status();

            fs::remove_file(xml_path)?;

            if result.is_ok() && result?.success() {
                println!("   ✓ {} detached from VM", device.address);
            }

            thread::sleep(Duration::from_millis(500));

            // 3. Unbind from vfio-pci
            self.unbind_device(&device.address)?;
            thread::sleep(Duration::from_millis(500));

            // 4. Rebind to original driver (if known)
            if let Some(ref driver) = device.original_driver {
                println!("   Restoring driver: {}", driver);
                self.rebind_to_driver(&device.address, driver)?;
                println!("   ✓ {} restored to {}", device.address, driver);
            }
        }

        Ok(())
    }

    fn unbind_device(&self, address: &str) -> Result<(), Box<dyn std::error::Error>> {
        let unbind_path = format!("/sys/bus/pci/devices/{}/driver/unbind", address);

        if Path::new(&unbind_path).exists() {
            let result = Command::new("sudo")
                .args(&["bash", "-c", &format!("echo '{}' > {}", address, unbind_path)])
                .status();

            if result.is_err() || !result?.success() {
                // Device may already be unbound, not a fatal error
                return Ok(());
            }
        }

        Ok(())
    }

    fn bind_to_vfio(&self, device: &PciDevice) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure vfio-pci module loaded
        Command::new("sudo").args(&["modprobe", "vfio-pci"]).status()?;

        // Bind device to vfio-pci using new_id
        let new_id = format!("{} {}", device.vendor_id, device.device_id);
        let new_id_path = "/sys/bus/pci/drivers/vfio-pci/new_id";

        let result = Command::new("sudo")
            .args(&["bash", "-c", &format!("echo '{}' > {}", new_id, new_id_path)])
            .status();

        if result.is_err() || !result?.success() {
            // May already be bound, try manual bind
            let bind_path = "/sys/bus/pci/drivers/vfio-pci/bind";
            Command::new("sudo")
                .args(&["bash", "-c", &format!("echo '{}' > {}", device.address, bind_path)])
                .status()?;
        }

        Ok(())
    }

    fn rebind_to_driver(&self, address: &str, driver: &str) -> Result<(), Box<dyn std::error::Error>> {
        let bind_path = format!("/sys/bus/pci/drivers/{}/bind", driver);

        if Path::new(&bind_path).exists() {
            Command::new("sudo")
                .args(&["bash", "-c", &format!("echo '{}' > {}", address, bind_path)])
                .status()?;
        }

        Ok(())
    }

    fn get_ssh_public_key(&self) -> Result<String, Box<dyn std::error::Error>> {
        let home_dir = std::env::var("HOME")?;
        let ssh_dir = Path::new(&home_dir).join(".ssh");

        // Try common SSH key types in order of preference
        let key_files = vec![
            ssh_dir.join("id_ed25519.pub"),
            ssh_dir.join("id_rsa.pub"),
            ssh_dir.join("id_ecdsa.pub"),
        ];

        // Check for existing keys
        for key_file in &key_files {
            if key_file.exists() {
                let public_key = fs::read_to_string(key_file)?
                    .trim()
                    .to_string();
                println!("   Using existing SSH key: {}", key_file.display());
                return Ok(public_key);
            }
        }

        // No keys found, generate a new ed25519 key
        println!("   No SSH key found, generating new ed25519 key...");
        fs::create_dir_all(&ssh_dir)?;

        let key_path = ssh_dir.join("id_ed25519");
        let output = Command::new("ssh-keygen")
            .args(&[
                "-t", "ed25519",
                "-f", key_path.to_str().unwrap(),
                "-N", "",  // No passphrase
                "-C", &format!("vm-provisioner@{}", hostname::get()?.to_string_lossy()),
            ])
            .output()?;

        if !output.status.success() {
            return Err(format!(
                "Failed to generate SSH key: {}",
                String::from_utf8_lossy(&output.stderr)
            ).into());
        }

        // Read the newly generated public key
        let pub_key_path = ssh_dir.join("id_ed25519.pub");
        let public_key = fs::read_to_string(pub_key_path)?
            .trim()
            .to_string();

        println!("   ✅ Generated new SSH key: {}", key_path.display());
        Ok(public_key)
    }
}
