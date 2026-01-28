use casparian_db::DbConnection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct CatalogIntent {
    pub view_name: String,
    pub parquet_glob: PathBuf,
}

pub struct CatalogExecutor {
    tx: Sender<CatalogIntent>,
}

impl CatalogExecutor {
    pub fn start(query_catalog_path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || run_catalog_thread(query_catalog_path, rx));
        Self { tx }
    }

    pub fn submit<I>(&self, intents: I)
    where
        I: IntoIterator<Item = CatalogIntent>,
    {
        for intent in intents {
            let _ = self.tx.send(intent);
        }
    }
}

fn run_catalog_thread(query_catalog_path: PathBuf, rx: Receiver<CatalogIntent>) {
    let mut pending: HashMap<String, PathBuf> = HashMap::new();
    let mut last_flush = Instant::now();
    let flush_interval = Duration::from_millis(500);

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(intent) => {
                pending.insert(intent.view_name, intent.parquet_glob);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if pending.is_empty() || last_flush.elapsed() < flush_interval {
            continue;
        }

        if let Err(err) = apply_updates(&query_catalog_path, &pending) {
            warn!("Catalog update failed: {}", err);
        } else {
            info!("Catalog updated with {} views", pending.len());
            pending.clear();
        }

        last_flush = Instant::now();
    }
}

fn apply_updates(query_catalog_path: &PathBuf, views: &HashMap<String, PathBuf>) -> anyhow::Result<()> {
    if let Some(parent) = query_catalog_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut has_quarantine = false;
    for view_name in views.keys() {
        if view_name.starts_with("quarantine.") {
            has_quarantine = true;
            break;
        }
    }

    let catalog_conn = DbConnection::open_duckdb(query_catalog_path)?;
    catalog_conn.execute("CREATE SCHEMA IF NOT EXISTS outputs", &[])?;
    if has_quarantine {
        catalog_conn.execute("CREATE SCHEMA IF NOT EXISTS quarantine", &[])?;
    }

    for (view_name, pattern) in views {
        let path_literal = escape_sql_literal(&pattern.to_string_lossy());
        let sql = format!(
            "CREATE OR REPLACE VIEW {} AS SELECT * FROM parquet_scan('{}')",
            view_name, path_literal
        );
        catalog_conn.execute(&sql, &[])?;
    }

    Ok(())
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}
