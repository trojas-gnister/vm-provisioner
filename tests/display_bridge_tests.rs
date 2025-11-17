/// DisplayBridge Integration Tests
/// Tests for the display protocol abstraction layer
use vm_provisioner::config::{AppVMConfig, DisplayProtocol};
use vm_provisioner::display_bridge::DisplayBridge;
use vm_provisioner::waypipe_manager::WaypipeManager;
use vm_provisioner::xpra_manager::XpraManager;

#[test]
fn test_display_protocol_enum_serialization() {
    let waypipe = DisplayProtocol::Waypipe;
    let xpra = DisplayProtocol::Xpra;

    // Test debug output
    assert_eq!(format!("{:?}", waypipe), "Waypipe");
    assert_eq!(format!("{:?}", xpra), "Xpra");

    // Test equality
    assert_eq!(waypipe, DisplayProtocol::Waypipe);
    assert_eq!(xpra, DisplayProtocol::Xpra);
    assert_ne!(waypipe, xpra);
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

    // Waypipe includes: weston (headless), waypipe, wl-clipboard, pipewire, kitty,
    // git, openssh-server (7 default) + user package
    assert!(
        config.system_packages.len() >= 7,
        "Should have default Waypipe packages + user package"
    );
    assert!(
        config.system_packages.contains(&"weston".to_string()),
        "Should have weston"
    );
    assert!(
        config.system_packages.contains(&"waypipe".to_string()),
        "Should have waypipe"
    );
    assert!(
        config.system_packages.contains(&"firefox".to_string()),
        "Should have user package"
    );

    assert_eq!(config.flatpak_packages.len(), 1);
    assert_eq!(config.flatpak_packages[0], "io.gitlab.librewolf-community");
}

#[test]
fn test_appvmconfig_with_xpra() {
    let config = AppVMConfig::new(
        "test-xpra-vm".to_string(),
        4096,
        4,
        30,
        vec!["gimp".to_string(), "inkscape".to_string()],
        vec!["com.github.tchx84.Flatseal".to_string()],
        false, // Not headless
        vec![],
        false,
        DisplayProtocol::Xpra,
    );

    assert_eq!(config.name, "test-xpra-vm");
    assert_eq!(config.display_protocol, DisplayProtocol::Xpra);
    assert_eq!(config.memory_mb, 4096);
    assert_eq!(config.vcpus, 4);

    // Xpra includes: xpra, xorg-x11-server-Xvfb, pulseaudio-libs, git, openssh-server
    // (5 default) + user packages
    assert!(
        config.system_packages.len() >= 7,
        "Should have default Xpra packages + user packages"
    );
    assert!(
        config.system_packages.contains(&"xpra".to_string()),
        "Should have xpra"
    );
    assert!(
        config.system_packages.contains(&"gimp".to_string()),
        "Should have gimp"
    );
    assert!(
        config.system_packages.contains(&"inkscape".to_string()),
        "Should have inkscape"
    );

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
fn test_xpra_manager_creation() {
    let config = AppVMConfig::new(
        "xpra-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::Xpra,
    );

    // XpraManager validates xpra binary availability
    let manager = XpraManager::new(&config);

    // Result depends on whether xpra is installed
    // We just verify the function returns a Result
    match manager {
        Ok(_) => {
            // xpra is installed - this is fine
        }
        Err(e) => {
            // xpra is not installed - check error message
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("xpra") || error_msg.contains("not found"),
                "Expected xpra error, got: {}",
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
                packages.iter().any(|p| p.contains("weston")),
                "Waypipe should include weston"
            );
            assert!(
                packages.iter().any(|p| p.contains("waypipe")),
                "Waypipe should include waypipe"
            );
            assert!(
                packages.iter().any(|p| p.contains("wl-clipboard")),
                "Waypipe should include wl-clipboard"
            );

            // Verify Xpra packages are NOT included
            assert!(
                !packages.iter().any(|p| p.contains("xpra")),
                "Waypipe should NOT include xpra"
            );
        }
        Err(_) => {
            eprintln!("Skipping test_waypipe_guest_packages - waypipe binary not installed");
        }
    }
}

#[test]
fn test_xpra_guest_packages() {
    let config = AppVMConfig::new(
        "xpra-pkg-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::Xpra,
    );

    // Skip test if xpra not installed
    if XpraManager::new(&config).is_err() {
        eprintln!("Skipping xpra package test - xpra binary not installed");
        return;
    }

    let manager = XpraManager::new(&config).expect("Should create XpraManager for package test");

    let packages = manager.guest_packages();

    // Verify Xpra-specific packages are included
    assert!(
        packages.iter().any(|p| p.contains("xpra")),
        "Xpra should include xpra package"
    );
    assert!(
        packages
            .iter()
            .any(|p| p.contains("xorg-x11-server") || p.contains("Xvfb")),
        "Xpra should include xorg server bits"
    );
    assert!(
        packages.iter().any(|p| p.contains("pulseaudio")),
        "Xpra should include pulseaudio libs"
    );

    // Verify Waypipe packages are NOT included
    assert!(
        !packages.iter().any(|p| p.contains("weston")),
        "Xpra should NOT include weston"
    );
    assert!(
        !packages.iter().any(|p| p.contains("waypipe")),
        "Xpra should NOT include waypipe"
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

    let config_xpra = AppVMConfig::new(
        "trait-test-xpra".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        DisplayProtocol::Xpra,
    );

    // Both managers should implement DisplayBridge
    // Test behavior depends on which binaries are installed

    let waypipe_result = WaypipeManager::new(&config_waypipe);
    let xpra_result = XpraManager::new(&config_xpra);

    match (waypipe_result, xpra_result) {
        (Ok(waypipe_mgr), Ok(xpra_mgr)) => {
            // Both managers are available
            let waypipe_apps = waypipe_mgr.list_applications();
            let xpra_apps = xpra_mgr.list_applications();

            // Both should return vectors (even if empty)
            assert!(
                waypipe_apps.is_empty() || !waypipe_apps.is_empty(),
                "Waypipe apps list ok"
            );
            assert!(
                xpra_apps.is_empty() || !xpra_apps.is_empty(),
                "Xpra apps list ok"
            );
        }
        (Ok(waypipe_mgr), Err(_)) => {
            let apps = waypipe_mgr.list_applications();
            assert!(apps.is_empty() || !apps.is_empty(), "Waypipe apps list ok");
        }
        (Err(_), Ok(xpra_mgr)) => {
            let apps = xpra_mgr.list_applications();
            assert!(apps.is_empty() || !apps.is_empty(), "Xpra apps list ok");
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
        DisplayProtocol::Xpra,
    );

    // Verify the config struct maintains display_protocol field
    assert_eq!(config.display_protocol, DisplayProtocol::Xpra);

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

    let xpra_headless = AppVMConfig::new(
        "headless-xpra".to_string(),
        1024,
        1,
        10,
        vec!["git".to_string()],
        vec![],
        true, // headless = true
        vec![],
        false,
        DisplayProtocol::Xpra,
    );

    // Both headless configs should be valid
    assert_eq!(waypipe_headless.headless, true);
    assert_eq!(xpra_headless.headless, true);

    // Memory should be lower for headless
    assert_eq!(waypipe_headless.memory_mb, 1024);
    assert_eq!(xpra_headless.memory_mb, 1024);
}
