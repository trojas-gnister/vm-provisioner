//! XPRA/Waypipe End-to-End Testing Guide
//!
//! This ignored test suite doubles as a living checklist for manual validation.
//! Run individual cases with `cargo test <name> -- --ignored` once hardware is
//! ready (libvirt with KVM, Fedora guest media present, etc.). None of the tests
//! execute destructive actions automatically; they only print the steps so human
//! operators can follow along.

use std::process::Command;

/// Minimal configuration shared across the guide
#[allow(dead_code)]
struct E2ETestConfig {
    vm_name: String,
    memory_mb: u64,
    display_protocol: &'static str,
}

impl Default for E2ETestConfig {
    fn default() -> Self {
        Self {
            vm_name: "xpra-e2e-test".into(),
            memory_mb: 4096,
            display_protocol: "xpra",
        }
    }
}

fn run_command(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_prerequisites() -> Result<(), Vec<&'static str>> {
    let mut missing = Vec::new();

    if !run_command("virsh", &["--version"]) {
        missing.push("virsh (libvirt)");
    }
    if !run_command("xpra", &["--version"]) {
        missing.push("xpra client binaries");
    }
    if !run_command("vm-provisioner", &["--version"])
        && !run_command("./target/release/vm-provisioner", &["--version"])
    {
        missing.push("vm-provisioner binary");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[test]
#[ignore]
fn test_01_prerequisites() {
    println!("\n=== TEST 01: Host prerequisites ===\n");
    match check_prerequisites() {
        Ok(_) => {
            println!("✅ virsh, xpra, and vm-provisioner detected");
            println!("   Tip: if Mullvad VPN is enabled, run `mullvad lan set allow on` once so 192.168.122.0/24 remains reachable.");
        }
        Err(missing) => {
            println!("❌ Missing components:");
            for item in missing {
                println!("   - {}", item);
            }
            panic!("Install the items above before continuing");
        }
    }
}

#[test]
#[ignore]
fn test_02_xpra_vm_creation() {
    println!("\n=== TEST 02: Create XPRA VM ===\n");
    let cfg = E2ETestConfig::default();
    println!("1. Create VM\n   vm-provisioner create --display-protocol {proto} --system firefox --memory {mem} --name {name}\n", proto = cfg.display_protocol, mem = cfg.memory_mb, name = cfg.vm_name);
    println!("   Expectation:\n     - Kickstart completes without errors\n     - ~/.config/vm-provisioner/{name}.toml exists and lists display_protocol = \"Xpra\"\n     - virsh list --all shows {name}\n", name = cfg.vm_name);
    println!("2. Verify SSH host key acceptance via ~/.ssh/known_hosts");
    println!("3. Confirm VM stops automatically after provisioning (state = \"shut off\")");
}

#[test]
#[ignore]
fn test_03_xpra_launch_and_audio() {
    println!("\n=== TEST 03: XPRA launch + audio over SSH socket ===\n");
    let cfg = E2ETestConfig::default();
    println!(
        "Prereq: start VM -> vm-provisioner start {name}\n",
        name = cfg.vm_name
    );
    println!(
        "1. Generate host shortcuts: vm-provisioner generate-shortcuts {name}\n",
        name = cfg.vm_name
    );
    println!("2. Launch Firefox from CLI: vm-provisioner launch {} firefox\n", cfg.vm_name);
    println!("   - xpra should spawn a window attached to the running VM\n   - Mullvad/WireGuard toggles must **not** close the window (SSH binds to the libvirt source IP)\n");
    println!("3. Audio check:\n   - In the VM window, play audio (e.g., youtube.com test clip)\n   - Host PulseAudio meters should show activity because the guest uses the SSH-forwarded /run/user/<host>/pulse/native socket\n");
    println!("4. VPN flip: enable/disable Mullvad and verify the xpra session + audio stay up while firefox still resolves outbound via the VPN");
}

// NOTE: Waypipe has been deprecated. Only Xpra is supported.
// The waypipe regression test has been removed.

#[test]
#[ignore]
fn test_05_cleanup() {
    println!("\n=== TEST 05: Cleanup ===\n");
    let cfg = E2ETestConfig::default();
    println!("1. vm-provisioner destroy {name} -y\n2. Remove ~/.config/vm-provisioner/{name}.toml if not automatically deleted\n3. Optionally prune cached ISOs/qcow2 files from /var/lib/libvirt/images\n", name = cfg.vm_name);
}
