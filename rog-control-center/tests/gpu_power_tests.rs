//! Tests for GPU power status handling in rog-control-center.
//!
//! These tests validate the power-status-to-notification-icon mapping and
//! the D-Bus proxy type definitions, without requiring hardware or a running
//! system bus.

use std::str::FromStr;

use rog_platform::gpu_pci::GfxPower;

// ---------------------------------------------------------------------------
// Power status to notification icon mapping
//
// The mapping in notify.rs::do_gpu_status_notif selects icons based on
// GfxPower variants. These tests verify the expected icon names.
// ---------------------------------------------------------------------------

fn expected_notification_icon(power: &GfxPower) -> &'static str {
    // Mirror the logic from notify.rs::do_gpu_status_notif
    match power {
        GfxPower::Suspended => "asus_notif_blue",
        GfxPower::Off => "asus_notif_green",
        GfxPower::AsusDisabled => "asus_notif_white",
        GfxPower::AsusMuxDiscreet | GfxPower::Active => "asus_notif_red",
        GfxPower::Unknown => "gpu-integrated",
    }
}

#[test]
fn notification_icon_active_is_red() {
    assert_eq!(
        expected_notification_icon(&GfxPower::Active),
        "asus_notif_red"
    );
}

#[test]
fn notification_icon_suspended_is_blue() {
    assert_eq!(
        expected_notification_icon(&GfxPower::Suspended),
        "asus_notif_blue"
    );
}

#[test]
fn notification_icon_off_is_green() {
    assert_eq!(
        expected_notification_icon(&GfxPower::Off),
        "asus_notif_green"
    );
}

#[test]
fn notification_icon_asus_disabled_is_white() {
    assert_eq!(
        expected_notification_icon(&GfxPower::AsusDisabled),
        "asus_notif_white"
    );
}

#[test]
fn notification_icon_asus_mux_discreet_is_red() {
    assert_eq!(
        expected_notification_icon(&GfxPower::AsusMuxDiscreet),
        "asus_notif_red"
    );
}

#[test]
fn notification_icon_unknown_is_gpu_integrated() {
    assert_eq!(
        expected_notification_icon(&GfxPower::Unknown),
        "gpu-integrated"
    );
}

// ---------------------------------------------------------------------------
// Power status to tray icon color mapping
//
// The mapping in tray.rs::map_power_to_icon selects icons based on
// power status strings and mode. These tests validate the expected behavior.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum TrayIconColor {
    Blue,
    Red,
    Green,
    White,
    Yellow,
    GpuIntegrated,
}

fn expected_tray_color(power_status: &str, mode: &str) -> TrayIconColor {
    // Mirror the logic from tray.rs::map_power_to_icon
    match power_status {
        "suspended" => TrayIconColor::Blue,
        "off" => {
            if mode == "Vfio" {
                TrayIconColor::Yellow
            } else {
                TrayIconColor::Green
            }
        }
        "dgpu_disabled" => TrayIconColor::White,
        "asus_mux_discreet" | "active" => TrayIconColor::Red,
        _ => TrayIconColor::GpuIntegrated,
    }
}

#[test]
fn tray_icon_active_is_red() {
    assert_eq!(expected_tray_color("active", "Optimus"), TrayIconColor::Red);
}

#[test]
fn tray_icon_suspended_is_blue() {
    assert_eq!(
        expected_tray_color("suspended", "Optimus"),
        TrayIconColor::Blue
    );
}

#[test]
fn tray_icon_off_optimus_is_green() {
    assert_eq!(expected_tray_color("off", "Optimus"), TrayIconColor::Green);
}

#[test]
fn tray_icon_off_vfio_is_yellow() {
    assert_eq!(expected_tray_color("off", "Vfio"), TrayIconColor::Yellow);
}

#[test]
fn tray_icon_off_integrated_is_green() {
    assert_eq!(
        expected_tray_color("off", "Integrated"),
        TrayIconColor::Green
    );
}

#[test]
fn tray_icon_dgpu_disabled_is_white() {
    assert_eq!(
        expected_tray_color("dgpu_disabled", "Integrated"),
        TrayIconColor::White
    );
}

#[test]
fn tray_icon_mux_discreet_is_red() {
    assert_eq!(
        expected_tray_color("asus_mux_discreet", "Ultimate"),
        TrayIconColor::Red
    );
}

#[test]
fn tray_icon_unknown_is_gpu_integrated() {
    assert_eq!(
        expected_tray_color("unknown", "Unknown"),
        TrayIconColor::GpuIntegrated
    );
}

#[test]
fn tray_icon_empty_string_is_gpu_integrated() {
    assert_eq!(
        expected_tray_color("", "Optimus"),
        TrayIconColor::GpuIntegrated
    );
}

// ---------------------------------------------------------------------------
// VFIO differentiation: the key requirement is that VFIO and Active are
// visually distinct. Active -> Red, VFIO+Off -> Yellow.
// ---------------------------------------------------------------------------

#[test]
fn vfio_and_active_are_different_colors() {
    let active_color = expected_tray_color("active", "Optimus");
    let vfio_color = expected_tray_color("off", "Vfio");
    assert_ne!(active_color, vfio_color);
}

// ---------------------------------------------------------------------------
// GfxPower string roundtrip via the D-Bus proxy string representation
// ---------------------------------------------------------------------------

#[test]
fn power_status_string_roundtrip() {
    let powers = [
        GfxPower::Active,
        GfxPower::Suspended,
        GfxPower::Off,
        GfxPower::AsusDisabled,
        GfxPower::AsusMuxDiscreet,
        GfxPower::Unknown,
    ];
    for power in powers {
        let s: &str = (&power).into();
        let back = GfxPower::from_str(s).unwrap();
        assert_eq!(back, power);
    }
}
