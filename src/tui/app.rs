use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use vm_provisioner::config::{AppVMConfig, GraphicsBackend};
use vm_provisioner::constants::{DEFAULT_DISK_SIZE_GB, DEFAULT_MEMORY_MB, DEFAULT_VCPUS, PASSWORD_FILE_NAME};
use vm_provisioner::provisioner::AppVMProvisioner;
use vm_provisioner::provisioner::installation::Installation;
use vm_provisioner::virsh;

use crate::cli::create;

pub enum Screen {
    Dashboard,
    Detail(usize),
    ConfirmDestroy(usize),
    Create,
    Provisioning,
}

#[derive(Clone, Copy, PartialEq)]
pub enum NetworkChoice {
    Nat,
    None,
    Bridge,
}

impl NetworkChoice {
    pub fn next(self) -> Self {
        match self {
            Self::Nat => Self::None,
            Self::None => Self::Bridge,
            Self::Bridge => Self::Nat,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Nat => Self::Bridge,
            Self::None => Self::Nat,
            Self::Bridge => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Nat => "NAT",
            Self::None => "None",
            Self::Bridge => "Bridge",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum GraphicsChoice {
    VirtioGpu,
    VncOnly,
}

impl GraphicsChoice {
    pub fn next(self) -> Self {
        match self {
            Self::VirtioGpu => Self::VncOnly,
            Self::VncOnly => Self::VirtioGpu,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::VirtioGpu => "VirtioGpu (Venus)",
            Self::VncOnly => "VNC Only",
        }
    }

    pub fn to_backend(self) -> GraphicsBackend {
        match self {
            Self::VirtioGpu => GraphicsBackend::VirtioGpu,
            Self::VncOnly => GraphicsBackend::VncOnly,
        }
    }
}

pub struct CreateForm {
    pub name: String,
    pub memory: String,
    pub vcpus: String,
    pub disk: String,
    pub system_packages: String,
    pub flatpak_packages: String,
    pub headless: bool,
    pub graphics: GraphicsChoice,
    pub network: NetworkChoice,
    pub bridge_name: String,
    pub focused_field: usize,
}

impl CreateForm {
    /// Field indices:
    /// 0=Name, 1=Memory, 2=vCPUs, 3=Disk, 4=SystemPkgs, 5=FlatpakPkgs,
    /// 6=Headless, 7=Graphics, 8=Network, 9=BridgeName (conditional)
    pub fn field_count(&self) -> usize {
        if self.network == NetworkChoice::Bridge {
            10
        } else {
            9
        }
    }
}

impl Default for CreateForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            memory: DEFAULT_MEMORY_MB.to_string(),
            vcpus: DEFAULT_VCPUS.to_string(),
            disk: DEFAULT_DISK_SIZE_GB.to_string(),
            system_packages: String::new(),
            flatpak_packages: String::new(),
            headless: false,
            graphics: GraphicsChoice::VirtioGpu,
            network: NetworkChoice::Nat,
            bridge_name: String::new(),
            focused_field: 0,
        }
    }
}

pub struct ProvisioningState {
    pub vm_name: String,
    pub done: Arc<AtomicBool>,
    pub error: Arc<Mutex<Option<String>>>,
    pub scroll_offset: u16,
}

pub struct VmEntry {
    pub name: String,
    pub state: String,
    pub config: Option<AppVMConfig>,
    pub ip: Option<String>,
}

pub struct App {
    pub running: bool,
    pub screen: Screen,
    pub vm_list: Vec<VmEntry>,
    pub selected: usize,
    pub status_message: Option<(String, Instant)>,
    pub last_refresh: Instant,
    pub create_form: CreateForm,
    pub provisioning: Option<ProvisioningState>,
    pub log_lines: Arc<Mutex<Vec<String>>>,
}

impl App {
    pub fn new(log_lines: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            running: true,
            screen: Screen::Dashboard,
            vm_list: Vec::new(),
            selected: 0,
            status_message: None,
            last_refresh: Instant::now(),
            create_form: CreateForm::default(),
            provisioning: None,
            log_lines,
        }
    }

    pub fn refresh_vm_list(&mut self) {
        let config_dir = match AppVMConfig::config_dir() {
            Ok(d) => d,
            Err(_) => return,
        };

        let dir = match std::fs::read_dir(&config_dir) {
            Ok(d) => d,
            Err(_) => return,
        };

        let mut entries = Vec::new();
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            if path.file_name().and_then(|f| f.to_str()) == Some(PASSWORD_FILE_NAME) {
                continue;
            }
            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let config = AppVMConfig::load(&name).ok();
            let state = virsh::get_vm_state(&name).unwrap_or_else(|| "unknown".to_string());
            let ip = if state == "running" {
                virsh::get_vm_ip(&name)
            } else {
                None
            };

            entries.push(VmEntry {
                name,
                state,
                config,
                ip,
            });
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        self.vm_list = entries;

        if self.selected >= self.vm_list.len() && !self.vm_list.is_empty() {
            self.selected = self.vm_list.len() - 1;
        }

        self.last_refresh = Instant::now();
    }

    pub fn select_next(&mut self) {
        if self.vm_list.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.vm_list.len();
    }

    pub fn select_prev(&mut self) {
        if self.vm_list.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.vm_list.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn start_selected(&mut self) {
        if let Some(vm) = self.vm_list.get(self.selected) {
            let name = vm.name.clone();
            match virsh::start(&name) {
                Ok(()) => self.set_status(format!("Started '{}'", name)),
                Err(e) => self.set_status(format!("Failed to start '{}': {}", name, e)),
            }
            self.refresh_vm_list();
        }
    }

    pub fn stop_selected(&mut self) {
        if let Some(vm) = self.vm_list.get(self.selected) {
            let name = vm.name.clone();
            match virsh::shutdown(&name) {
                Ok(()) => self.set_status(format!("Stopping '{}'", name)),
                Err(e) => self.set_status(format!("Failed to stop '{}': {}", name, e)),
            }
            self.refresh_vm_list();
        }
    }

    pub fn destroy_vm(&mut self, idx: usize) {
        if let Some(vm) = self.vm_list.get(idx) {
            let name = vm.name.clone();
            let _ = virsh::destroy_unchecked(&name);
            match virsh::undefine(&name, true) {
                Ok(()) => {
                    self.set_status(format!("Destroyed '{}'", name));
                }
                Err(e) => {
                    // VM may not be defined in libvirt (e.g. failed provisioning),
                    // still clean up the config file
                    self.set_status(format!("Removed '{}' (undefine: {})", name, e));
                }
            }
            // Always remove config file so the entry disappears
            if let Ok(path) = AppVMConfig::config_path(&name) {
                let _ = std::fs::remove_file(path);
            }
            self.refresh_vm_list();
        }
    }

    pub fn reset_create_form(&mut self) {
        self.create_form = CreateForm::default();
    }

    pub fn start_provisioning(&mut self) {
        let form = &self.create_form;

        // Validate name
        let name = form.name.trim().to_string();
        if name.is_empty() {
            self.set_status("Name cannot be empty".into());
            return;
        }
        if let Err(e) = AppVMConfig::validate_vm_name(&name) {
            self.set_status(format!("Invalid name: {}", e));
            return;
        }

        let memory: u64 = match form.memory.trim().parse() {
            Ok(v) if v >= 512 => v,
            Ok(_) => {
                self.set_status("Memory must be >= 512 MB".into());
                return;
            }
            Err(_) => {
                self.set_status("Invalid memory value".into());
                return;
            }
        };

        let vcpus: u32 = if form.vcpus.trim().eq_ignore_ascii_case("max") {
            std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(DEFAULT_VCPUS)
        } else {
            match form.vcpus.trim().parse() {
                Ok(v) if v >= 1 => v,
                Ok(_) => {
                    self.set_status("vCPUs must be >= 1".into());
                    return;
                }
                Err(_) => {
                    self.set_status("Invalid vCPUs value (number or MAX)".into());
                    return;
                }
            }
        };

        let disk: u64 = match form.disk.trim().parse() {
            Ok(v) if v >= 10 => v,
            Ok(_) => {
                self.set_status("Disk must be >= 10 GB".into());
                return;
            }
            Err(_) => {
                self.set_status("Invalid disk value".into());
                return;
            }
        };

        // Check if VM already exists
        if virsh::domain_exists(&name) {
            self.set_status(format!("VM '{}' already exists", name));
            return;
        }
        if let Ok(path) = AppVMConfig::config_path(&name) {
            if std::path::Path::new(&path).exists() {
                self.set_status(format!("Config for '{}' already exists", name));
                return;
            }
        }

        // Parse packages (comma or space separated, trimmed, empty strings filtered)
        let system_packages: Vec<String> = form
            .system_packages
            .split([',', ' '])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let flatpak_packages: Vec<String> = form
            .flatpak_packages
            .split([',', ' '])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let network_bridge = match form.network {
            NetworkChoice::Bridge => {
                let br = form.bridge_name.trim().to_string();
                if br.is_empty() {
                    self.set_status("Bridge name cannot be empty".into());
                    return;
                }
                Some(br)
            }
            _ => None,
        };
        let no_network = form.network == NetworkChoice::None;

        let graphics = form.graphics;

        let mut config = AppVMConfig::new(
            name.clone(),
            memory,
            vcpus,
            disk,
            system_packages,
            flatpak_packages,
            form.headless,
            Vec::new(),
            false,
            Vec::new(),
            network_bridge,
            false,
            no_network,
        );
        config.graphics_backend = graphics.to_backend();

        // Save config and passwords
        let config_file = match create::save_config_and_passwords(&config) {
            Ok(f) => f,
            Err(e) => {
                self.set_status(format!("Failed to save config: {}", e));
                return;
            }
        };

        // Clear log buffer
        if let Ok(mut buf) = self.log_lines.lock() {
            buf.clear();
        }

        let done = Arc::new(AtomicBool::new(false));
        let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let done_clone = Arc::clone(&done);
        let error_clone = Arc::clone(&error);

        let name_for_cleanup = name.clone();
        let config_file_for_cleanup = config_file.clone();
        std::thread::spawn(move || {
            let mut config = config;
            if let Err(e) = AppVMProvisioner::new(config.clone()).provision_vm() {
                if let Ok(mut err) = error_clone.lock() {
                    *err = Some(format!("{:?}", e));
                }
                // Clean up config file and libvirt domain on failure so the
                // user can retry without manual cleanup.
                let _ = std::fs::remove_file(&config_file_for_cleanup);
                let _ = virsh::destroy_unchecked(&name_for_cleanup);
                let _ = virsh::undefine(&name_for_cleanup, true);
                done_clone.store(true, Ordering::SeqCst);
                return;
            }
            if let Err(e) = create::handle_post_provisioning(&mut config, &config_file) {
                if let Ok(mut err) = error_clone.lock() {
                    *err = Some(format!("{:?}", e));
                }
            }
            done_clone.store(true, Ordering::SeqCst);
        });

        self.provisioning = Some(ProvisioningState {
            vm_name: name,
            done,
            error,
            scroll_offset: 0,
        });
        self.screen = Screen::Provisioning;
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }
}
