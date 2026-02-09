# v1.5 Plan: Sunshine/Moonlight Per-App Streaming

## Goal

Replace the removed Xpra display forwarding with Sunshine/Moonlight streaming.
Each VM runs Sunshine as a streaming server. Each VM's installed applications
appear as individual app entries in Moonlight on the host. Launching an app
streams only that application fullscreen — no desktop view.

## Architecture

```
Host (Moonlight client)                 VM (Sunshine server)
┌──────────────────────┐               ┌──────────────────────────────┐
│  Moonlight           │               │  NixOS Guest                 │
│  ├── browser-vm      │  ◄── stream   │  ├── Sunshine (streaming)    │
│  │   └── LibreWolf   │──────────────►│  ├── Openbox (WM)            │
│  ├── steam-vm        │               │  ├── PipeWire (audio)        │
│  │   ├── Steam       │               │  ├── Venus virtio-gpu (GPU)  │
│  │   └── Desktop     │               │  └── Apps (flatpak/system)   │
│  └── dev-vm          │               └──────────────────────────────┘
│      └── VS Code     │
└──────────────────────┘
```

Each VM is a separate "host" in Moonlight. Under each host, the user sees
only the apps configured for that VM.

## Design Decisions

### Display capture: X11 + Openbox (not Wayland)

Sunshine's X11 capture (via KMS or NvFBC fallback to X11 shm) is the most
mature path. The VM already installs Openbox as the window manager. We
configure Openbox to auto-fullscreen launched applications so the user
sees only the app, not a desktop with panels/wallpaper.

Openbox `rc.xml` rules:
- All windows start maximized + undecorated
- No desktop icons, no panel, no right-click menu

### Video encoding: software (x264) with VAAPI probe

Venus forwards Vulkan but VA-API availability through virtio-gpu is
uncertain. The plan:

1. **Default**: Software encoding via x264 (works everywhere)
2. **Probe at config gen time**: Check if VA-API works through virtio-gpu
   by looking for `/dev/dri/renderD*` + vainfo in the guest
3. **Future**: When Mesa adds Vulkan Video encode support through Venus,
   switch to that

Software encoding at 1080p60 requires ~2-4 vCPUs. We should recommend
`--vcpus 4` minimum for streaming VMs and note this in the docs.

### Port allocation: per-VM port range

Sunshine uses ports 47984-47990 (TCP+UDP) and 48010 (TCP) by default.
To support multiple VMs streaming simultaneously, each VM gets a port
offset:

```
VM 0: 47984-47990, 48010  (default)
VM 1: 48084-48090, 48110  (+100)
VM 2: 48184-48190, 48210  (+200)
...
```

The port offset is stored in the VM config and passed to Sunshine's config.
For NAT-mode VMs, the host uses `virsh` to add port-forwarding rules.
For bridged VMs, the ports are directly accessible on the VM's LAN IP.

### Pairing: PIN-based via CLI/TUI

Moonlight requires a one-time PIN pairing with each Sunshine instance.
We provide:

1. `vm-provisioner pair <vm>` CLI command — starts the VM if needed,
   displays connection info, prompts user to enter the PIN shown in
   Moonlight
2. TUI "Pair" action on the VM detail screen
3. Sunshine's web UI at `https://<vm-ip>:47990` as fallback

### App entry generation

Sunshine uses an `apps.json` file listing available applications. We
auto-generate this from the VM config:

**Flatpak apps**: Each flatpak ID becomes an entry.
- Name: extracted from flatpak metadata (or derived from ID)
- Command: `flatpak run <app-id>`
- Example: `io.gitlab.librewolf-community` → name "LibreWolf",
  cmd `flatpak run io.gitlab.librewolf-community`

**System packages**: Harder to map to launchable apps. We handle known
GUI packages (firefox, gimp, inkscape, etc.) and skip CLI-only packages.
Users can add custom entries via `--stream-app "Name:command"`.

**Desktop entry**: Always include a "Desktop" fallback entry that just
shows the Openbox desktop (for manual app launching).

## Implementation Steps

### Step 1: Config struct changes (`src/config.rs`)

Add fields to `AppVMConfig`:

```rust
// Streaming (Sunshine/Moonlight)
#[serde(default)]
pub enable_streaming: bool,        // Enable Sunshine streaming server
#[serde(default)]
pub sunshine_port_offset: u16,     // Port offset for multi-VM (0, 100, 200, ...)
#[serde(default)]
pub streaming_apps: Vec<StreamingApp>,  // Custom app entries for Sunshine
```

Add new struct:

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamingApp {
    pub name: String,     // Display name in Moonlight
    pub command: String,  // Shell command to launch
}
```

Update `AppVMConfigBuilder` with:
- `.enable_streaming(bool)`
- `.add_streaming_app(name, command)`
- `.sunshine_port_offset(u16)`

Update `AppVMConfig::new()` to auto-enable streaming for non-headless VMs.

### Step 2: Sunshine NixOS config generation (`src/nixos/sunshine.rs`)

New module that generates the Sunshine portion of `configuration.nix`:

1. **Sunshine package**: Add to `environment.systemPackages`
2. **Sunshine systemd service**: Auto-start Sunshine as user "user"
3. **Sunshine config file**: Generate `/etc/sunshine/sunshine.conf` with:
   - Port settings (based on `sunshine_port_offset`)
   - Encoder settings (prefer VAAPI, fallback to x264)
   - Audio capture (PipeWire)
   - Input settings (mouse, keyboard, gamepad passthrough)
4. **apps.json generation**: Map flatpak + system packages to app entries
5. **Firewall rules**: Open Sunshine ports in `networking.firewall`
6. **Display setup**: Configure X11 auto-start with Openbox

NixOS config additions:

```nix
# Sunshine streaming
services.sunshine = {
  enable = true;
  openFirewall = true;  # Opens required ports
};

# Auto-login to X11 session (required for Sunshine capture)
services.xserver = {
  enable = true;
  windowManager.openbox.enable = true;
  displayManager.autoLogin = {
    enable = true;
    user = "user";
  };
};
```

Note: Sunshine is available in nixpkgs as `sunshine`. The NixOS module
(`services.sunshine`) handles most of the configuration. We need to
verify it's available in nixos-24.11 or if we need nixos-unstable.

### Step 3: Openbox per-app fullscreen config

Generate Openbox `rc.xml` with rules to fullscreen all windows:

```xml
<applications>
  <application class="*">
    <maximized>yes</maximized>
    <decor>no</decor>
  </application>
</applications>
```

Also configure:
- No desktop right-click menu
- No keyboard shortcuts that conflict with Moonlight
- Focus-follows-mouse disabled (Moonlight handles focus)

Write this as a Nix derivation or inline file in the NixOS config.

### Step 4: App entry auto-generation (`src/nixos/sunshine.rs`)

Build the `apps.json` content from config:

```rust
fn generate_sunshine_apps(config: &AppVMConfig) -> Vec<SunshineAppEntry> {
    let mut apps = Vec::new();

    // Flatpak apps
    for pkg in &config.flatpak_packages {
        let name = derive_app_name(pkg);  // "io.gitlab.librewolf-community" → "LibreWolf"
        apps.push(SunshineAppEntry {
            name,
            cmd: format!("flatpak run {}", pkg),
            auto_detach: true,
        });
    }

    // Known GUI system packages
    for pkg in &config.system_packages {
        if let Some(entry) = map_system_package_to_app(pkg) {
            apps.push(entry);
        }
    }

    // Custom streaming apps from config
    for app in &config.streaming_apps {
        apps.push(SunshineAppEntry {
            name: app.name.clone(),
            cmd: app.command.clone(),
            auto_detach: true,
        });
    }

    // Always add a Desktop fallback
    apps.push(SunshineAppEntry {
        name: "Desktop".into(),
        cmd: String::new(),  // Empty = show desktop
        auto_detach: false,
    });

    apps
}
```

The `derive_app_name()` function extracts a human-readable name from
a flatpak ID by taking the last segment and title-casing it. For
well-known apps, use a lookup table.

### Step 5: CLI changes (`src/cli/mod.rs`, `src/cli/create.rs`)

Add flags to the `Create` command:

```rust
/// Enable Sunshine streaming server in the VM
#[arg(long)]
stream: bool,

/// Custom streaming app entry (format: "Name:command")
#[arg(long, action = clap::ArgAction::Append)]
stream_app: Vec<String>,
```

Add `pair` command:

```rust
/// Pair Moonlight with a VM's Sunshine instance
Pair {
    name: String,
},
```

Update `CreateOptions`, `build_config()`, and `display_config_summary()`
to handle streaming options.

Implement `pair_vm()`:
1. Start VM if not running
2. Wait for VM IP
3. Print connection instructions:
   ```
   Sunshine is running on <vm-ip>:47990
   1. Open Moonlight on your host
   2. Add host: <vm-ip> (or <host-ip> with port offset for NAT)
   3. Enter the PIN shown in Moonlight when prompted
   ```
4. Wait for user to confirm pairing is complete

### Step 6: TUI changes (`src/tui/app.rs`, `src/tui/ui.rs`)

**Create form**: Add field index 9 (or 10 if bridge shown):
- "Streaming" checkbox (default: on for GUI VMs)

**VM detail screen**: Show streaming status:
- Sunshine port / URL
- Connection instructions
- "Pair" action (key shortcut)

**Dashboard**: Show streaming icon/indicator for streaming-enabled VMs.

### Step 7: Port forwarding for NAT VMs

For NAT-mode VMs, Moonlight on the host can't directly reach the VM's
private IP (192.168.122.x) — unless the host is also the libvirt host
(which it is in our case, so this actually works via the virbr0 bridge).

However, for Moonlight auto-discovery (mDNS), the VM needs to be
reachable. Options:

1. **Direct connection**: User manually adds the VM's NAT IP
   (192.168.122.x) in Moonlight — works because the host can reach
   virbr0. Simplest approach.
2. **Host proxy**: Run a port-forward on the host mapping
   localhost:offset → vm-ip:sunshine-port. More complex but enables
   `localhost` connections.

**Recommendation**: Start with option 1 (direct NAT IP). The `pair`
command prints the IP. Bridged VMs work with their LAN IP.

### Step 8: Documentation updates

- Update README: Add "Streaming" section explaining Sunshine/Moonlight
- Update CHANGELOG: Document v1.5 changes
- Add troubleshooting: Sunshine not starting, pairing issues, encoding
  performance, firewall issues

## File Changes Summary

| File | Change |
|------|--------|
| `src/config.rs` | Add `enable_streaming`, `sunshine_port_offset`, `streaming_apps`, `StreamingApp` struct |
| `src/nixos/mod.rs` | Add `pub mod sunshine;` |
| `src/nixos/sunshine.rs` | **New**: Sunshine config generation, app entry mapping |
| `src/nixos/config_gen.rs` | Call sunshine config generation for streaming VMs |
| `src/cli/mod.rs` | Add `--stream`, `--stream-app` flags, `Pair` command |
| `src/cli/create.rs` | Handle streaming options in `CreateOptions`/`build_config` |
| `src/cli/vm_ops.rs` | Add `pair_vm()` implementation |
| `src/tui/app.rs` | Add streaming checkbox to `CreateForm` |
| `src/tui/ui.rs` | Render streaming fields, show streaming info in detail view |
| `src/constants.rs` | Add `SUNSHINE_BASE_PORT`, `SUNSHINE_PORT_RANGE` constants |
| `src/error.rs` | Add `StreamingError` variant if needed |
| `README.md` | Add Streaming section |
| `CHANGELOG.md` | v1.5 entry |

## Open Questions

1. **Sunshine in nixos-24.11**: Is the `sunshine` package and NixOS module
   available in the 24.11 channel, or do we need nixos-unstable? Need to
   verify. If not available, we can use a Nix overlay or fetchurl.

2. **VA-API through Venus**: Does VA-API video encoding work through
   virtio-gpu Venus? If yes, we get hardware-accelerated streaming
   encoding for free. If not, software encoding (x264) is the fallback.
   This affects the recommended vCPU count.

3. **Moonlight auto-discovery**: Sunshine broadcasts via mDNS. In NAT
   mode, this won't reach the host network. Users will need to manually
   add the VM IP in Moonlight. Is this acceptable, or should we add
   an Avahi/mDNS relay?

4. **Multi-monitor**: Should we support streaming to multiple virtual
   monitors? Sunshine supports it, but it adds complexity. Recommend
   starting with single-monitor fullscreen.

5. **Controller passthrough**: Moonlight forwards gamepad input to
   Sunshine. This works out of the box. Should we add any special
   handling (e.g., udev rules in the guest for controller support)?

## Testing Plan

1. **Smoke test**: Create a VM with `--stream --flatpak org.mozilla.firefox`,
   verify Sunshine starts, pair with Moonlight, launch Firefox
2. **Multi-VM**: Run two streaming VMs simultaneously with different port
   offsets, verify both are accessible in Moonlight
3. **Encoding**: Test software encoding performance at 1080p60, measure
   CPU usage, verify acceptable latency (<50ms)
4. **Audio**: Verify PipeWire audio streams correctly through Sunshine
5. **Gamepad**: Verify controller input works in a Steam VM
6. **Network modes**: Test with NAT, bridged, verify connectivity
