//! CLI interface for vm-provisioner
//!
//! This module handles command-line argument parsing and dispatches
//! to the appropriate command handlers.

pub mod create;
pub mod usb;
pub mod vm_ops;

use clap::{Parser, Subcommand};

use vm_provisioner::error::Result;
use vm_provisioner::virsh;

#[derive(Parser)]
#[command(name = "vm-provisioner")]
#[command(about = "Lightweight VM isolation system for running applications in sandboxed VMs", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
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
        /// Disable networking entirely (headless CLI-only, access via virsh console)
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
        None => return crate::tui::run(),
        Some(Commands::Create {
            name,
            system,
            flatpak,
            yes,
            config,
            memory,
            vcpus,
            disk,
            headless,
            usb,
            usb_hotplug,
            share,
            share_readonly,
            network_bridge,
            grant_device_access,
            no_network,
        }) => {
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
                usb_addresses: usb,
                usb_hotplug,
                share_paths: share,
                share_readonly,
                network_bridge,
                grant_device_access,
                no_network,
            })?;
        }
        Some(Commands::Start { name }) => vm_ops::start_vm(name)?,
        Some(Commands::Stop { name }) => vm_ops::stop_vm(name)?,
        Some(Commands::List) => vm_ops::list_vms()?,
        Some(Commands::Passwords) => vm_ops::show_passwords()?,
        Some(Commands::Destroy { name, yes }) => vm_ops::destroy_vm(name, yes)?,
        Some(Commands::Console { name }) => vm_ops::connect_console(name)?,
        Some(Commands::UsbAttach { name, device }) => usb::usb_attach(name, device)?,
        Some(Commands::UsbDetach { name, device }) => usb::usb_detach(name, device)?,
    }
    Ok(())
}

/// Get the current VM status
pub fn get_vm_status(name: &str) -> String {
    virsh::get_vm_state(name).unwrap_or_else(|| "not created".to_string())
}
