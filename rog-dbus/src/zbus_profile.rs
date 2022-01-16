//! # DBus interface proxy for: `org.asuslinux.Daemon`
//!
//! This code was generated by `zbus-xmlgen` `1.0.0` from DBus introspection data.
//! Source: `Interface '/org/asuslinux/Profile' from service 'org.asuslinux.Daemon' on system bus`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://zeenix.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This DBus object implements
//! [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PeerProxy`]
//! * [`zbus::fdo::PropertiesProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

use rog_profiles::{
    fan_curve_set::{CurveData, FanCurveSet},
    Profile,
};
use zbus_macros::dbus_proxy;

#[dbus_proxy(
    interface = "org.asuslinux.Daemon",
    default_path = "/org/asuslinux/Profile"
)]
trait Profile {
    /// Get the fan-curve data for the currently active Profile
    fn fan_curve_data(&self, profile: Profile) -> zbus::Result<FanCurveSet>;

    /// Fetch the active profile name
    fn active_profile(&self) -> zbus::Result<Profile>;

    /// Get a list of profiles that have fan-curves enabled.
    fn enabled_fan_profiles(&self) -> zbus::Result<Vec<Profile>>;

    /// Toggle to next platform_profile. Names provided by `Profiles`.
    /// If fan-curves are supported will also activate a fan curve for profile.
    fn next_profile(&self) -> zbus::Result<()>;

    /// Fetch profile names
    fn profiles(&self) -> zbus::Result<Vec<Profile>>;

    /// Set this platform_profile name as active
    fn set_active_profile(&self, profile: Profile) -> zbus::Result<()>;

    /// Set a profile fan curve enabled status. Will also activate a fan curve.
    fn set_fan_curve_enabled(&self, profile: Profile, enabled: bool) -> zbus::Result<()>;

    /// Set the fan curve for the specified profile, or the profile the user is
    /// currently in if profile == None. Will also activate the fan curve.
    fn set_fan_curve(&self, profile: Profile, curve: CurveData) -> zbus::Result<()>;

    /// Reset the stored (self) and device curve to the defaults of the platform.
    ///
    /// Each platform_profile has a different default and the defualt can be read
    /// only for the currently active profile.
    fn set_active_curve_to_defaults(&self) -> zbus::Result<()>;

    /// NotifyProfile signal
    #[dbus_proxy(signal)]
    fn notify_profile(&self, profile: Profile) -> zbus::Result<Profile>;
}
