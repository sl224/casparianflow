use casparian_protocol::types::SinkConfig;
use casparian_schema::SchemaStorage;
use casparian_state_store::{StateStore, StateStoreQueueSession};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;
use tracing::error;

pub struct SqliteExecutor {
    tx: SyncSender<SqliteCmd>,
}

pub struct SqliteContext {
    pub schema_storage: SchemaStorage,
    pub topic_map: HashMap<String, Vec<SinkConfig>>,
    pub topic_map_last_refresh: f64,
}

enum SqliteCmd {
    Run(Box<dyn FnOnce(&StateStore, &StateStoreQueueSession, &mut SqliteContext) + Send>),
}

impl SqliteExecutor {
    pub fn start(state_store: Arc<StateStore>) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(256);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let queue_session = match state_store.session_fast() {
                Ok(session) => session,
                Err(err) => {
                    let _ = ready_tx.send(Err(anyhow::anyhow!(
                        "Failed to open queue session: {}",
                        err
                    )));
                    return;
                }
            };
            let schema_storage = match state_store.schema_storage() {
                Ok(storage) => storage,
                Err(err) => {
                    let _ = ready_tx.send(Err(anyhow::anyhow!(
                        "Failed to initialize schema storage: {}",
                        err
                    )));
                    return;
                }
            };
            let context = SqliteContext {
                schema_storage,
                topic_map: HashMap::new(),
                topic_map_last_refresh: 0.0,
            };
            let _ = ready_tx.send(Ok(()));
            run_executor(state_store, queue_session, context, rx);
        });
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(err) => {
                return Err(anyhow::anyhow!(
                    "Sqlite executor initialization failed: {}",
                    err
                ))
            }
        }
        Ok(Self { tx })
    }

    pub fn call<R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&StateStore, &StateStoreQueueSession, &mut SqliteContext) -> anyhow::Result<R>
            + Send
            + 'static,
    {
        let response_rx = self.submit(f)?;
        response_rx.recv().map_err(|err| {
            anyhow::anyhow!("Sqlite executor response error: {}", err)
        })?
    }

    pub fn submit<R, F>(&self, f: F) -> anyhow::Result<Receiver<anyhow::Result<R>>>
    where
        R: Send + 'static,
        F: FnOnce(&StateStore, &StateStoreQueueSession, &mut SqliteContext) -> anyhow::Result<R>
            + Send
            + 'static,
    {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let cmd = SqliteCmd::Run(Box::new(move |state_store, queue, ctx| {
            let _ = response_tx.send(f(state_store, queue, ctx));
        }));
        self.tx
            .send(cmd)
            .map_err(|err| anyhow::anyhow!("Failed to send sqlite command: {}", err))?;
        Ok(response_rx)
    }

    pub fn execute<F>(&self, f: F) -> anyhow::Result<()>
    where
        F: FnOnce(&StateStore, &StateStoreQueueSession, &mut SqliteContext) -> anyhow::Result<()>
            + Send
            + 'static,
    {
        let cmd = SqliteCmd::Run(Box::new(move |state_store, queue, ctx| {
            if let Err(err) = f(state_store, queue, ctx) {
                error!("Sqlite executor task failed: {}", err);
            }
        }));
        self.tx
            .send(cmd)
            .map_err(|err| anyhow::anyhow!("Failed to send sqlite command: {}", err))?;
        Ok(())
    }
}

fn run_executor(
    state_store: Arc<StateStore>,
    queue_session: StateStoreQueueSession,
    mut context: SqliteContext,
    rx: Receiver<SqliteCmd>,
) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            SqliteCmd::Run(task) => {
                task(&state_store, &queue_session, &mut context);
            }
        }
    }
}
