//! Casparian Deck - Tauri Desktop Application
//!
//! Embeds the Sentinel and provides real-time system monitoring via Tauri events.

use casparian_sentinel::{Sentinel, SentinelConfig, METRICS};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tracing::{error, info};

/// System pulse event - emitted periodically with current metrics
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPulse {
    /// Number of connected workers (active - cleaned up)
    pub connected_workers: u64,
    /// Jobs completed in total
    pub jobs_completed: u64,
    /// Jobs failed in total
    pub jobs_failed: u64,
    /// Jobs dispatched in total
    pub jobs_dispatched: u64,
    /// Jobs currently in-flight (dispatched - completed - failed)
    pub jobs_in_flight: u64,
    /// Average dispatch latency in milliseconds
    pub avg_dispatch_ms: f64,
    /// Average conclude latency in milliseconds
    pub avg_conclude_ms: f64,
    /// Messages sent via ZMQ
    pub messages_sent: u64,
    /// Messages received via ZMQ
    pub messages_received: u64,
    /// Unix timestamp of this pulse
    pub timestamp: u64,
}

impl SystemPulse {
    /// Create from current metrics snapshot
    fn from_metrics() -> Self {
        let snapshot = METRICS.snapshot();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Calculate in-flight jobs (dispatched but not concluded)
        let concluded = snapshot.jobs_completed + snapshot.jobs_failed;
        let in_flight = snapshot.jobs_dispatched.saturating_sub(concluded);

        // Active workers = registered - cleaned up
        let active_workers = snapshot
            .workers_registered
            .saturating_sub(snapshot.workers_cleaned_up);

        SystemPulse {
            connected_workers: active_workers,
            jobs_completed: snapshot.jobs_completed,
            jobs_failed: snapshot.jobs_failed,
            jobs_dispatched: snapshot.jobs_dispatched,
            jobs_in_flight: in_flight,
            avg_dispatch_ms: snapshot.avg_dispatch_time_ms(),
            avg_conclude_ms: snapshot.avg_conclude_time_ms(),
            messages_sent: snapshot.messages_sent,
            messages_received: snapshot.messages_received,
            timestamp: now,
        }
    }
}

/// Sentinel state managed by Tauri
struct SentinelState {
    /// Signal to stop the sentinel (consumed on shutdown)
    #[allow(dead_code)]
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Flag indicating if sentinel is running
    running: Arc<AtomicBool>,
    /// The address the sentinel is bound to
    bind_addr: String,
}

/// Get current system metrics
#[tauri::command]
fn get_system_pulse() -> SystemPulse {
    SystemPulse::from_metrics()
}

/// Get metrics in Prometheus format
#[tauri::command]
fn get_prometheus_metrics() -> String {
    METRICS.prometheus_format()
}

/// Check if sentinel is running
#[tauri::command]
fn is_sentinel_running(state: tauri::State<'_, SentinelState>) -> bool {
    state.running.load(Ordering::Relaxed)
}

/// Get the sentinel bind address
#[tauri::command]
fn get_bind_address(state: tauri::State<'_, SentinelState>) -> String {
    state.bind_addr.clone()
}

/// Start the pulse emitter task
fn start_pulse_emitter(app: AppHandle, running: Arc<AtomicBool>) {
    // Spawn a task that emits system pulse events every 500ms
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("Failed to create pulse runtime");

        rt.block_on(async {
            let mut interval = tokio::time::interval(Duration::from_millis(500));

            while running.load(Ordering::Relaxed) {
                interval.tick().await;

                let pulse = SystemPulse::from_metrics();

                // Emit to all windows
                if let Err(e) = app.emit("system-pulse", &pulse) {
                    error!("Failed to emit system-pulse: {}", e);
                }
            }

            info!("Pulse emitter stopped");
        });
    });
}

/// Start the Sentinel on a background thread
fn start_sentinel(
    running: Arc<AtomicBool>,
    bind_addr: String,
    database_url: String,
) -> oneshot::Sender<()> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    std::thread::spawn(move || {
        // Create a dedicated Tokio runtime for the Sentinel
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("sentinel")
            .build()
            .expect("Failed to create Sentinel runtime");

        rt.block_on(async {
            running.store(true, Ordering::Relaxed);

            let config = SentinelConfig {
                bind_addr: bind_addr.clone(),
                database_url: database_url.clone(),
            };

            match Sentinel::bind(config).await {
                Ok(mut sentinel) => {
                    info!("Sentinel started on {}", bind_addr);

                    // Run sentinel until shutdown signal
                    tokio::select! {
                        result = sentinel.run() => {
                            match result {
                                Ok(_) => info!("Sentinel stopped normally"),
                                Err(e) => error!("Sentinel error: {}", e),
                            }
                        }
                        _ = shutdown_rx => {
                            info!("Shutdown signal received, stopping Sentinel");
                            sentinel.stop();
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to start Sentinel: {}", e);
                }
            }

            running.store(false, Ordering::Relaxed);
            info!("Sentinel runtime stopped");
        });
    });

    shutdown_tx
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("casparian=info".parse().unwrap())
                .add_directive("casparian_sentinel=info".parse().unwrap()),
        )
        .init();

    info!("Starting Casparian Deck");

    // Configuration (could be loaded from file/env in production)
    // Use Unix domain socket for local desktop app (faster, more secure)
    let bind_addr = std::env::var("CASPARIAN_BIND").unwrap_or_else(|_| {
        let socket_path = dirs::runtime_dir()
            .or_else(|| dirs::cache_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("casparian.sock");
        format!("ipc://{}", socket_path.display())
    });

    // Default to the database in the project root
    let database_url = std::env::var("CASPARIAN_DATABASE").unwrap_or_else(|_| {
        // Try to find database relative to executable or use absolute path
        let project_db = std::path::Path::new("/Users/shan/workspace/casparianflow/casparian_flow.sqlite3");
        if project_db.exists() {
            format!("sqlite://{}", project_db.display())
        } else {
            "sqlite://casparian_flow.sqlite3".to_string()
        }
    });

    // Shared running flag
    let running = Arc::new(AtomicBool::new(false));

    // Start the Sentinel
    let shutdown_tx = start_sentinel(running.clone(), bind_addr.clone(), database_url);

    // Create state
    let state = SentinelState {
        shutdown_tx: Some(shutdown_tx),
        running: running.clone(),
        bind_addr,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_system_pulse,
            get_prometheus_metrics,
            is_sentinel_running,
            get_bind_address,
        ])
        .setup(|app| {
            // Start the pulse emitter after app is ready
            let app_handle = app.handle().clone();
            start_pulse_emitter(app_handle, running);

            info!("Casparian Deck setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            // Handle window close - graceful shutdown
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("Window close requested, initiating graceful shutdown");

                // Get state and signal sentinel to stop
                let state = window.state::<SentinelState>();
                state.running.store(false, Ordering::Relaxed);

                // Let the window close naturally - no need to prevent and re-trigger
                // The sentinel will stop on its next loop iteration
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
