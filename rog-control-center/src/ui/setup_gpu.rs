use std::sync::Arc;

use log::error;
use rog_dbus::asus_armoury::AsusArmouryProxy;
use rog_platform::asus_armoury::FirmwareAttribute;
use slint::{ComponentHandle, SharedString, Weak};

use super::show_toast;
use crate::zbus_proxies::find_iface_async;
use crate::{GPUPageData, MainWindow};

/// A selectable GPU mode, independent of which sysfs attributes back it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GpuMode {
    Integrated, // dGPU powered off
    Hybrid,     // iGPU + dGPU (Optimus)
    Ultimate,   // dGPU drives the displays directly (MUX)
}

impl GpuMode {
    fn label(self) -> &'static str {
        match self {
            GpuMode::Integrated => "Integrated",
            GpuMode::Hybrid => "Hybrid",
            GpuMode::Ultimate => "Ultimate",
        }
    }
}

/// GPU switching capabilities exposed by asusd, plus the proxies to drive them.
///
/// `gpu_mux_mode` and `dgpu_disable` are independent firmware attributes: a
/// laptop may expose either, both, or neither. The available modes and whether
/// the control is shown at all are derived from which proxies are present.
struct GpuCaps {
    dgpu: Option<AsusArmouryProxy<'static>>,
    mux: Option<AsusArmouryProxy<'static>>,
}

impl GpuCaps {
    /// Collect the dgpu_disable / gpu_mux_mode proxies from asusd.
    async fn discover() -> Result<Self, Box<dyn std::error::Error>> {
        let attrs = find_iface_async::<AsusArmouryProxy>("xyz.ljones.AsusArmoury").await?;
        let mut caps = GpuCaps {
            dgpu: None,
            mux: None,
        };
        for attr in attrs {
            match attr.name().await {
                Ok(FirmwareAttribute::DgpuDisable) => caps.dgpu = Some(attr),
                Ok(FirmwareAttribute::GpuMuxMode) => caps.mux = Some(attr),
                Ok(_) => {}
                Err(e) => error!("setup_gpu_page: failed to read attribute name: {e:?}"),
            }
        }
        Ok(caps)
    }

    /// Whether any GPU switching is possible on this hardware.
    fn switchable(&self) -> bool {
        self.dgpu.is_some() || self.mux.is_some()
    }

    /// Modes this hardware supports, in display order.
    fn modes(&self) -> Vec<GpuMode> {
        let mut modes = Vec::new();
        if self.dgpu.is_some() {
            modes.push(GpuMode::Integrated);
        }
        if self.switchable() {
            modes.push(GpuMode::Hybrid);
        }
        if self.mux.is_some() {
            modes.push(GpuMode::Ultimate);
        }
        modes
    }

    /// Read the current hardware state and map it to a mode.
    async fn current_mode(&self) -> GpuMode {
        // Absent attributes report their inactive default so they never win below;
        // a read error is logged and falls back to the same default.
        let mux = match &self.mux {
            Some(a) => a.current_value().await.unwrap_or_else(|e| {
                error!("setup_gpu: failed to read gpu_mux_mode: {e:?}");
                1
            }),
            None => 1,
        };
        let dgpu = match &self.dgpu {
            Some(a) => a.current_value().await.unwrap_or_else(|e| {
                error!("setup_gpu: failed to read dgpu_disable: {e:?}");
                0
            }),
            None => 0,
        };
        if mux == 0 {
            GpuMode::Ultimate
        } else if dgpu == 1 {
            GpuMode::Integrated
        } else {
            GpuMode::Hybrid
        }
    }

    /// Apply a target mode, writing only the attributes that exist.
    async fn apply(&self, mode: GpuMode) -> zbus::Result<()> {
        let (dgpu_val, mux_val) = match mode {
            GpuMode::Integrated => (1, 1),
            GpuMode::Hybrid => (0, 1),
            GpuMode::Ultimate => (0, 0),
        };
        if let Some(attr) = &self.dgpu {
            attr.set_current_value(dgpu_val).await?;
        }
        if let Some(attr) = &self.mux {
            attr.set_current_value(mux_val).await?;
        }
        Ok(())
    }
}

/// Index of `mode` within `modes`, defaulting to 0 if not found.
fn index_of(modes: &[GpuMode], mode: GpuMode) -> i32 {
    modes.iter().position(|m| *m == mode).unwrap_or(0) as i32
}

fn set_dropdown_enabled(handle: &Weak<MainWindow>, enabled: bool) {
    handle
        .upgrade_in_event_loop(move |h| {
            h.global::<GPUPageData>().set_gpu_dropdown_enabled(enabled);
        })
        .unwrap_or_else(|e| error!("setup_gpu: failed to set dropdown state: {e:?}"));
}

/// Disable the dropdown, apply `mode`, toast the result, then refresh + re-enable.
fn set_gpu_mode(caps: Arc<GpuCaps>, handle: Weak<MainWindow>, mode: GpuMode) {
    // Called from the slint callback on the event-loop thread, so disable the
    // dropdown synchronously here — a second selection can't slip in before the
    // write is queued.
    if let Some(h) = handle.upgrade() {
        h.global::<GPUPageData>().set_gpu_dropdown_enabled(false);
    }

    tokio::spawn(async move {
        let result = caps.apply(mode).await;
        show_toast(
            SharedString::from("GPU mode change scheduled — reboot required for changes to apply."),
            SharedString::from("Failed to set GPU mode"),
            handle.clone(),
            result,
        );

        // Reflect the (possibly unchanged) hardware state back into the dropdown.
        let new_index = index_of(&caps.modes(), caps.current_mode().await);
        handle
            .upgrade_in_event_loop(move |h| {
                h.global::<GPUPageData>().set_gpu_mode_index(new_index);
            })
            .unwrap_or_else(|e| error!("setup_gpu: failed to refresh mode: {e:?}"));

        set_dropdown_enabled(&handle, true);
    });
}

// Populate GPU page choices and wire the `cb_set_gpu_mode` callback
pub fn setup_gpu_page(ui: &MainWindow) {
    let handle = ui.as_weak();

    tokio::spawn(async move {
        let caps = match GpuCaps::discover().await {
            Ok(caps) => Arc::new(caps),
            Err(e) => {
                error!("setup_gpu_page: failed to get AsusArmoury proxies: {e:?}");
                return;
            }
        };

        let modes = caps.modes();
        let switchable = caps.switchable();
        let current_index = index_of(&modes, caps.current_mode().await);
        let choices: Vec<SharedString> = modes
            .iter()
            .map(|m| SharedString::from(m.label()))
            .collect();

        let caps_cb = caps.clone();
        let handle_cb = handle.clone();
        if let Err(e) = handle.upgrade_in_event_loop(move |handle| {
            let global = handle.global::<GPUPageData>();
            global.set_gpu_modes(choices.as_slice().into());
            global.set_gpu_switchable(switchable);
            global.set_gpu_dropdown_enabled(switchable);
            global.set_gpu_mode_index(current_index);

            global.on_cb_set_gpu_mode(move |index| {
                let Some(mode) = modes.get(index as usize).copied() else {
                    return;
                };
                set_gpu_mode(caps_cb.clone(), handle_cb.clone(), mode);
            });
        }) {
            error!("setup_gpu_page: upgrade_in_event_loop: {e:?}");
        }
    });
}
