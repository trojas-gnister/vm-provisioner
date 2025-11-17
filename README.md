# VM Provisioner – Qubes-like Application Isolation

Rust CLI that provisions Fedora-based VMs per application, wires in seamless window forwarding, and hides the guest desktop behind either Waypipe (Wayland) or Xpra (X11). This repository replaces the previous mix of shell scripts and docs with one Rust 2021 codebase built around libvirt/KVM and Tokio async workflows.

## Snapshot

- **Isolation model:** each app or workload runs inside its own VM with on-demand package installation (dnf + Flatpak).
- **Display stacks:** `waypipe` + Weston headless compositor by default; `xpra` + transient Xvfb when `--display-protocol xpra` is selected; headless mode bypasses both bridges.
- **Host support:** Fedora, Debian/Ubuntu, and other libvirt-enabled distros on x86_64 or aarch64. Requires `libvirtd`, `qemu-kvm`, `virt-install`, and `virt-viewer`.
- **Guest base:** Fedora 41 Server via virt-install network installs and kickstart templates stored under `/tmp/<vm>-kickstart`.
- **CLI:** `vm-provisioner create/start/stop/destroy/generate-shortcuts/launch/apps/passwords/console/list`.
- **Config + secrets:** TOML configs and password store live in `~/.config/vm-provisioner/`.

## Capabilities

### What already works

- Automated provisioning flow with prerequisite checks, ISO download/cache, kickstart generation, virt-install invocation, and post-install scripts (Flatpak setup, SSH keys, Weston or Xpra enablement, PipeWire helper, firewall rules).
- Dynamic package selection through `--system` (dnf) and `--flatpak` (Flathub). Defaults vary per display protocol and headless mode.
- Display abstraction via `DisplayBridge` trait with concrete Waypipe and Xpra managers (`src/waypipe_manager.rs`, `src/xpra_manager.rs`).
- Per-app `.desktop` generation under `~/.local/share/applications/vm-provisioner/` plus `vm-provisioner launch <vm> "<command>"`.
- VM lifecycle helpers (`create`, `start`, `stop`, `destroy`, `list`, `console`, `passwords`, `generate-shortcuts`, `launch`, `apps`).
- Smart memory trim: installers boot with 4 GB, then `virsh setmem/setmaxmem` shrinks the VM to the requested runtime memory (2 GB default).
- PCI passthrough and optional hot-plug support with IOMMU warnings and driver rebinding helpers.
- Password vault stored at `~/.config/vm-provisioner/vm-passwords.toml` with per-VM configs alongside it.
- Unit tests in `tests/display_bridge_tests.rs` plus manual guides/scripts (`tests/e2e_testing_guide.rs`, `test-xpra.sh`).

### Still in progress

- Xpra manual/QA coverage (VPN/audio resilience, host capability probes, doc polish).
- Waypipe enhancements like automated host capability detection, multi-monitor/HiDPI docs, and smarter PulseAudio socket discovery.
- Documentation pass for protocol-specific troubleshooting once more validation runs conclude.

## Display Protocols

| Protocol | Default | Guest stack | Host requirement | Recommended use | Known status |
|----------|---------|-------------|------------------|-----------------|--------------|
| Waypipe  | ✅ yes   | `weston`, `waypipe`, `wl-clipboard`, `pipewire`, `kitty`, `openssh-server` | `waypipe` CLI, host PulseAudio/PipeWire socket | Wayland-native apps, modern desktops, clipboard + audio via SSH reverse tunneling | Production-ready |
| Xpra     | Opt-in  | `xpra`, `xorg-x11-server-Xvfb`, `pulseaudio-libs`, `kitty`, `openssh-server` | `xpra` CLI on host | Legacy/X11 apps, hosts lacking Wayland pipelines, VPN-resilient sessions | Code complete; QA/audio under test |
| Headless | `--headless` | Minimal CLI stack (`git` by default) | None beyond libvirt/kvm | Server/CLI workloads; access via `virsh console` | Stable |

### Waypipe path

1. Reuses existing host SSH keys (`WaypipeManager::get_ssh_public_key()`), generating `~/.ssh/id_ed25519` if missing.
2. Kickstart auto-logins on tty1 to launch headless Weston (`weston --backend=headless-backend.so --width=1920 --height=1080`) and primes a PipeWire helper before apps start.
3. Host PulseAudio socket detection looks at `$WAYPIPE_PULSE_SOCKET`, `$XDG_RUNTIME_DIR/pulse/native`, then `/run/user/<uid>/pulse/native`.
4. Desktop entries call `waypipe --compress zstd ssh -R <guest-pulse>:<host-pulse> user@<vm-ip> <command>`. Audio and clipboard use Wayland protocols tunneled over SSH.
5. `vm-provisioner launch <vm> "<command>"` reuses the same exec string for ad-hoc commands.

### Xpra path

1. Validates the `xpra` binary at bridge initialization. Recommend enabling “Allow LAN/local network” in your VPN so the libvirt subnet (192.168.122.0/24) stays reachable.
2. Kickstart keeps the guest in multi-user target, injects host SSH keys, enables Unix-socket forwarding (`StreamLocalBindUnlink yes`), disables PipeWire auto-start, and exports `PULSE_SERVER=unix:/run/user/1000/pulse/native`.
3. Host `.desktop` entries run `xpra start ssh://user@<vm-ip>/ --ssh="<ssh options>" --start-child="<command>" --exit-with-children`. The `--ssh` string binds to the libvirt source IP and adds `-R` PulseAudio socket tunneling.
4. Sessions launch on demand and exit once the child process ends—no persistent xpra state to clean up.
5. `test-xpra.sh` offers an automated smoke test (create VM → shortcuts → xpra launch/audio → cleanup).

### Installing the protocol tooling

```bash
# Shared prerequisites
sudo dnf install libvirt qemu-kvm virt-install virt-viewer   # Fedora
sudo apt install libvirt-daemon qemu-kvm virtinst virt-viewer # Debian/Ubuntu
sudo systemctl enable --now libvirtd

# Waypipe (default protocol)
sudo dnf install waypipe        # Fedora/RHEL
sudo apt install waypipe        # Debian/Ubuntu
sudo pacman -S waypipe          # Arch

# Xpra (optional)
sudo dnf install xpra           # Fedora/RHEL
sudo apt install xpra           # Debian/Ubuntu
sudo pacman -S xpra             # Arch
```

## Getting Started

```bash
git clone <repo-url>
cd vm-provisioner
cargo build --release
./target/release/vm-provisioner --help
```

### Creating VMs

```bash
# Default Waypipe VM with a Flatpak browser
vm-provisioner create --flatpak io.gitlab.librewolf-community

# Waypipe VM with mixed packages
vm-provisioner create \
  --system git gcc rust cargo nodejs npm \
  --flatpak com.visualstudio.code \
  --memory 8192 --disk 40 --name dev-vm

# Headless CLI VM
vm-provisioner create --system python3 python3-pip git --headless --name cli-vm

# Opt-in Xpra VM for X11 workloads
vm-provisioner create --display-protocol xpra --system firefox --name legacy-browser
```

### Operating VMs

- `vm-provisioner start <name>` – boots the VM and launches the SPICE viewer (blank screen for Waypipe/Xpra VMs; use shortcuts to run apps).
- `vm-provisioner generate-shortcuts <name>` – after the VM boots, waits 5 s and writes `.desktop` files to `~/.local/share/applications/vm-provisioner/`.
- `vm-provisioner launch <name> "<command>"` – runs ad-hoc commands via the selected display bridge.
- `vm-provisioner apps <name>` – lists system packages (minus helper deps) plus Flatpaks to help build menus.
- `vm-provisioner stop <name>` / `destroy <name> -y` – graceful shutdown and cleanup (including `.desktop` files).
- `vm-provisioner passwords` – prints stored credentials. Console logins use `user/<generated password>`; graphical sessions auto-login.

## VM Modes & Advanced Features

### Headless mode

`--headless` provisions a minimal Fedora guest (no GUI packages, no display bridge) and presents only a serial console. Great for server workloads, automation tasks, or development stacks you want to reach via `virsh console`.

### PCI passthrough

Use `--pci <BDF>` (repeatable) to attach devices and `--pci-hotplug` to only take ownership while the VM runs.

```bash
vm-provisioner create \
  --flatpak org.mozilla.firefox \
  --pci 0000:01:00.0 --pci 0000:01:00.1 \
  --pci-hotplug \
  --name gpu-browser
```

Enable IOMMU in firmware (`intel_iommu=on` or `amd_iommu=on`), check groupings with `lspci -nn -D` and `find /sys/kernel/iommu_groups`, and expect warnings if multiple devices must move together.

### Memory management

Installers always boot with 4 GB RAM; after provisioning, the tool powers the VM off, applies `virsh setmem/setmaxmem`, and restarts so runtime memory matches `--memory` (default 2048 MB). No manual action required.

### Desktop integration

Generated `.desktop` entries contain the full launch string (Waypipe or Xpra) including SSH options, reverse PulseAudio tunnels, and `--exit-with-children`. Remove them with `vm-provisioner destroy <name>` or `vm-provisioner generate-shortcuts <name>` after pruning packages.

## Configuration & Secrets

- Per-VM configs: `~/.config/vm-provisioner/<vm>.toml`
- Password store: `~/.config/vm-provisioner/vm-passwords.toml`

Typical Waypipe VM config:

```toml
name = "browser-vm"
memory_mb = 4096
vcpus = 2
disk_size_gb = 20
vm_dir = "/var/lib/libvirt/images"
system_packages = ["weston", "waypipe", "wl-clipboard", "pipewire", "kitty", "git", "openssh-server", "flatpak"]
flatpak_packages = ["org.mozilla.firefox"]
display_protocol = "Waypipe"
graphics_backend = "VirtioGpu"
enable_clipboard = true
enable_audio = true
network_mode = "Nat"
```

For headless VMs the defaults collapse to just `["git"]` and Flatpaks are ignored. Xpra configs swap in `["xpra", "xorg-x11-server-Xvfb", "pulseaudio-libs", "git", "openssh-server"]` and keep the system in multi-user target.

## Troubleshooting & Testing

- `cargo test` – runs unit tests (config defaults, display bridge wiring).
- `cargo test --test e2e_testing_guide -- --ignored` – documentation-style E2E checklist once infrastructure is available.
- `./test-xpra.sh` – optional helper to exercise the Xpra stack (creation, shortcuts, xpra launch, audio, cleanup).

Common fixes:

- **Missing virtualization tools:** Install `virsh`, `virt-install`, `qemu-img`, ensure `libvirtd` is running.
- **Waypipe issues:** Install the host binary, ensure `ssh-keyscan` captured the VM host key, and verify the `.desktop` `Exec` line keeps the `-R /run/user/1000/pulse/native:/run/user/<host_uid>/pulse/native` tunnel.
- **Xpra drops on VPN enable:** Allow LAN/local network traffic in your VPN client or add a manual route so 192.168.122.0/24 stays reachable; xpra’s SSH transport binds to the libvirt source IP and cannot traverse VPNs that block that subnet.
- **`generate-shortcuts` fails:** VM must be running; rerun after `vm-provisioner start <name>` and give it a few extra seconds on slower disks.
- **No audio:** Confirm PipeWire/PulseAudio is running on the host (`pactl info`). The guest disables its own audio stack and expects the reverse SSH socket.
- **Clipboard/resolution problems in SPICE viewer:** Enable “Auto resize VM with window” in virt-viewer; remember the headless Weston session intentionally stays blank.

## Development

```bash
cargo fmt
cargo clippy --all-targets
cargo test
```

New display protocols plug into `DisplayBridge` and expose methods for guest packages, kickstart fragments, shortcut generation, and launch strings. Shared helpers will eventually move into a `vm_utils` module; for now, follow the patterns in `waypipe_manager.rs` and `xpra_manager.rs`.

Contributions should keep documentation in sync with the current Rust implementation so new contributors can rely on this README as the canonical workflow guide.
