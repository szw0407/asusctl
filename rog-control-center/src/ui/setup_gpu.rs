use log::error;
use rog_dbus::asus_armoury::AsusArmouryProxy;
use rog_platform::asus_armoury::FirmwareAttribute;
use slint::{ComponentHandle, ModelRc, SharedString};

use crate::zbus_proxies::find_iface_async;
use crate::{GPUPageData, MainWindow};

fn gpu_index_from_values(gpu_mux_available: bool, current_dgpu: i32, current_mux: i32) -> i32 {
    if gpu_mux_available {
        match current_mux {
            0 => 1, // Ultimate
            _ => {
                if current_dgpu == 1 {
                    0 // Integrated
                } else {
                    2 // Hybrid
                }
            }
        }
    } else if current_dgpu == 1 {
        0
    } else {
        1
    }
}

async fn refresh_gpu_state(
    dgpu_attr: &Option<AsusArmouryProxy<'static>>,
    mux_attr: &Option<AsusArmouryProxy<'static>>,
) -> (bool, bool, i32) {
    let gpu_mux_available = mux_attr.is_some();
    let current_dgpu = match dgpu_attr {
        Some(attr) => attr.current_value().await.unwrap_or(0),
        None => 0,
    };
    let current_mux = match mux_attr {
        Some(attr) => attr.current_value().await.unwrap_or(1),
        None => 1,
    };

    (
        gpu_mux_available,
        current_dgpu == 1,
        gpu_index_from_values(gpu_mux_available, current_dgpu, current_mux),
    )
}

// Populate GPU page choices and wire the `cb_set_gpu_mode` callback
pub fn setup_gpu_page(ui: &MainWindow) {
    let handle = ui.as_weak();

    tokio::spawn(async move {
        let attrs = match find_iface_async::<AsusArmouryProxy>("xyz.ljones.AsusArmoury").await {
            Ok(attrs) => attrs,
            Err(e) => {
                error!("setup_gpu_page: failed to get AsusArmoury proxies: {e:?}");
                return;
            }
        };

        let mut dgpu_attr = None;
        let mut mux_attr = None;
        for attr in attrs {
            match attr.name().await {
                Ok(FirmwareAttribute::DgpuDisable) => dgpu_attr = Some(attr),
                Ok(FirmwareAttribute::GpuMuxMode) => mux_attr = Some(attr),
                Ok(_) => {}
                Err(e) => error!("setup_gpu_page: failed to read attribute name: {e:?}"),
            }
        }

        let (gpu_mux_available, dgpu_disabled, initial_index) =
            refresh_gpu_state(&dgpu_attr, &mux_attr).await;

        let mut choices: Vec<SharedString> = Vec::new();
        choices.push(SharedString::from("Integrated"));
        if gpu_mux_available {
            choices.push(SharedString::from("Ultimate"));
        }
        choices.push(SharedString::from("Hybrid"));

        let handle_copy = handle.clone();
        if let Err(e) = handle.upgrade_in_event_loop(move |handle| {
            let global = handle.global::<GPUPageData>();

            let model: ModelRc<SharedString> = choices.as_slice().into();
            global.set_gpu_modes_choises(model);
            global.set_gpu_mux_available(gpu_mux_available);
            global.set_dgpu_disabled(if dgpu_disabled { 1 } else { 0 });
            global.set_gpu_mux_mode(initial_index);

            // Disable dropdown initially if `dgpu_disable` attribute is not exposed by asusd
            global.set_gpu_dropdown_enabled(dgpu_attr.is_some());

            let handle_cb = handle_copy.clone();
            let dgpu_attr = dgpu_attr.clone();
            let mux_attr = mux_attr.clone();
            global.on_cb_set_gpu_mode(move |index: i32| {
                let toast_handle = handle_cb.clone();
                let dgpu_attr = dgpu_attr.clone();
                let mux_attr = mux_attr.clone();
                let handle_next = handle_cb.clone();

                // Disable dropdown immediately to prevent concurrent GPU switch commands
                if let Err(e) = handle_cb.upgrade_in_event_loop(|h| {
                    let g = h.global::<GPUPageData>();
                    g.set_gpu_dropdown_enabled(false);
                }) {
                    error!("setup_gpu callback: failed to disable dropdown: {e:?}");
                }

                tokio::spawn(async move {
                    let result = async {
                        match index {
                            0 => {
                                if let Some(attr) = &dgpu_attr {
                                    attr.set_current_value(1).await?;
                                }
                                if let Some(attr) = &mux_attr {
                                    attr.set_current_value(1).await?;
                                }
                            }
                            1 => {
                                if mux_attr.is_some() {
                                    if let Some(attr) = &dgpu_attr {
                                        attr.set_current_value(0).await?;
                                    }
                                    if let Some(attr) = &mux_attr {
                                        attr.set_current_value(0).await?;
                                    }
                                } else if let Some(attr) = &dgpu_attr {
                                    attr.set_current_value(0).await?;
                                }
                            }
                            2 => {
                                if let Some(attr) = &dgpu_attr {
                                    attr.set_current_value(0).await?;
                                }
                                if let Some(attr) = &mux_attr {
                                    attr.set_current_value(1).await?;
                                }
                            }
                            _ => {}
                        }

                        Ok::<(), zbus::Error>(())
                    }
                    .await;

                    crate::ui::show_toast(
                        SharedString::from(
                            "GPU mode change scheduled — reboot required for changes to apply.",
                        ),
                        SharedString::from("Failed to set GPU mode"),
                        toast_handle,
                        result,
                    );

                    let (_, dgpu_disabled, new_index) =
                        refresh_gpu_state(&dgpu_attr, &mux_attr).await;

                    if let Err(e) = handle_next.upgrade_in_event_loop(move |h| {
                        let g = h.global::<GPUPageData>();
                        g.set_dgpu_disabled(if dgpu_disabled { 1 } else { 0 });
                        g.set_gpu_mux_mode(new_index);
                    }) {
                        error!("setup_gpu callback: upgrade_in_event_loop: {e:?}");
                    }

                    // Re-enable dropdown after toast completes (5 seconds, matching show_toast timing)
                    let enable_handle = handle_next.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        if let Err(e) = enable_handle.upgrade_in_event_loop(|h| {
                            let g = h.global::<GPUPageData>();
                            g.set_gpu_dropdown_enabled(true);
                        }) {
                            error!("setup_gpu callback: failed to re-enable dropdown: {e:?}");
                        }
                    });
                });
            });
        }) {
            error!("setup_gpu_page: upgrade_in_event_loop: {e:?}");
        }
    });
}
