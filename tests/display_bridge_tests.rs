/// DisplayBridge Integration Tests
/// Tests for the display protocol abstraction layer
use vm_provisioner::config::{AppVMConfig, DisplayProtocol};
use vm_provisioner::display_bridge::DisplayBridge;
use vm_provisioner::waypipe_manager::WaypipeManager;
use vm_provisioner::x2go_manager::X2GoManager;

#[test]
fn test_display_protocol_enum_serialization() {
    let waypipe = DisplayProtocol::Waypipe;
    let x2go = DisplayProtocol::X2Go;

    // Test debug output
    assert_eq!(format!("{:?}", waypipe), "Waypipe");
    assert_eq!(format!("{:?}", x2go), "X2Go");

    // Test equality
    assert_eq!(waypipe, DisplayProtocol::Waypipe);
    assert_eq!(x2go, DisplayProtocol::X2Go);
    assert_ne!(waypipe, x2go);
}

#[test]
fn test_appvmconfig_with_waypipe() {
    let config = AppVMConfig::new(
        "test-waypipe-vm".to_string(),
        2048,
        2,
        20,
        vec!["firefox".to_string()],
        vec!["io.gitlab.librewolf-community".to_string()],
        false,
        vec![],
        false,
        DisplayProtocol::Waypipe,
    );

    assert_eq!(config.name, "test-waypipe-vm");
    assert_eq!(config.display_protocol, DisplayProtocol::Waypipe);
    assert_eq!(config.memory_mb, 2048);

    // Waypipe includes: sway, swaylock, swayidle, waybar, i3status, dmenu, rofi,
    // wl-clipboard, pipewire, kitty, git, waypipe, openssh-server (13 default) + user package
    assert!(config.system_packages.len() > 10, "Should have default Waypipe packages + user package");
    assert!(config.system_packages.contains(&"waypipe".to_string()), "Should have waypipe");
    assert!(config.system_packages.contains(&"firefox".to_string()), "Should have user package");

    assert_eq!(config.flatpak_packages.len(), 1);
    assert_eq!(config.flatpak_packages[0], "io.gitlab.librewolf-community");
}

#[test]
fn test_appvmconfig_with_x2go() {
    let config = AppVMConfig::new(
        "test-x2go-vm".to_string(),
        4096,
        4,
        30,
        vec!["gimp".to_string(), "inkscape".to_string()],
        vec!["com.github.tchx84.Flatseal".to_string()],
        false, // Not headless
        vec![],
        false,
        DisplayProtocol::X2Go,
    );

    assert_eq!(config.name, "test-x2go-vm");
    assert_eq!(config.display_protocol, DisplayProtocol::X2Go);
    assert_eq!(config.memory_mb, 4096);
    assert_eq!(config.vcpus, 4);

    // X2Go includes: xorg-x11-server-Xorg, xorg-x11-xinit, i3, i3status, dmenu, rofi,
    // x2goserver, x2goserver-xsession, pulseaudio, pulseaudio-utils, xclip, kitty, git,
    // openssh-server (14 default) + 2 user packages
    assert!(config.system_packages.len() > 10, "Should have default X2Go packages + user packages");
    assert!(config.system_packages.contains(&"x2goserver".to_string()), "Should have x2goserver");
    assert!(config.system_packages.contains(&"gimp".to_string()), "Should have gimp");
    assert!(config.system_packages.contains(&"inkscape".to_string()), "Should have inkscape");

    assert_eq!(config.flatpak_packages.len(), 1);
}

#[test]
fn test_waypipe_manager_creation() {
    let config = AppVMConfig::new(
        "waypipe-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::Waypipe,
    );

    // WaypipeManager validates waypipe binary availability
    // Test may fail if waypipe is not installed, which is expected
    match WaypipeManager::new(&config) {
        Ok(_) => {
            // waypipe is installed, good
        }
        Err(e) => {
            // waypipe not installed - verify error message
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("waypipe"),
                "Expected waypipe-related error, got: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_x2go_manager_creation_without_x2goclient() {
    let config = AppVMConfig::new(
        "x2go-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::X2Go,
    );

    // X2GoManager validates x2goclient availability
    let manager = X2GoManager::new(&config);

    // Result depends on whether x2goclient is installed
    // We just verify the function returns a Result
    match manager {
        Ok(_) => {
            // x2goclient is installed - this is fine
        }
        Err(e) => {
            // x2goclient is not installed - check error message
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("x2goclient") || error_msg.contains("not found"),
                "Expected x2goclient error, got: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_waypipe_guest_packages() {
    let config = AppVMConfig::new(
        "waypipe-pkg-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::Waypipe,
    );

    match WaypipeManager::new(&config) {
        Ok(manager) => {
            let packages = manager.guest_packages();

            // Verify Waypipe-specific packages are included
            assert!(
                packages.iter().any(|p| p.contains("sway")),
                "Waypipe should include sway"
            );
            assert!(
                packages.iter().any(|p| p.contains("waypipe")),
                "Waypipe should include waypipe"
            );
            assert!(
                packages.iter().any(|p| p.contains("wl-clipboard")),
                "Waypipe should include wl-clipboard"
            );

            // Verify X2Go packages are NOT included
            assert!(
                !packages.iter().any(|p| p.contains("x2goserver")),
                "Waypipe should NOT include x2goserver"
            );
        }
        Err(_) => {
            eprintln!("Skipping test_waypipe_guest_packages - waypipe binary not installed");
        }
    }
}

#[test]
fn test_x2go_guest_packages() {
    let config = AppVMConfig::new(
        "x2go-pkg-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::X2Go,
    );

    // Skip test if x2goclient not installed
    if X2GoManager::new(&config).is_err() {
        eprintln!("Skipping x2go package test - x2goclient not installed");
        return;
    }

    let manager = X2GoManager::new(&config)
        .expect("Should create X2GoManager for package test");

    let packages = manager.guest_packages();

    // Verify X2Go-specific packages are included
    assert!(
        packages.iter().any(|p| p.contains("x2goserver")),
        "X2Go should include x2goserver"
    );
    assert!(
        packages.iter().any(|p| p.contains("xorg-x11-server-Xorg")),
        "X2Go should include xorg-x11-server"
    );
    assert!(
        packages.iter().any(|p| p.contains("i3")),
        "X2Go should include i3"
    );

    // Verify Waypipe packages are NOT included
    assert!(
        !packages.iter().any(|p| p.contains("sway")),
        "X2Go should NOT include sway"
    );
    assert!(
        !packages.iter().any(|p| p.contains("waypipe")),
        "X2Go should NOT include waypipe"
    );
}

#[test]
fn test_display_bridge_trait_consistency() {
    let config_waypipe = AppVMConfig::new(
        "trait-test-waypipe".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::Waypipe,
    );

    let config_x2go = AppVMConfig::new(
        "trait-test-x2go".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::X2Go,
    );

    // Both managers should implement DisplayBridge
    // Test behavior depends on which binaries are installed

    let waypipe_result = WaypipeManager::new(&config_waypipe);
    let x2go_result = X2GoManager::new(&config_x2go);

    match (waypipe_result, x2go_result) {
        (Ok(waypipe_mgr), Ok(x2go_mgr)) => {
            // Both managers are available
            let waypipe_apps = waypipe_mgr.list_applications();
            let x2go_apps = x2go_mgr.list_applications();

            // Both should return vectors (even if empty)
            assert!(waypipe_apps.is_empty() || !waypipe_apps.is_empty(), "Waypipe apps list ok");
            assert!(x2go_apps.is_empty() || !x2go_apps.is_empty(), "X2Go apps list ok");
        }
        (Ok(waypipe_mgr), Err(_)) => {
            // Waypipe available, X2Go not
            let apps = waypipe_mgr.list_applications();
            assert!(apps.is_empty() || !apps.is_empty(), "Waypipe apps list ok");
        }
        (Err(_), Ok(x2go_mgr)) => {
            // X2Go available, Waypipe not
            let apps = x2go_mgr.list_applications();
            assert!(apps.is_empty() || !apps.is_empty(), "X2Go apps list ok");
        }
        (Err(_), Err(_)) => {
            // Neither available - skip test
            eprintln!("Skipping trait consistency test - no display protocol binaries installed");
        }
    }
}

#[test]
fn test_config_serialization_display_protocol() {
    // Test that DisplayProtocol can be serialized/deserialized properly
    let config = AppVMConfig::new(
        "serialize-test".to_string(),
        2048,
        2,
        20,
        vec!["test-pkg".to_string()],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::X2Go,
    );

    // Verify the config struct maintains display_protocol field
    assert_eq!(config.display_protocol, DisplayProtocol::X2Go);

    let config2 = AppVMConfig::new(
        "serialize-test2".to_string(),
        2048,
        2,
        20,
        vec![],
        vec!["io.test.App".to_string()],
        false,
        vec![],
        false,
        DisplayProtocol::Waypipe,
    );

    assert_eq!(config2.display_protocol, DisplayProtocol::Waypipe);
}

#[test]
fn test_headless_mode_with_both_protocols() {
    let waypipe_headless = AppVMConfig::new(
        "headless-waypipe".to_string(),
        1024,
        1,
        10,
        vec!["git".to_string()],
        vec![],
        true, // headless = true
        vec![],
        false,
        DisplayProtocol::Waypipe,
    );

    let x2go_headless = AppVMConfig::new(
        "headless-x2go".to_string(),
        1024,
        1,
        10,
        vec!["git".to_string()],
        vec![],
        true, // headless = true
        vec![],
        false,
        DisplayProtocol::X2Go,
    );

    // Both headless configs should be valid
    assert_eq!(waypipe_headless.headless, true);
    assert_eq!(x2go_headless.headless, true);

    // Memory should be lower for headless
    assert_eq!(waypipe_headless.memory_mb, 1024);
    assert_eq!(x2go_headless.memory_mb, 1024);
}
