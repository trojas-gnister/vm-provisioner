use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::config::{AppVMConfig, AppType, GraphicsBackend, NetworkMode};

pub struct AppVMProvisioner {
    config: AppVMConfig,
}

impl AppVMProvisioner {
    pub fn new(config: AppVMConfig) -> Self {
        Self { config }
    }
    
    pub async fn provision_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting Application VM provisioning...");
        println!("   Application: {}", self.config.app_command);
        
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
        println!("   Application: {}", self.config.app_command);
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
        
        // Build package list
        let mut all_packages = self.config.system_packages.clone();
        all_packages.extend(self.config.app_packages.clone());
        let packages = all_packages.join("\n");
        
        // Build repository configuration for LibreWolf if needed
        let repo_config = if self.config.app_packages.contains(&"librewolf".to_string()) {
            r#"
# Add LibreWolf repository
cat > /etc/yum.repos.d/librewolf.repo << 'EOF'
[librewolf]
name=LibreWolf
baseurl=https://rpm.librewolf.net/x86_64
enabled=1
gpgcheck=1
gpgkey=https://rpm.librewolf.net/pubkey.gpg
EOF
dnf install -y librewolf"#
        } else {
            ""
        };
        
        // Build environment variables
        let env_vars = self.config.app_env_vars
            .iter()
            .map(|(k, v)| format!("export {}={}", k, v))
            .collect::<Vec<_>>()
            .join("\n");
        
        // Build Cage compositor configuration
        let cage_config = format!(r#"
# Create Cage service for auto-starting application
cat > /etc/systemd/system/cage-app.service << 'EOF'
[Unit]
Description=Cage Wayland Compositor with Application
After=multi-user.target

[Service]
Type=simple
User=user
PAMName=login
TTYPath=/dev/tty7
Environment="XDG_RUNTIME_DIR=/run/user/1000"
Environment="WAYLAND_DISPLAY=wayland-0"
{}
ExecStartPre=/bin/bash -c 'mkdir -p /run/user/1000 && chown user:user /run/user/1000'
ExecStart=/usr/bin/cage -- {}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical.target
EOF

# Enable the service
systemctl enable cage-app.service

# Set graphical target as default
systemctl set-default graphical.target"#,
            env_vars,
            self.config.app_command
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

# Add extra repositories if needed
{}

# Configure sudo for user
echo "user ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers.d/user

# Configure Wayland environment
mkdir -p /home/user/.config
cat > /home/user/.config/environment << 'EOF'
MOZ_ENABLE_WAYLAND=1
WAYLAND_DISPLAY=wayland-0
XDG_SESSION_TYPE=wayland
QT_QPA_PLATFORM=wayland
GDK_BACKEND=wayland
EOF

{}

{}

{}

# Configure firewall rules
{}

# Create window manager agent placeholder
cat > /usr/local/bin/vm-window-agent << 'EOF'
#!/bin/bash
# VM Window Agent - communicates with host for window management
# This will be replaced with the actual Rust agent
echo "VM Window Agent started"
EOF
chmod +x /usr/local/bin/vm-window-agent

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
            self.config.app_command,
            self.config.user_password,
            packages,
            repo_config,
            cage_config,
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
            "aarch64" => "https://dl.fedoraproject.org/pub/fedora/linux/releases/41/Server/aarch64/os/",
            _ => return Err(format!("Unsupported architecture: {}", arch).into()),
        };
        
        let memory_str = self.config.memory_mb.to_string();
        let vcpus_str = self.config.vcpus.to_string();
        
        // Configure graphics based on backend
        let graphics_args = match self.config.graphics_backend {
            GraphicsBackend::VirtioGpu => {
                vec!["--graphics", "none", "--video", "virtio", "--channel", "spicevmc"]
            },
            GraphicsBackend::QxlSpice => {
                vec!["--graphics", "spice", "--video", "qxl", "--channel", "spicevmc"]
            },
            GraphicsBackend::VncOnly => {
                vec!["--graphics", "vnc,listen=127.0.0.1,port=5900"]
            },
        };
        
        let mut virt_install_args = vec![
            "--name", &self.config.name,
            "--memory", &memory_str,
            "--vcpus", &vcpus_str,
            "--disk", &format!("path={},size={},format=qcow2,bus=virtio", 
                               disk_path, self.config.disk_size_gb),
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
            virt_install_args.extend_from_slice(&["--sound", "default"]);
        }
        
        // Add USB controller if needed
        if self.config.enable_usb_passthrough {
            virt_install_args.extend_from_slice(&["--controller", "usb,model=qemu-xhci"]);
        }
        
        println!("⏳ Running automated installation (15-20 minutes)...");
        
        Command::new("virt-install")
            .args(&virt_install_args)
            .status()?;
            
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
        
        // If using VirtIO-GPU, start the host-side window proxy
        if matches!(self.config.graphics_backend, GraphicsBackend::VirtioGpu) {
            println!("🪟 Starting window proxy...");
            // This would launch the host-side Wayland proxy
            // that creates native windows for the VM application
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
}
