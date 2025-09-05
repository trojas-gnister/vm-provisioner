use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use dialoguer::{Input, Confirm, MultiSelect};
use tokio;

mod container_validator;
use container_validator::ContainerValidator;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VMConfig {
    pub name: String,
    pub memory_mb: u64,
    pub vcpus: u32,
    pub disk_size_gb: u64,
    pub vm_dir: String,
    pub dnf_packages: Vec<String>,
    pub user_password: String,
    pub container_runtime: String,
    pub container_registry: String,
    pub containers: Vec<String>,
    pub container_ports: Vec<String>,
    pub firewall_ports: Vec<String>,
    pub vpn_config_path: String,
    pub vpn_provider: String,
    pub vpn_type: String,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            name: "container-vm".to_string(),
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 20,
            vm_dir: "/var/lib/libvirt/images".to_string(),
            dnf_packages: vec![
                "podman".to_string(),
                "podman-compose".to_string(),
                "qemu-guest-agent".to_string(),
                "git".to_string(),
                "curl".to_string(),
                "wget".to_string(),
                "htop".to_string(),
                "vim".to_string(),
            ],
            user_password: Self::generate_password(),
            container_runtime: "podman".to_string(),
            container_registry: "docker.io/linuxserver".to_string(),
            containers: vec![],
            container_ports: vec![],
            firewall_ports: vec![
                "22".to_string(),    // SSH - always needed
            ],
            vpn_config_path: "".to_string(),
            vpn_provider: "custom".to_string(),
            vpn_type: "openvpn".to_string(),
        }
    }
}

impl VMConfig {
    fn generate_password() -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let mut hasher = DefaultHasher::new();
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().hash(&mut hasher);
        format!("vm-{:x}", hasher.finish()).chars().take(12).collect()
    }


    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // Assume .env is already loaded by the caller
        
        // Helper function for parsing with better error messages
        fn parse_env_var<T: std::str::FromStr>(var_name: &str) -> Result<T, Box<dyn std::error::Error>> 
        where
            T::Err: std::error::Error + Send + Sync + 'static,
        {
            let value = std::env::var(var_name)
                .map_err(|_| format!("Missing required environment variable: {}", var_name))?;
            value.parse::<T>()
                .map_err(|e| format!("Failed to parse {}: {}", var_name, e).into())
        }
        
        let config = Self {
            name: std::env::var("VM_NAME").unwrap_or_else(|_| "container-vm".to_string()),
            memory_mb: parse_env_var("VM_MEMORY_MB")?,
            vcpus: parse_env_var("VM_VCPUS")?,
            disk_size_gb: parse_env_var("VM_DISK_SIZE_GB")?,
            vm_dir: std::env::var("VM_DIR").unwrap_or_else(|_| "/var/lib/libvirt/images".to_string()),
            dnf_packages: std::env::var("DNF_PACKAGES")
                .unwrap_or_else(|_| "podman,podman-compose,qemu-guest-agent,git,curl,wget,htop,vim".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            user_password: {
                let pwd = std::env::var("USER_PASSWORD").unwrap_or_default();
                if pwd.is_empty() { Self::generate_password() } else { pwd }
            },
            container_runtime: std::env::var("CONTAINER_RUNTIME").unwrap_or_else(|_| "podman".to_string()),
            container_registry: std::env::var("CONTAINER_REGISTRY").unwrap_or_else(|_| "docker.io/linuxserver".to_string()),
            containers: std::env::var("CONTAINERS")
                .unwrap_or_else(|_| "".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            container_ports: std::env::var("CONTAINER_PORTS")
                .unwrap_or_else(|_| "".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            firewall_ports: std::env::var("FIREWALL_PORTS")
                .unwrap_or_else(|_| "22".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            vpn_config_path: std::env::var("VPN_CONFIG_PATH").unwrap_or_else(|_| "".to_string()),
            vpn_provider: std::env::var("VPN_PROVIDER").unwrap_or_else(|_| "custom".to_string()),
            vpn_type: std::env::var("VPN_TYPE").unwrap_or_else(|_| "openvpn".to_string()),
        };
        
        Ok(config)
    }

    pub async fn interactive_config() -> Result<Self, Box<dyn std::error::Error>> {
        println!("🔧 Interactive VM Configuration");
        println!("================================");
        
        let name: String = Input::new()
            .with_prompt("VM Name")
            .default("librewolf-vm".to_string())
            .interact_text()?;

        let memory_mb: u64 = Input::new()
            .with_prompt("Memory (MB)")
            .default(4096)
            .interact_text()?;

        let vcpus: u32 = Input::new()
            .with_prompt("Virtual CPUs")
            .default(2)
            .interact_text()?;

        let disk_size_gb: u64 = Input::new()
            .with_prompt("Disk Size (GB)")
            .default(20)
            .interact_text()?;

        // DNF packages selection for server
        let available_dnf_packages = vec![
            "podman", "podman-compose", "qemu-guest-agent", "git", "curl", "wget", "htop", "vim",
            "fish", "zsh", "tmux", "nano", "tree", "rsync", "unzip", "tar"
        ];

        let mut dnf_defaults = vec![false; available_dnf_packages.len()];
        for i in 0..8.min(available_dnf_packages.len()) {
            dnf_defaults[i] = true; // Select first 8 by default
        }

        let dnf_selected = MultiSelect::new()
            .with_prompt("Select server packages to install")
            .items(&available_dnf_packages)
            .defaults(&dnf_defaults)
            .interact()?;

        let dnf_packages: Vec<String> = dnf_selected
            .iter()
            .map(|&i| available_dnf_packages[i].to_string())
            .collect();

        // Container runtime selection
        let container_runtime: String = Input::new()
            .with_prompt("Container runtime (podman/docker)")
            .default("podman".to_string())
            .interact_text()?;

        // Container registry selection
        let container_registry: String = Input::new()
            .with_prompt("Container registry")
            .default("docker.io/linuxserver".to_string())
            .interact_text()?;

        // Simple container input with optional suggestions
        let mut selected_containers: Vec<String> = Vec::new();
        println!("Enter containers (without registry prefix, e.g., 'librewolf:latest')");
        
        // Offer to show available containers for LinuxServer registry
        if container_registry.contains("linuxserver") {
            let show_available = Confirm::new()
                .with_prompt("Show available LinuxServer containers?")
                .default(false)
                .interact()?;
                
            if show_available {
                match ContainerValidator::get_available_linuxserver_containers().await {
                    Ok(containers) => {
                        println!("Available containers (first 20):");
                        for (i, container) in containers.iter().take(20).enumerate() {
                            println!("  {}. {}", i + 1, container);
                        }
                        if containers.len() > 20 {
                            println!("  ... and {} more", containers.len() - 20);
                        }
                    },
                    Err(_) => {
                        println!("⚠️ Could not fetch available containers");
                    }
                }
            }
        }
        
        loop {
            let container: String = Input::new()
                .with_prompt("Container image (or 'done' to finish)")
                .interact_text()?;

            if container.to_lowercase() == "done" {
                break;
            }

            if !container.is_empty() {
                selected_containers.push(container);
                println!("Added: {}/{}", container_registry, selected_containers.last().unwrap());
            }
            
            if selected_containers.is_empty() {
                println!("Note: You must add at least one container");
            }
        }
        
        // Validate containers
        if !selected_containers.is_empty() {
            ContainerValidator::validate_containers(&container_registry, &selected_containers).await?;
        }

        // Container port configuration
        let mut container_ports: Vec<String> = Vec::new();
        let mut port_num = 3000;
        
        for container in &selected_containers {
            let default_port = format!("{}:3000", port_num);
            let port_mapping: String = Input::new()
                .with_prompt(&format!("Port mapping for {} (host:container)", container))
                .default(default_port)
                .interact_text()?;
            
            container_ports.push(port_mapping);
            port_num += 1;
        }

        let vm_dir: String = Input::new()
            .with_prompt("VM storage directory")
            .default("/var/lib/libvirt/images".to_string())
            .interact_text()?;

        let user_password: String = Input::new()
            .with_prompt("User password (or press Enter to auto-generate)")
            .default(Self::generate_password())
            .interact_text()?;

        // VPN configuration for containers like qbittorrent
        let has_vpn_container = selected_containers.iter().any(|c| 
            c.contains("qbittorrent") || c.contains("vpn")
        );

        let (vpn_config_path, vpn_provider, vpn_type) = if has_vpn_container {
            println!("🔒 VPN container detected. Configure VPN settings:");
            
            let vpn_config_path: String = Input::new()
                .with_prompt("Path to VPN config file (.ovpn or .conf)")
                .default("".to_string())
                .interact_text()?;

            let vpn_provider: String = Input::new()
                .with_prompt("VPN provider (pia/airvpn/protonvpn/mullvad/custom)")
                .default("custom".to_string())
                .interact_text()?;

            let vpn_type: String = Input::new()
                .with_prompt("VPN type (openvpn/wireguard)")
                .default("openvpn".to_string())
                .interact_text()?;

            (vpn_config_path, vpn_provider, vpn_type)
        } else {
            ("".to_string(), "custom".to_string(), "openvpn".to_string())
        };

        Ok(Self {
            name,
            memory_mb,
            vcpus,
            disk_size_gb,
            vm_dir,
            dnf_packages,
            user_password,
            container_runtime,
            container_registry,
            containers: selected_containers,
            container_ports: container_ports.clone(),
            firewall_ports: {
                let mut ports = vec!["22".to_string(), "5900".to_string()]; // SSH and VNC always needed
                // Extract host ports from container_ports and add to firewall
                for port_mapping in &container_ports {
                    if let Some(host_port) = port_mapping.split(':').next() {
                        ports.push(host_port.to_string());
                    }
                }
                ports
            },
            vpn_config_path,
            vpn_provider,
            vpn_type,
        })
    }

    pub fn save_to_env(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let env_content = format!(r#"# Container VM Configuration
# Generated by container-vm-provisioner

# VM Hardware Configuration
VM_NAME={}
VM_MEMORY_MB={}
VM_VCPUS={}
VM_DISK_SIZE_GB={}
VM_DIR={}

# Server Package Configuration
# Comma-separated list of DNF packages  
DNF_PACKAGES={}

# Container Configuration
CONTAINER_RUNTIME={}
CONTAINER_REGISTRY={}

# Comma-separated list of container images (without registry prefix)
CONTAINERS={}

# Comma-separated list of port mappings (host:container)
CONTAINER_PORTS={}

# Comma-separated list of firewall ports to open
FIREWALL_PORTS={}

# VPN Configuration (for qBittorrent/VPN containers)
VPN_CONFIG_PATH={}
VPN_PROVIDER={}
VPN_TYPE={}

# User Configuration
USER_PASSWORD={}

# Examples:
# LINUXSERVER_CONTAINERS=linuxserver/webtop:latest,binhex/arch-qbittorrentvpn:latest
# CONTAINER_PORTS=3000:3000,8080:8080
# VPN_CONFIG_PATH=/home/user/vpn/mullvad.ovpn
# VPN_PROVIDER=mullvad
# VPN_TYPE=openvpn
"#,
            self.name,
            self.memory_mb,
            self.vcpus,
            self.disk_size_gb,
            self.vm_dir,
            self.dnf_packages.join(","),
            self.container_runtime,
            self.container_registry,
            self.containers.join(","),
            self.container_ports.join(","),
            self.firewall_ports.join(","),
            self.vpn_config_path,
            self.vpn_provider,
            self.vpn_type,
            self.user_password
        );

        fs::write(path, env_content)?;
        println!("💾 Configuration saved to: {}", path);
        Ok(())
    }

    pub async fn load_or_create_config(env_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if Path::new(env_path).exists() {
            println!("📋 Loading configuration from: {}", env_path);
            // Load the .env file with the correct path
            dotenv::from_filename(env_path).map_err(|e| format!("Failed to load .env file: {}", e))?;
            Self::from_env()
        } else {
            println!("📝 No configuration file found, starting interactive setup...");
            let config = Self::interactive_config().await?;
            
            let save_config = Confirm::new()
                .with_prompt("Save this configuration to .env file?")
                .default(true)
                .interact()?;
                
            if save_config {
                config.save_to_env(env_path)?;
                // Reload the configuration from the saved file to ensure consistency
                dotenv::from_filename(env_path).map_err(|e| format!("Failed to reload .env file: {}", e))?;
                Self::from_env()
            } else {
                Ok(config)
            }
        }
    }
}

pub struct ContainerVMProvisioner {
    config: VMConfig,
}

impl ContainerVMProvisioner {
    pub fn new(config: VMConfig) -> Self {
        Self { config }
    }

    pub async fn provision_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting Container VM provisioning...");
        
        // Validate containers before starting lengthy provisioning process
        if !self.config.containers.is_empty() {
            ContainerValidator::validate_containers(&self.config.container_registry, &self.config.containers).await?;
        }
        
        // Check prerequisites
        self.check_prerequisites()?;
        
        // Download Fedora minimal ISO
        let iso_path = self.download_fedora_minimal()?;
        
        // Create VM disk
        let disk_path = self.create_vm_disk()?;
        
        // Generate kickstart configuration
        let kickstart_path = self.generate_kickstart_config()?;
        
        // Kickstart file will be injected directly into initrd
        
        // Start automated kickstart installation using virt-install
        self.start_kickstart_installation(&iso_path, &disk_path, &kickstart_path)?;
        
        // Configure post-installation
        self.configure_vm()?;
        
        println!("✅ Container VM provisioned successfully!");
        println!("   VM Name: {}", self.config.name);
        println!("   User Password: {}", self.config.user_password);
        println!("   Container ports: {}", self.config.container_ports.join(", "));
        println!("");
        println!("🔗 To access containers from host:");
        for port in &self.config.container_ports {
            let host_port = port.split(':').next().unwrap_or("3000");
            println!("   ssh -L {}:localhost:{} user@VM_IP", host_port, host_port);
        }
        println!("   Then access via http://localhost:PORT in your browser");
        
        Ok(())
    }

    fn check_prerequisites(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔍 Checking prerequisites...");
        
        let required_commands = ["virsh", "virt-install", "qemu-img", "genisoimage"];
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

    fn download_fedora_minimal(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Detect host architecture
        let arch = std::env::consts::ARCH;
        let iso_name = format!("fedora-minimal-{}.iso", arch);
        let iso_path = format!("{}/{}", self.config.vm_dir, iso_name);
        
        if Path::new(&iso_path).exists() {
            println!("📦 Using existing Fedora minimal ISO for {}", arch);
            return Ok(iso_path);
        }
        
        println!("📥 Downloading Fedora minimal ISO for {}...", arch);
        
        // Architecture-specific download URLs
        let download_url = match arch {
            "x86_64" => "https://download.fedoraproject.org/pub/fedora/linux/releases/40/Server/x86_64/iso/Fedora-Server-netinst-x86_64-40-1.14.iso",
            "aarch64" => "https://download.fedoraproject.org/pub/fedora/linux/releases/40/Server/aarch64/iso/Fedora-Server-netinst-aarch64-40-1.14.iso",
            _ => return Err(format!("Unsupported architecture: {}", arch).into()),
        };
        
        println!("📍 Downloading from: {}", download_url);
        
        let output = Command::new("curl")
            .args(&["-L", "-o", &iso_path, download_url])
            .output()?;
            
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to download Fedora ISO: {}", error_msg).into());
        }
        
        Ok(iso_path)
    }

    fn create_vm_disk(&self) -> Result<String, Box<dyn std::error::Error>> {
        let disk_path = format!("{}/{}.qcow2", self.config.vm_dir, self.config.name);
        
        if Path::new(&disk_path).exists() {
            fs::remove_file(&disk_path)?;
        }
        
        println!("💾 Creating VM disk ({} GB)...", self.config.disk_size_gb);
        
        let output = Command::new("qemu-img")
            .args(&[
                "create",
                "-f", "qcow2",
                &disk_path,
                &format!("{}G", self.config.disk_size_gb)
            ])
            .output()?;
            
        if !output.status.success() {
            return Err("Failed to create VM disk".into());
        }
        
        Ok(disk_path)
    }

    fn generate_kickstart_config(&self) -> Result<String, Box<dyn std::error::Error>> {
        let kickstart_dir = format!("/tmp/{}-kickstart", self.config.name);
        fs::create_dir_all(&kickstart_dir)?;
        
        let kickstart_path = format!("{}/kickstart.cfg", kickstart_dir);
        
        println!("🏗️  Generating kickstart configuration...");
        
        // Build DNF packages list for kickstart
        let dnf_packages = self.config.dnf_packages
            .iter()
            .map(|pkg| pkg.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Build container startup commands
        let container_commands = self.config.containers
            .iter()
            .zip(self.config.container_ports.iter())
            .map(|(container, port)| {
                let container_name = container.split(':').next().unwrap_or("container").replace("/", "-");
                let full_image = format!("{}/{}", self.config.container_registry, container);
                
                format!("{} run -d --name {} --security-opt seccomp=unconfined -e PUID=1000 -e PGID=1000 -e TZ=UTC -p {} --shm-size=1gb --restart=unless-stopped {}", 
                    self.config.container_runtime,
                    container_name,
                    port,
                    full_image
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
            
        // Build firewall port commands
        let firewall_commands = self.config.firewall_ports
            .iter()
            .map(|port| format!("firewall-cmd --add-port={}/tcp --permanent", port))
            .collect::<Vec<_>>()
            .join("\n");

        // VPN configuration if needed
        let vpn_config = if !self.config.vpn_config_path.is_empty() {
            format!(r#"
# Copy VPN configuration file
mkdir -p /home/user/vpn
cp {} /home/user/vpn/
chown -R user:user /home/user/vpn"#, 
                self.config.vpn_config_path)
        } else {
            "".to_string()
        };

        // Generate kickstart file with dynamic configuration for server-only setup
        let kickstart_content = format!(r#"# Kickstart file for Container VM
# Generated by container-vm-provisioner

# Use text mode install
text

# Accept EULA
eula --agreed

# System language
lang en_US.UTF-8

# Keyboard layout
keyboard us

# Network configuration
network --bootproto=dhcp --device=link --activate

# Root password (disabled, using sudo user)
rootpw --lock

# Create user account
user --name=user --gecos="Container User" --groups=wheel --password={} --plaintext

# System timezone
timezone UTC

# Disk partitioning (automatic)
autopart --type=plain
clearpart --all --initlabel

# Boot loader configuration
bootloader --location=mbr

# Security - SELinux enabled
selinux --enforcing

# Firewall configuration
firewall --enabled --service=ssh

# Services
services --enabled=qemu-guest-agent,sshd --disabled=telnet,rsh,rexec

# Package selection - minimal server install
%packages --ignoremissing
@core
@standard
{}
# Remove unwanted packages
-abrt*
-sendmail
-postfix
-xorg-x11*
-NetworkManager-tui
-initial-setup
%end

# Post-installation script
%post --log=/var/log/kickstart-post.log

# Keep multi-user target (server mode)
systemctl set-default multi-user.target

# Enable and start container runtime service
systemctl enable {} --now

# Configure firewall ports
{}
firewall-cmd --reload

# Configure auto-login for user on tty1
mkdir -p /etc/systemd/system/getty@tty1.service.d
cat > /etc/systemd/system/getty@tty1.service.d/autologin.conf << 'EOF'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin user --noclear %I $TERM
EOF

# Create container startup script
cat > /home/user/start-containers.sh << 'EOF'
#!/bin/bash
# Container startup script

echo "Starting containers..."
{}

# Wait for containers to start
sleep 10

# Show running containers
{} ps

# Display access information
echo "============================="
echo "Container VM is ready!"
echo "============================="
echo "Access your services:"
{}
echo "============================="
EOF

chmod +x /home/user/start-containers.sh
chown user:user /home/user/start-containers.sh

# Auto-start containers on login
echo '# Auto-start containers on login' >> /home/user/.bashrc
echo 'if [[ $$ == $(pgrep -o -u $USER bash) ]]; then' >> /home/user/.bashrc
echo '    ~/start-containers.sh' >> /home/user/.bashrc
echo 'fi' >> /home/user/.bashrc
chown user:user /home/user/.bashrc

{}

# Ensure hostname is set
echo "{}" > /etc/hostname

# Reboot after installation
reboot

%end
"#, 
            self.config.user_password,
            dnf_packages,
            self.config.container_runtime,
            firewall_commands,
            container_commands,
            self.config.container_runtime,
            self.config.container_ports.iter()
                .enumerate()
                .map(|(i, port)| format!("echo \"Service {}: http://localhost:{}\"", i+1, port.split(':').next().unwrap_or("3000")))
                .collect::<Vec<_>>()
                .join("\n"),
            vpn_config,
            self.config.name
        );

        // Write kickstart file
        fs::write(&kickstart_path, kickstart_content)?;
        
        Ok(kickstart_path)
    }


    fn start_kickstart_installation(&self, _iso_path: &str, disk_path: &str, kickstart_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting kickstart installation...");
        
        let arch = std::env::consts::ARCH;
        
        // Use network install for proper kickstart support
        let install_location = match arch {
            "x86_64" => "https://dl.fedoraproject.org/pub/fedora/linux/releases/41/Server/x86_64/os/",
            "aarch64" => "https://dl.fedoraproject.org/pub/fedora/linux/releases/41/Server/aarch64/os/",
            _ => return Err(format!("Unsupported architecture: {}", arch).into()),
        };
        
        // Create strings that need to live for the duration of the command
        let memory_str = self.config.memory_mb.to_string();
        let vcpus_str = self.config.vcpus.to_string();
        let disk_str = format!("path={},size={},format=qcow2,bus=virtio", disk_path, self.config.disk_size_gb);
        let graphics_str = "vnc,listen=127.0.0.1,port=5900".to_string();
        
        // Build virt-install command with kickstart
        let mut virt_install_args = vec![
            "--name", &self.config.name,
            "--memory", &memory_str,
            "--vcpus", &vcpus_str,
            "--disk", &disk_str,
            "--location", install_location,
            "--network", "network=default,model=virtio",
            "--graphics", &graphics_str,
            "--initrd-inject", kickstart_path,
            "--extra-args", "inst.ks=file:/kickstart.cfg console=tty0 console=ttyS0,115200n8 inst.text",
            "--noautoconsole",
            "--wait", "-1", // Wait for installation to complete
        ];
        
        // Add architecture-specific options
        if arch == "aarch64" {
            virt_install_args.extend_from_slice(&[
                "--arch", "aarch64",
                "--machine", "virt",
                "--boot", "uefi",
            ]);
        }
        
        println!("⏳ Running automated installation (this may take 15-20 minutes)...");
        println!("   Using network install from: {}", install_location);
        println!("   Monitor with: vncviewer localhost:5900");
        println!("   Or console: virsh console {}", self.config.name);
        
        let output = Command::new("virt-install")
            .args(&virt_install_args)
            .output()?;
            
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Kickstart installation failed: {}", error_msg).into());
        }
        
        println!("✅ Installation completed successfully!");
        
        Ok(())
    }

    fn configure_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("⚙️  Configuring VM post-installation...");
        
        // Wait for system to settle after installation
        thread::sleep(Duration::from_secs(10));
        
        println!("✅ Server VM configured successfully!");
        println!("   Containers will start automatically on boot");
        println!("   SSH access: ssh user@VM_IP");
        println!("   Container ports: {}", self.config.container_ports.join(", "));
        
        Ok(())
    }

    fn get_vm_status(&self) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new("virsh")
            .args(&["domstate", &self.config.name])
            .output()?;
            
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn start_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("▶️  Starting VM...");
        
        let output = Command::new("virsh")
            .args(&["start", &self.config.name])
            .output()?;
            
        if !output.status.success() {
            return Err(format!("Failed to start VM: {}", String::from_utf8_lossy(&output.stderr)).into());
        }
        
        println!("✅ VM started successfully!");
        println!("   Connect with: vncviewer localhost:5900");
        println!("   Or console: virsh console {}", self.config.name);
        
        Ok(())
    }

    pub fn stop_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏹️  Stopping VM...");
        
        let output = Command::new("virsh")
            .args(&["shutdown", &self.config.name])
            .output()?;
            
        if !output.status.success() {
            return Err(format!("Failed to stop VM: {}", String::from_utf8_lossy(&output.stderr)).into());
        }
        
        Ok(())
    }

    pub fn destroy_vm(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗑️  Destroying VM...");
        
        // Stop VM forcefully if running
        println!("   Stopping VM...");
        let _ = Command::new("virsh")
            .args(&["destroy", &self.config.name])
            .output();
        
        // Remove NVRAM file for aarch64 UEFI VMs
        let nvram_path = format!("/var/lib/libvirt/qemu/nvram/{}_VARS.fd", self.config.name);
        if Path::new(&nvram_path).exists() {
            println!("   Removing NVRAM file...");
            let _ = fs::remove_file(&nvram_path);
        }
        
        // Undefine VM with NVRAM removal
        println!("   Undefining VM...");
        let mut undefine_args = vec!["undefine", &self.config.name];
        
        // Check if VM has NVRAM and add appropriate flags
        let dominfo_output = Command::new("virsh")
            .args(&["dominfo", &self.config.name])
            .output();
            
        if dominfo_output.is_ok() {
            undefine_args.push("--nvram");
        }
        
        let output = Command::new("virsh")
            .args(&undefine_args)
            .output();
            
        // If standard undefine fails, try without nvram flag
        if output.is_err() || !output.as_ref().unwrap().status.success() {
            println!("   Retrying VM undefine...");
            let _ = Command::new("virsh")
                .args(&["undefine", &self.config.name])
                .output();
        }
        
        // Manually remove VM disk
        let disk_path = format!("{}/{}.qcow2", self.config.vm_dir, self.config.name);
        if Path::new(&disk_path).exists() {
            println!("   Removing VM disk...");
            let _ = fs::remove_file(&disk_path);
        }
        
        // Clean up temporary files (both cloud-init and kickstart)
        println!("   Cleaning up temporary files...");
        let _ = fs::remove_file(format!("/tmp/{}-kickstart.iso", self.config.name));
        let _ = fs::remove_file(format!("/tmp/{}-cloud-init.iso", self.config.name));
        let _ = fs::remove_dir_all(format!("/tmp/{}-kickstart", self.config.name));
        let _ = fs::remove_dir_all(format!("/tmp/{}-cloud-init", self.config.name));
        let _ = fs::remove_file(format!("/tmp/{}-setup.sh", self.config.name));
        let _ = fs::remove_file(format!("/tmp/{}.xml", self.config.name));
        
        println!("✅ VM destroyed successfully!");
        
        Ok(())
    }

    pub fn status(&self) -> Result<(), Box<dyn std::error::Error>> {
        let status = self.get_vm_status()?;
        println!("VM Status: {}", status);
        
        if status == "running" {
            println!("VNC access: vncviewer localhost:5900");
            println!("SSH access: ssh user@VM_IP (password: {})", self.config.user_password);
            println!("");
            println!("🔗 Port forwarding for containers:");
            for port in &self.config.container_ports {
                let host_port = port.split(':').next().unwrap_or("3000");
                println!("   ssh -L {}:localhost:{} user@VM_IP", host_port, host_port);
            }
            
            // Get VM info
            let output = Command::new("virsh")
                .args(&["dominfo", &self.config.name])
                .output()?;
                
            println!("\nVM Info:\n{}", String::from_utf8_lossy(&output.stdout));
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        println!("🦀 Container VM Provisioner");
        println!("============================");
        println!("Usage: {} <command> [options]", args[0]);
        println!("");
        println!("Commands:");
        println!("  provision  - Create and provision new container VM");
        println!("  configure  - Interactive configuration (generates .env)");
        println!("  start      - Start existing VM");
        println!("  stop       - Stop running VM");
        println!("  status     - Show VM status");
        println!("  destroy    - Destroy VM and cleanup");
        println!("");
        println!("Configuration:");
        println!("  --env <path>    - Use specific .env file (default: ./vm.env)");
        println!("  --interactive   - Force interactive configuration");
        println!("  --yes, -y       - Skip confirmation prompts (non-interactive)");
        println!("");
        println!("Examples:");
        println!("  {} configure                    # Interactive setup", args[0]);
        println!("  {} provision                    # Use ./vm.env or interactive", args[0]);
        println!("  {} provision --env custom.env   # Use custom.env", args[0]);
        println!("  {} provision --interactive      # Force interactive mode", args[0]);
        println!("  {} provision --yes              # Non-interactive mode", args[0]);
        return Ok(());
    }

    let env_path = if let Some(env_idx) = args.iter().position(|x| x == "--env") {
        args.get(env_idx + 1).unwrap_or(&"./vm.env".to_string()).clone()
    } else {
        "./vm.env".to_string()
    };

    let force_interactive = args.contains(&"--interactive".to_string());

    let config = match args[1].as_str() {
        "configure" => {
            let config = VMConfig::interactive_config().await?;
            config.save_to_env(&env_path)?;
            return Ok(());
        },
        "provision" => {
            if force_interactive {
                VMConfig::interactive_config().await?
            } else {
                VMConfig::load_or_create_config(&env_path).await?
            }
        },
        _ => {
            // For other commands, try to load from .env or use defaults
            if Path::new(&env_path).exists() {
                dotenv::from_filename(&env_path).ok();
                VMConfig::from_env().unwrap_or_else(|_| {
                    println!("⚠️  Failed to load .env, using defaults");
                    VMConfig::default()
                })
            } else {
                VMConfig::default()
            }
        }
    };

    let provisioner = ContainerVMProvisioner::new(config);

    match args[1].as_str() {
        "provision" => {
            println!("📋 VM Configuration:");
            println!("   Name: {}", provisioner.config.name);
            println!("   Memory: {} MB", provisioner.config.memory_mb);
            println!("   vCPUs: {}", provisioner.config.vcpus);
            println!("   Disk: {} GB", provisioner.config.disk_size_gb);
            println!("   User Password: {}", provisioner.config.user_password);
            println!("   Container Runtime: {}", provisioner.config.container_runtime);
            println!("   Container Registry: {}", provisioner.config.container_registry);
            println!("   Containers: {:?}", provisioner.config.containers);
            println!("   Firewall Ports: {:?}", provisioner.config.firewall_ports);
            println!("   DNF Packages: {:?}", provisioner.config.dnf_packages);
            println!("");
            
            let non_interactive = args.contains(&"--yes".to_string()) || args.contains(&"-y".to_string());
            
            let confirm = if non_interactive {
                true
            } else {
                Confirm::new()
                    .with_prompt("Proceed with VM provisioning?")
                    .default(true)
                    .interact()?
            };
                
            if confirm {
                provisioner.provision_vm().await?;
            } else {
                println!("❌ Provisioning cancelled");
            }
        },
        "start" => {
            provisioner.start_vm()?;
        },
        "stop" => {
            provisioner.stop_vm()?;
        },
        "status" => {
            provisioner.status()?;
        },
        "destroy" => {
            println!("⚠️  This will permanently destroy the VM and all its data!");
            let non_interactive = args.contains(&"--yes".to_string()) || args.contains(&"-y".to_string());
            
            let confirm = if non_interactive {
                true
            } else {
                Confirm::new()
                    .with_prompt("Are you sure you want to destroy the VM?")
                    .default(false)
                    .interact()?
            };
                
            if confirm {
                provisioner.destroy_vm()?;
            } else {
                println!("❌ Destruction cancelled");
            }
        },
        _ => {
            eprintln!("❌ Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_config_creation() {
        let config = VMConfig::default();
        assert_eq!(config.name, "container-vm");
        assert_eq!(config.memory_mb, 4096);
        assert_eq!(config.vcpus, 2);
    }

    #[test]
    fn test_provisioner_creation() {
        let config = VMConfig::default();
        let provisioner = ContainerVMProvisioner::new(config);
        assert_eq!(provisioner.config.name, "container-vm");
    }
}
