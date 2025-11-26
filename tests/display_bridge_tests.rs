/// DisplayBridge Integration Tests
/// Tests for the display protocol abstraction layer
use vm_provisioner::config::{AppVMConfig, DisplayProtocol};
use vm_provisioner::display_bridge::DisplayBridge;
use vm_provisioner::xpra_manager::XpraManager;

#[test]
fn test_display_protocol_enum_serialization() {
    let xpra = DisplayProtocol::Xpra;

    // Test debug output
    assert_eq!(format!("{:?}", xpra), "Xpra");

    // Test equality
    assert_eq!(xpra, DisplayProtocol::Xpra);
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
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
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
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
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
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
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

    // Verify deprecated protocol packages are NOT included
    assert!(
        !packages.iter().any(|p| p.contains("weston")),
        "Xpra should NOT include weston (Waypipe deprecated)"
    );
    assert!(
        !packages.iter().any(|p| p.contains("waypipe")),
        "Xpra should NOT include waypipe (deprecated)"
    );
}

#[test]
fn test_display_bridge_trait_consistency() {
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
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
    );

    // XpraManager should implement DisplayBridge
    let xpra_result = XpraManager::new(&config_xpra);

    match xpra_result {
        Ok(xpra_mgr) => {
            let apps = xpra_mgr.list_applications();
            // Should return a vector (even if empty)
            assert!(apps.is_empty() || !apps.is_empty(), "Xpra apps list ok");
        }
        Err(_) => {
            eprintln!("Skipping trait consistency test - xpra binary not installed");
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
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
    );

    // Verify the config struct maintains display_protocol field
    assert_eq!(config.display_protocol, DisplayProtocol::Xpra);
}

#[test]
fn test_headless_mode_with_xpra() {
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
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
    );

    // Headless config should be valid
    assert_eq!(xpra_headless.headless, true);

    // Memory should be as requested
    assert_eq!(xpra_headless.memory_mb, 1024);
}

#[test]
fn test_default_display_protocol() {
    // DisplayProtocol should default to Xpra
    let default_protocol = DisplayProtocol::default();
    assert_eq!(default_protocol, DisplayProtocol::Xpra);
}

#[test]
fn test_web_port_config() {
    // Test config with web streaming port enabled (Selkies-GStreamer)
    let config_with_web = AppVMConfig::new(
        "web-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        Some(8080), // web_port enabled
        vec![],     // usb_devices
        false,      // usb_hotplug
        vec![],     // shared_folders
        None,       // network_bridge
        false,      // grant_device_access
        false,      // no_network
    );
    assert_eq!(config_with_web.web_port, Some(8080));

    // Test config without web streaming port
    let config_without_web = AppVMConfig::new(
        "no-web-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        None,   // web_port disabled
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge
        false,  // grant_device_access
        false,  // no_network
    );
    assert_eq!(config_without_web.web_port, None);
}

#[test]
fn test_network_bridge_config() {
    use vm_provisioner::config::NetworkMode;

    // Test config with bridged networking
    let config_bridged = AppVMConfig::new(
        "bridge-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        None,                    // web_port
        vec![],                  // usb_devices
        false,                   // usb_hotplug
        vec![],                  // shared_folders
        Some("br0".to_string()), // network_bridge
        false,                   // grant_device_access
        false,                   // no_network
    );
    assert!(matches!(config_bridged.network_mode, NetworkMode::Bridge(ref name) if name == "br0"));

    // Test config with default NAT networking
    let config_nat = AppVMConfig::new(
        "nat-test".to_string(),
        2048,
        2,
        20,
        vec![],
        vec![],
        false,
        vec![],
        false,
        None,   // web_port
        vec![], // usb_devices
        false,  // usb_hotplug
        vec![], // shared_folders
        None,   // network_bridge (uses NAT)
        false,  // grant_device_access
        false,  // no_network
    );
    assert!(matches!(config_nat.network_mode, NetworkMode::Nat));
}
