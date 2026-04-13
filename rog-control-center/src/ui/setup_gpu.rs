use log::error;
use rog_platform::asus_armoury::{AttrValue, FirmwareAttributes};
use slint::{ComponentHandle, ModelRc, SharedString};

use crate::{GPUPageData, MainWindow};

// Populate GPU page choices and wire the `cb_set_gpu_mode` callback
pub fn setup_gpu_page(ui: &MainWindow) {
    let handle = ui.as_weak();

    tokio::spawn(async move {
        // Read available attributes
        let attrs = FirmwareAttributes::new();
        let gpu_mux_available = attrs
            .gpu_mux_mode()
            .map(|a| a.base_path_exists())
            .unwrap_or(false);

        // Prepare choice strings
        let mut choices: Vec<SharedString> = Vec::new();
        choices.push(SharedString::from("Integrated"));
        if gpu_mux_available {
            choices.push(SharedString::from("Ultimate"));
        }
        choices.push(SharedString::from("Hybrid"));

        // Read current attribute values to initialise UI state
        let current_dgpu = attrs
            .dgpu_disable()
            .and_then(|a| a.current_value().ok())
            .unwrap_or(AttrValue::Integer(0));
        let current_mux = attrs
            .gpu_mux_mode()
            .and_then(|a| a.current_value().ok())
            .unwrap_or(AttrValue::Integer(1));

        // Convert to UI-able values
        let dgpu_disabled = matches!(current_dgpu, AttrValue::Integer(v) if v == 1);
        // Determine initial index for gpu_mux_mode property
        let initial_index: i32 = if gpu_mux_available {
            // If mux attr says 0 -> Ultimate, else try dgpu to refine
            match current_mux {
                AttrValue::Integer(0) => 1, // Ultimate
                _ => {
                    match current_dgpu {
                        AttrValue::Integer(1) => 0, // Integrated
                        _ => 2,                     // Hybrid/Optimus fallback
                    }
                }
            }
        } else {
            // Only Integrated / Hybrid
            match current_dgpu {
                AttrValue::Integer(1) => 0,
                _ => 1,
            }
        };

        let handle_copy = handle.clone();
        if let Err(e) = handle.upgrade_in_event_loop(move |handle| {
            let global = handle.global::<GPUPageData>();

            // set choices model
            let model: ModelRc<SharedString> = choices.as_slice().into();
            global.set_gpu_modes_choises(model);
            global.set_gpu_mux_available(gpu_mux_available);

            // set initial state
            global.set_dgpu_disabled(if dgpu_disabled { 1 } else { 0 });
            global.set_gpu_mux_mode(initial_index);

            // register callback
            let handle_cb = handle_copy.clone();
            global.on_cb_set_gpu_mode(move |index: i32| {
                // show a blue toast informing user a reboot is required (auto-clears)
                let toast_handle = handle_cb.clone();
                crate::ui::show_toast(
                    SharedString::from(
                        "GPU mode change scheduled — reboot required for changes to apply.",
                    ),
                    SharedString::from("Failed to set GPU mode"),
                    toast_handle.clone(),
                    Ok(()),
                );

                let handle_next = handle_cb.clone();
                tokio::spawn(async move {
                    let attrs = FirmwareAttributes::new();
                    let mux_avail = attrs
                        .gpu_mux_mode()
                        .map(|a| a.base_path_exists())
                        .unwrap_or(false);

                    // helper to set attribute ignoring errors
                    if mux_avail {
                        match index {
                            0 => {
                                // Integrated
                                if let Some(attr) = attrs.dgpu_disable() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(1));
                                }
                                if let Some(attr) = attrs.gpu_mux_mode() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(1));
                                }
                            }
                            1 => {
                                // Ultimate
                                if let Some(attr) = attrs.dgpu_disable() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(0));
                                }
                                if let Some(attr) = attrs.gpu_mux_mode() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(0));
                                }
                            }
                            2 => {
                                // Dynamic
                                if let Some(attr) = attrs.dgpu_disable() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(0));
                                }
                                if let Some(attr) = attrs.gpu_mux_mode() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(1));
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match index {
                            0 => {
                                if let Some(attr) = attrs.dgpu_disable() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(1));
                                }
                            }
                            1 => {
                                if let Some(attr) = attrs.dgpu_disable() {
                                    let _ = attr.set_current_value(&AttrValue::Integer(0));
                                }
                            }
                            _ => {}
                        }
                    };

                    // After attempting write(s), refresh UI from attributes
                    let attrs2 = FirmwareAttributes::new();
                    let cur_dgpu = attrs2
                        .dgpu_disable()
                        .and_then(|a| a.current_value().ok())
                        .unwrap_or(AttrValue::Integer(0));
                    let cur_mux = attrs2
                        .gpu_mux_mode()
                        .and_then(|a| a.current_value().ok())
                        .unwrap_or(AttrValue::Integer(1));

                    let dgpu_disabled = matches!(cur_dgpu, AttrValue::Integer(v) if v == 1);
                    let new_index: i32 = if mux_avail {
                        match cur_mux {
                            AttrValue::Integer(0) => 1,
                            _ => match cur_dgpu {
                                AttrValue::Integer(1) => 0,
                                _ => 2,
                            },
                        }
                    } else {
                        match cur_dgpu {
                            AttrValue::Integer(1) => 0,
                            _ => 1,
                        }
                    };

                    if let Err(e) = handle_next.upgrade_in_event_loop(move |h| {
                        let g = h.global::<GPUPageData>();
                        g.set_dgpu_disabled(if dgpu_disabled { 1 } else { 0 });
                        g.set_gpu_mux_mode(new_index);
                    }) {
                        error!("setup_gpu callback: upgrade_in_event_loop: {e:?}");
                    }
                });
            });
        }) {
            error!("setup_gpu_page: upgrade_in_event_loop: {e:?}");
        }
    });
}
