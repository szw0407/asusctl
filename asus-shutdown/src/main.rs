use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use log::{debug, error, info, warn};
use logind_zbus::manager::{InhibitType, ManagerProxy};
use rog_dbus::asus_armoury::AsusArmouryProxy;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedFd;
use zbus::Connection;

const SERVICE_NAME: &str = "asus-shutdown";
const SHUTDOWN_REASON: &str = "defer risky ASUS GPU firmware writes until shutdown";
const WAIT_FOR_GPU_IDLE: Duration = Duration::from_secs(8);
const GPU_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const ASUSD_BUS_NAME: &str = "xyz.ljones.Asusd";
const ASUSD_ARMOURY_IFACE: &str = "xyz.ljones.AsusArmoury";

#[derive(Clone, Debug)]
struct PendingAction {
    path: String,
    name: String,
    value: i32,
}

#[derive(Debug)]
struct DiscreteGpu {
    card: String,
    runtime_status: Option<String>,
    devnodes: Vec<PathBuf>,
    busy: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut logger = env_logger::Builder::new();
    logger
        .parse_default_env()
        .target(env_logger::Target::Stdout)
        .format_timestamp(None)
        .filter_level(log::LevelFilter::Info)
        .init();

    let is_service = match env::var_os("IS_SERVICE") {
        Some(val) => val == "1",
        None => true,
    };

    if !is_service {
        print_dry_run_actions().await;
        return Ok(());
    }

    info!("Starting {}", SERVICE_NAME);

    let connection = Connection::system().await?;
    let manager = ManagerProxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;

    let inhibitor = Arc::new(Mutex::new(Some(
        acquire_shutdown_inhibitor(&manager).await?,
    )));
    let mut shutdown_events = manager.receive_prepare_for_shutdown().await?;

    while let Some(event) = shutdown_events.next().await {
        match event.args() {
            Ok(args) if args.start => {
                info!("Shutdown requested, applying deferred ASUS GPU settings");
                if let Err(err) = apply_shutdown_settings().await {
                    error!("Failed to apply deferred GPU settings: {err}");
                }
                inhibitor.lock().await.take();
                info!("Released shutdown delay inhibitor");
            }
            Ok(args) => {
                debug!("PrepareForShutdown({})", args.start);
                let mut guard = inhibitor.lock().await;
                if guard.is_none() {
                    match acquire_shutdown_inhibitor(&manager).await {
                        Ok(fd) => {
                            *guard = Some(fd);
                            info!("Reacquired shutdown delay inhibitor");
                        }
                        Err(err) => {
                            error!("Failed to reacquire shutdown inhibitor: {err}");
                        }
                    }
                }
            }
            Err(err) => warn!("Failed to decode PrepareForShutdown signal: {err}"),
        }
    }

    Ok(())
}

async fn acquire_shutdown_inhibitor(manager: &ManagerProxy<'_>) -> Result<OwnedFd, zbus::Error> {
    manager
        .inhibit(
            InhibitType::Shutdown,
            SERVICE_NAME,
            SHUTDOWN_REASON,
            "delay",
        )
        .await
}

async fn print_dry_run_actions() {
    println!("asus-shutdown dry-run mode (manual start)");
    println!("Planned shutdown actions from asusd queue:");

    match fetch_pending_actions().await {
        Ok(actions) if actions.is_empty() => {
            println!("- none");
        }
        Ok(actions) => {
            for action in actions {
                println!(
                    "- {} => {} (path: {})",
                    action.name, action.value, action.path
                );
            }
        }
        Err(err) => {
            println!("- could not query asusd queue: {err}");
        }
    }
}

async fn fetch_pending_actions() -> Result<Vec<PendingAction>, Box<dyn std::error::Error>> {
    let conn = Connection::system().await?;
    let manager = zbus::fdo::ObjectManagerProxy::new(&conn, ASUSD_BUS_NAME, "/").await?;
    let managed = manager.get_managed_objects().await?;

    let mut actions = Vec::new();
    for (path, ifaces) in managed {
        if !ifaces.contains_key(ASUSD_ARMOURY_IFACE) {
            continue;
        }

        let proxy = AsusArmouryProxy::builder(&conn)
            .path(path.clone())?
            .destination(ASUSD_BUS_NAME)?
            .build()
            .await?;

        let value = proxy.queued_gpu_value().await?;
        if value < 0 {
            continue;
        }

        let name = proxy.name().await?;
        actions.push(PendingAction {
            path: path.to_string(),
            name: <&str>::from(name).to_owned(),
            value,
        });
    }

    actions.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(actions)
}

async fn apply_shutdown_settings() -> Result<(), Box<dyn std::error::Error>> {
    let queued = fetch_pending_actions().await?;
    if queued.is_empty() {
        info!("No deferred GPU settings queued");
        return Ok(());
    }

    info!("Deferred GPU settings queued for shutdown apply:");
    for action in &queued {
        info!("  {} => {} ({})", action.name, action.value, action.path);
    }

    info!("Witiging for discrete GPU to become idle before applying settings...");
    wait_for_discrete_gpu_idle().await;
    info!("Proceeding with applying deferred GPU settings");

    let conn = Connection::system().await?;
    for action in queued {
        let proxy = AsusArmouryProxy::builder(&conn)
            .path(action.path.as_str())?
            .destination(ASUSD_BUS_NAME)?
            .build()
            .await?;

        info!(
            "Applying deferred GPU attribute {} = {}",
            action.name, action.value
        );
        let applied = proxy.apply_queued_gpu_value().await?;
        if !applied {
            warn!("No queued value remained for {}", action.name);
        }
    }

    Ok(())
}

async fn wait_for_discrete_gpu_idle() {
    let deadline = Instant::now() + WAIT_FOR_GPU_IDLE;

    loop {
        match collect_discrete_gpu_state() {
            Ok(gpus) if gpus.is_empty() => {
                info!("No discrete DRM devices detected, continuing shutdown apply");
                return;
            }
            Ok(gpus) => {
                if gpus.iter().all(discrete_gpu_is_idle) {
                    info!("Discrete GPU nodes are idle");
                    return;
                }

                if Instant::now() >= deadline {
                    warn!("Timed out waiting for discrete GPU users to drain");
                    for gpu in &gpus {
                        if gpu.busy {
                            warn!(
                                "GPU {} still busy with runtime_status={:?} nodes={:?}",
                                gpu.card, gpu.runtime_status, gpu.devnodes
                            );
                        }
                    }
                    return;
                }
            }
            Err(err) => {
                warn!("Failed to inspect GPU state: {err}");
                return;
            }
        }

        sleep(GPU_IDLE_POLL_INTERVAL).await;
    }
}

fn discrete_gpu_is_idle(gpu: &DiscreteGpu) -> bool {
    // Only check /proc file descriptors to determine if GPU is in use.
    // Runtime power state (sysfs power/runtime_status) is unavailable on most
    // laptops, so we don't rely on it. If a process holds an open fd to the GPU,
    // it is definitely using it.
    !gpu.busy
}

fn collect_discrete_gpu_state() -> Result<Vec<DiscreteGpu>, Box<dyn std::error::Error>> {
    let render_map = render_node_map()?;
    let busy_nodes = busy_gpu_nodes()?;
    let mut gpus = Vec::new();

    for entry in fs::read_dir("/sys/class/drm")? {
        let entry = entry?;
        let file_name = entry.file_name();
        let card = file_name.to_string_lossy();
        if !is_card_entry(&card) {
            continue;
        }

        let device_path = entry.path().join("device");
        if read_trimmed(device_path.join("boot_vga")).as_deref() == Some("1") {
            continue;
        }

        let runtime_status = read_trimmed(device_path.join("power/runtime_status"));
        let mut devnodes = vec![PathBuf::from(format!("/dev/dri/{card}"))];

        let canonical_device = match fs::canonicalize(&device_path) {
            Ok(path) => path,
            Err(err) => {
                debug!(
                    "Skipping DRM entry {} without canonical device path: {err}",
                    card
                );
                continue;
            }
        };

        if let Some(render_nodes) = render_map.get(&canonical_device) {
            devnodes.extend(render_nodes.iter().cloned());
        }

        let busy = devnodes.iter().any(|node| busy_nodes.contains(node));
        gpus.push(DiscreteGpu {
            card: card.into_owned(),
            runtime_status,
            devnodes,
            busy,
        });
    }

    Ok(gpus)
}

fn render_node_map() -> Result<HashMap<PathBuf, Vec<PathBuf>>, Box<dyn std::error::Error>> {
    let mut render_map = HashMap::new();

    for entry in fs::read_dir("/sys/class/drm")? {
        let entry = entry?;
        let file_name = entry.file_name();
        let render = file_name.to_string_lossy();
        if !render.starts_with("renderD") {
            continue;
        }

        let device_path = match fs::canonicalize(entry.path().join("device")) {
            Ok(path) => path,
            Err(_) => continue,
        };

        render_map
            .entry(device_path)
            .or_insert_with(Vec::new)
            .push(PathBuf::from(format!("/dev/dri/{render}")));
    }

    Ok(render_map)
}

fn busy_gpu_nodes() -> Result<HashSet<PathBuf>, Box<dyn std::error::Error>> {
    let mut nodes = HashSet::new();

    for proc_entry in fs::read_dir("/proc")? {
        let proc_entry = proc_entry?;
        let pid = proc_entry.file_name();
        if !pid.to_string_lossy().chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }

        let fd_dir = proc_entry.path().join("fd");
        let Ok(fd_entries) = fs::read_dir(fd_dir) else {
            continue;
        };

        for fd_entry in fd_entries.flatten() {
            let Ok(target) = fs::read_link(fd_entry.path()) else {
                continue;
            };

            if target.starts_with("/dev/dri/") {
                nodes.insert(target);
            }
        }
    }

    Ok(nodes)
}

fn is_card_entry(name: &str) -> bool {
    name.starts_with("card") && name[4..].chars().all(|ch| ch.is_ascii_digit())
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
}
