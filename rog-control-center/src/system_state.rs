use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

use egui::Vec2;
use rog_aura::{layouts::KeyLayout, usb::AuraPowerDev, AuraEffect, AuraModeNum};
use rog_platform::{platform::GpuMode, supported::SupportedFunctions};
use rog_profiles::{fan_curve_set::FanCurveSet, FanCurvePU, Profile};
use supergfxctl::{
    pci_device::{GfxMode, GfxPower},
    zbus_proxy::DaemonProxyBlocking as GfxProxyBlocking,
};

use crate::{error::Result, update_and_notify::EnabledNotifications, RogDbusClientBlocking};
use log::error;

#[derive(Clone, Debug, Default)]
pub struct BiosState {
    /// To be shared to a thread that checks notifications.
    /// It's a bit general in that it won't provide *what* was
    /// updated, so the full state needs refresh
    pub post_sound: bool,
    pub dedicated_gfx: GpuMode,
    pub panel_overdrive: bool,
    pub dgpu_disable: bool,
    pub egpu_enable: bool,
}

impl BiosState {
    pub fn new(supported: &SupportedFunctions, dbus: &RogDbusClientBlocking<'_>) -> Result<Self> {
        Ok(Self {
            post_sound: if supported.rog_bios_ctrl.post_sound {
                dbus.proxies().rog_bios().post_boot_sound()? != 0
            } else {
                false
            },
            dedicated_gfx: if supported.rog_bios_ctrl.gpu_mux {
                dbus.proxies().rog_bios().gpu_mux_mode()?
            } else {
                GpuMode::NotSupported
            },
            panel_overdrive: if supported.rog_bios_ctrl.panel_overdrive {
                dbus.proxies().rog_bios().panel_od()?
            } else {
                false
            },
            // TODO: needs supergfx
            dgpu_disable: supported.rog_bios_ctrl.dgpu_disable,
            egpu_enable: supported.rog_bios_ctrl.egpu_enable,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProfilesState {
    pub list: Vec<Profile>,
    pub current: Profile,
}

impl ProfilesState {
    pub fn new(supported: &SupportedFunctions, dbus: &RogDbusClientBlocking<'_>) -> Result<Self> {
        Ok(Self {
            list: if supported.platform_profile.platform_profile {
                let mut list = dbus.proxies().profile().profiles()?;
                list.sort();
                list
            } else {
                vec![]
            },
            current: if supported.platform_profile.platform_profile {
                dbus.proxies().profile().active_profile()?
            } else {
                Profile::Balanced
            },
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FanCurvesState {
    pub show_curve: Profile,
    pub show_graph: FanCurvePU,
    pub enabled: HashSet<Profile>,
    pub curves: BTreeMap<Profile, FanCurveSet>,
    pub drag_delta: Vec2,
}

impl FanCurvesState {
    pub fn new(supported: &SupportedFunctions, dbus: &RogDbusClientBlocking<'_>) -> Result<Self> {
        let profiles = if supported.platform_profile.platform_profile {
            dbus.proxies().profile().profiles()?
        } else {
            vec![Profile::Balanced, Profile::Quiet, Profile::Performance]
        };
        let enabled = if supported.platform_profile.fan_curves {
            dbus.proxies()
                .profile()
                .enabled_fan_profiles()?
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        } else {
            HashSet::from([Profile::Balanced, Profile::Quiet, Profile::Performance])
        };

        let mut curves: BTreeMap<Profile, FanCurveSet> = BTreeMap::new();
        for p in &profiles {
            if supported.platform_profile.fan_curves {
                if let Ok(curve) = dbus.proxies().profile().fan_curve_data(*p) {
                    curves.insert(*p, curve);
                }
            } else {
                let mut curve = FanCurveSet::default();
                curve.cpu.pwm = [30, 40, 60, 100, 140, 180, 200, 250];
                curve.cpu.temp = [20, 30, 40, 50, 70, 80, 90, 100];
                curve.gpu.pwm = [40, 80, 100, 140, 170, 200, 230, 250];
                curve.gpu.temp = [20, 30, 40, 50, 70, 80, 90, 100];
                curves.insert(*p, curve);
            }
        }

        let show_curve = if supported.platform_profile.fan_curves {
            dbus.proxies().profile().active_profile()?
        } else {
            Profile::Balanced
        };

        Ok(Self {
            show_curve,
            show_graph: FanCurvePU::CPU,
            enabled,
            curves,
            drag_delta: Vec2::default(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct AuraState {
    pub current_mode: AuraModeNum,
    pub modes: BTreeMap<AuraModeNum, AuraEffect>,
    pub enabled: AuraPowerDev,
    /// Brightness from 0-3
    pub bright: i16,
    pub wave_red: [u8; 22],
    pub wave_green: [u8; 22],
    pub wave_blue: [u8; 22],
}

impl AuraState {
    pub fn new(supported: &SupportedFunctions, dbus: &RogDbusClientBlocking<'_>) -> Result<Self> {
        Ok(Self {
            current_mode: if !supported.keyboard_led.stock_led_modes.is_empty() {
                dbus.proxies().led().led_mode().unwrap_or_default()
            } else {
                AuraModeNum::Static
            },

            modes: if !supported.keyboard_led.stock_led_modes.is_empty() {
                dbus.proxies().led().led_modes().unwrap_or_default()
            } else {
                BTreeMap::new()
            },
            enabled: dbus.proxies().led().leds_enabled().unwrap_or_default(),
            bright: if !supported.keyboard_led.brightness_set {
                dbus.proxies().led().led_brightness().unwrap_or_default()
            } else {
                2
            },
            wave_red: [0u8; 22],
            wave_green: [0u8; 22],
            wave_blue: [0u8; 22],
        })
    }

    /// Bump value in to the wave and surf all along.
    pub fn nudge_wave(&mut self, r: u8, g: u8, b: u8) {
        for i in (0..self.wave_red.len()).rev() {
            if i > 0 {
                self.wave_red[i] = self.wave_red[i - 1];
                self.wave_green[i] = self.wave_green[i - 1];
                self.wave_blue[i] = self.wave_blue[i - 1];
            }
        }
        self.wave_red[0] = r;
        self.wave_green[0] = g;
        self.wave_blue[0] = b;
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimeState {
    pub bright: u8,
    pub boot: bool,
    pub awake: bool,
    pub sleep: bool,
}

impl AnimeState {
    pub fn new(supported: &SupportedFunctions, dbus: &RogDbusClientBlocking<'_>) -> Result<Self> {
        Ok(Self {
            boot: if supported.anime_ctrl.0 {
                dbus.proxies().anime().boot_enabled()?
            } else {
                false
            },
            awake: if supported.anime_ctrl.0 {
                dbus.proxies().anime().awake_enabled()?
            } else {
                false
            },
            // TODO:
            sleep: false,
            bright: 200,
        })
    }
}

#[derive(Clone, Debug)]
pub struct GfxState {
    pub has_supergfx: bool,
    pub mode: GfxMode,
    pub power_status: GfxPower,
}

impl GfxState {
    pub fn new(_supported: &SupportedFunctions, dbus: &GfxProxyBlocking<'_>) -> Result<Self> {
        Ok(Self {
            has_supergfx: dbus.mode().is_ok(),
            mode: dbus.mode().unwrap_or_default(),
            power_status: dbus.power().unwrap_or_default(),
        })
    }
}

impl Default for GfxState {
    fn default() -> Self {
        Self {
            has_supergfx: false,
            mode: GfxMode::None,
            power_status: GfxPower::Unknown,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PowerState {
    pub charge_limit: u8,
    pub ac_power: bool,
}

impl PowerState {
    pub fn new(_supported: &SupportedFunctions, dbus: &RogDbusClientBlocking<'_>) -> Result<Self> {
        Ok(Self {
            charge_limit: dbus.proxies().charge().charge_control_end_threshold()?,
            ac_power: dbus.proxies().charge().mains_online()?,
        })
    }
}

///  State stored from system daemons. This is shared with: tray, zbus notifications thread
/// and the GUI app thread.
pub struct SystemState {
    pub keyboard_layout: KeyLayout,
    pub enabled_notifications: Arc<Mutex<EnabledNotifications>>,
    /// Because much of the app state here is the same as `RogBiosSupportedFunctions`
    /// we can re-use that structure.
    pub bios: BiosState,
    pub aura: AuraState,
    pub anime: AnimeState,
    pub profiles: ProfilesState,
    pub fan_curves: FanCurvesState,
    pub gfx_state: GfxState,
    pub power_state: PowerState,
    pub error: Option<String>,
    /// Specific field for the tray only so that we can know when it does need update.
    /// The tray should set this to false when done.
    pub tray_should_update: bool,
    pub app_should_update: bool,
    pub asus_dbus: RogDbusClientBlocking<'static>,
    pub gfx_dbus: GfxProxyBlocking<'static>,
}

impl SystemState {
    /// Creates self, including the relevant dbus connections and proixies for internal use
    pub fn new(
        keyboard_layout: KeyLayout,
        enabled_notifications: Arc<Mutex<EnabledNotifications>>,
        supported: &SupportedFunctions,
    ) -> Result<Self> {
        let (asus_dbus, conn) = RogDbusClientBlocking::new()?;
        let mut error = None;
        let gfx_dbus = GfxProxyBlocking::new(&conn).expect("Couldn't connect to supergfxd");
        Ok(Self {
            keyboard_layout,
            enabled_notifications,
            power_state: PowerState::new(supported, &asus_dbus)
                .map_err(|e| {
                    let e = format!("Could not get PowerState state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            bios: BiosState::new(supported, &asus_dbus)
                .map_err(|e| {
                    let e = format!("Could not get BiosState state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            aura: AuraState::new(supported, &asus_dbus)
                .map_err(|e| {
                    let e = format!("Could not get AuraState state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            anime: AnimeState::new(supported, &asus_dbus)
                .map_err(|e| {
                    let e = format!("Could not get AanimeState state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            profiles: ProfilesState::new(supported, &asus_dbus)
                .map_err(|e| {
                    let e = format!("Could not get ProfilesState state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            fan_curves: FanCurvesState::new(supported, &asus_dbus)
                .map_err(|e| {
                    let e = format!("Could not get FanCurvesState state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            gfx_state: GfxState::new(supported, &gfx_dbus)
                .map_err(|e| {
                    let e = format!("Could not get supergfxd state: {e}");
                    error!("{e}");
                    error = Some(e);
                })
                .unwrap_or_default(),
            error,
            tray_should_update: true,
            app_should_update: true,
            asus_dbus,
            gfx_dbus,
        })
    }

    pub fn set_notified(&mut self) {
        self.tray_should_update = true;
        self.app_should_update = true;
    }
}

impl Default for SystemState {
    fn default() -> Self {
        let (asus_dbus, conn) = RogDbusClientBlocking::new().expect("Couldn't connect to asusd");
        let gfx_dbus = GfxProxyBlocking::new(&conn).expect("Couldn't connect to supergfxd");

        Self {
            keyboard_layout: KeyLayout::ga401_layout(),
            enabled_notifications: Default::default(),
            bios: BiosState {
                post_sound: Default::default(),
                dedicated_gfx: GpuMode::NotSupported,
                panel_overdrive: Default::default(),
                dgpu_disable: Default::default(),
                egpu_enable: Default::default(),
            },
            aura: AuraState {
                current_mode: AuraModeNum::Static,
                modes: Default::default(),
                enabled: AuraPowerDev {
                    tuf: vec![],
                    x1866: vec![],
                    x19b6: vec![],
                },
                bright: Default::default(),
                wave_red: Default::default(),
                wave_green: Default::default(),
                wave_blue: Default::default(),
            },
            anime: AnimeState {
                bright: Default::default(),
                boot: Default::default(),
                awake: Default::default(),
                sleep: Default::default(),
            },
            profiles: ProfilesState {
                list: Default::default(),
                current: Default::default(),
            },
            fan_curves: FanCurvesState {
                show_curve: Default::default(),
                show_graph: Default::default(),
                enabled: Default::default(),
                curves: Default::default(),
                drag_delta: Default::default(),
            },
            gfx_state: GfxState {
                has_supergfx: false,
                mode: GfxMode::None,
                power_status: GfxPower::Unknown,
            },
            power_state: PowerState {
                charge_limit: 99,
                ac_power: false,
            },
            error: Default::default(),
            tray_should_update: true,
            app_should_update: true,
            asus_dbus,
            gfx_dbus,
        }
    }
}