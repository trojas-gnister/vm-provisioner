//! CLI interface for vm-provisioner
//!
//! This module handles command-line argument parsing and dispatches
//! to the appropriate command handlers.

pub mod create;
pub mod shortcuts;
pub mod usb;
pub mod vm_ops;

use clap::{Parser, Subcommand};

use vm_provisioner::config::AppVMConfig;
use vm_provisioner::display_bridge::DisplayBridge;
use vm_provisioner::error::Result;
use vm_provisioner::virsh;
use vm_provisioner::xpra_manager::XpraManager;

#[derive(Parser)]
#[command(name = "vm-provisioner")]
#[command(about = "Lightweight VM isolation system with seamless windowing", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Create {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long, action = clap::ArgAction::Append)]
        system: Vec<String>,
        #[arg(long, action = clap::ArgAction::Append)]
        flatpak: Vec<String>,
        #[arg(short = 'y', long)]
        yes: bool,
        #[arg(short, long)]
        config: Option<String>,
        #[arg(long, default_value = "2048")]
        memory: u64,
        #[arg(long, default_value = "2")]
        vcpus: u32,
        #[arg(long, default_value = "20")]
        disk: u64,
        #[arg(long)]
        headless: bool,
        #[arg(long, action = clap::ArgAction::Append)]
        pci: Vec<String>,
        #[arg(long)]
        pci_hotplug: bool,
        /// Enable web-based streaming on this port (Selkies-GStreamer WebRTC)
        #[arg(long)]
        web_port: Option<u16>,
        /// USB device passthrough (format: "vendor:product" e.g. "046d:c52b")
        #[arg(long, action = clap::ArgAction::Append)]
        usb: Vec<String>,
        /// Hot-attach USB devices only while VM runs (restore to host on stop)
        #[arg(long)]
        usb_hotplug: bool,
        /// Share host folder with VM (format: "/host/path:/guest/mount/path")
        #[arg(long, action = clap::ArgAction::Append)]
        share: Vec<String>,
        /// Mount shared folders as read-only
        #[arg(long)]
        share_readonly: bool,
        /// Use bridged networking (VM gets LAN IP). Requires bridge interface to exist.
        #[arg(long)]
        network_bridge: Option<String>,
        /// Grant flatpak apps access to all devices (webcams, audio, etc.)
        #[arg(long)]
        grant_device_access: bool,
        /// Disable networking entirely (uses vsock for display forwarding)
        #[arg(long)]
        no_network: bool,
    },
    Start {
        name: String,
    },
    Stop {
        name: String,
    },
    List,
    Passwords,
    Destroy {
        name: String,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    Console {
        name: String,
    },
    GenerateShortcuts {
        name: String,
    },
    Launch {
        name: String,
        app: String,
    },
    Apps {
        name: String,
    },
    /// Attach a USB device to a running VM
    UsbAttach {
        /// VM name
        name: String,
        /// USB device in vendor:product format (e.g., "046d:c52b")
        device: String,
    },
    /// Detach a USB device from a running VM
    UsbDetach {
        /// VM name
        name: String,
        /// USB device in vendor:product format (e.g., "046d:c52b")
        device: String,
    },
}

/// Initialize the logger with custom formatting
pub fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            use std::io::Write;
            let level_icon = match record.level() {
                log::Level::Error => "❌",
                log::Level::Warn => "⚠️ ",
                log::Level::Info => "ℹ️ ",
                log::Level::Debug => "🔍",
                log::Level::Trace => "📋",
            };
            writeln!(buf, "{} {}", level_icon, record.args())
        })
        .init();
}

/// Run the CLI application
pub fn run() -> Result<()> {
    init_logger();

    let cli = Cli::parse();
    match cli.command {
        Commands::Create {
            name,
            system,
            flatpak,
            yes,
            config,
            memory,
            vcpus,
            disk,
            headless,
            pci,
            pci_hotplug,
            web_port,
            usb,
            usb_hotplug,
            share,
            share_readonly,
            network_bridge,
            grant_device_access,
            no_network,
        } => {
            create::create_vm(create::CreateOptions {
                name,
                system_packages: system,
                flatpak_packages: flatpak,
                skip_confirm: yes,
                config_path: config,
                memory,
                vcpus,
                disk,
                headless,
                pci_addresses: pci,
                pci_hotplug,
                web_port,
                usb_addresses: usb,
                usb_hotplug,
                share_paths: share,
                share_readonly,
                network_bridge,
                grant_device_access,
                no_network,
            })?;
        }
        Commands::Start { name } => vm_ops::start_vm(name)?,
        Commands::Stop { name } => vm_ops::stop_vm(name)?,
        Commands::List => vm_ops::list_vms()?,
        Commands::Passwords => vm_ops::show_passwords()?,
        Commands::Destroy { name, yes } => vm_ops::destroy_vm(name, yes)?,
        Commands::Console { name } => vm_ops::connect_console(name)?,
        Commands::GenerateShortcuts { name } => shortcuts::generate_shortcuts(name)?,
        Commands::Launch { name, app } => shortcuts::launch_app(name, app)?,
        Commands::Apps { name } => shortcuts::list_apps(name)?,
        Commands::UsbAttach { name, device } => usb::usb_attach(name, device)?,
        Commands::UsbDetach { name, device } => usb::usb_detach(name, device)?,
    }
    Ok(())
}

/// Get the display bridge for a given VM config
pub fn get_display_bridge(config: &AppVMConfig) -> Result<Box<dyn DisplayBridge>> {
    // Xpra is the only supported display protocol
    Ok(Box::new(XpraManager::new(config)?))
}

/// Get the current VM status
pub fn get_vm_status(name: &str) -> String {
    virsh::get_vm_state(name).unwrap_or_else(|| "not created".to_string())
}
