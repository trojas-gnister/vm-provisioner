//! VM Provisioner - Lightweight VM isolation with seamless windowing
//!
//! This is the main entry point for the CLI application.
//! The CLI uses the vm_provisioner library crate for all functionality.

mod cli;
mod tui;

fn main() -> vm_provisioner::Result<()> {
    cli::run()
}
