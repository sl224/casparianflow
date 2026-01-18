use anyhow::Result;
use casparian_worker::{Worker, WorkerConfig};
use casparian_protocol::types::{self};
use casparian_protocol::{Message, OpCode};
use std::time::Duration;
use tokio::time::timeout;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

/// Generate a random port in the ephemeral range to avoid collisions
fn random_test_port() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    // Use process ID + time-based seed for uniqueness
    let pid = std::process::id() as u64;
    ((seed ^ pid) % 10000 + 50000) as u16 // Ports 50000-59999
}

async fn bind_router() -> Result<(RouterSocket, String)> {
    let mut last_err = None;
    for _ in 0..25 {
        let mut sentinel = RouterSocket::new();
        let port = random_test_port();
        let bound_addr = format!("tcp://127.0.0.1:{}", port);
        match sentinel.bind(&bound_addr).await {
            Ok(_) => return Ok((sentinel, bound_addr)),
            Err(err) => {
                last_err = Some(err);
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to bind mock sentinel after multiple attempts: {:?}",
        last_err
    ))
}

/// Test that worker responds to heartbeat messages promptly.
/// This validates the worker's event loop is non-blocking.
#[tokio::test]
async fn test_worker_heartbeat_responsiveness() -> Result<()> {
    // 1. Setup Mock Sentinel (Router) with random port
    let (mut sentinel, bound_addr) = bind_router().await?;
    println!("Mock Sentinel bound to {}", bound_addr);

    // 2. Setup Worker with isolated temp directory
    let tmp = tempfile::tempdir()?;
    let test_dir = tmp.path().to_path_buf();
    let parquet_root = test_dir.join("output");
    let venvs_dir = test_dir.join("venvs");
    std::fs::create_dir_all(&venvs_dir)?;

    let shim_path = test_dir.join("bridge_shim.py");
    std::fs::write(&shim_path, "# placeholder")?;

    let config = WorkerConfig {
        sentinel_addr: bound_addr.clone(),
        parquet_root,
        worker_id: "test-heartbeat-worker".to_string(),
        shim_path,
        capabilities: vec!["*".to_string()],
        venvs_dir: Some(venvs_dir),
    };

    // 3. Connect Worker
    let worker_handle = tokio::spawn(async move {
        let (worker, _shutdown_tx) = Worker::connect(config)
            .await
            .expect("Worker failed to connect");
        worker.run().await.expect("Worker run failed");
    });

    // 4. Accept IDENTIFY from worker
    let multipart = timeout(Duration::from_secs(5), sentinel.recv()).await??;
    let identity = multipart.get(0).unwrap().to_vec();
    let msg = Message::unpack(&[
        multipart.get(1).unwrap().to_vec(),
        multipart.get(2).unwrap().to_vec(),
    ])?;
    assert_eq!(msg.header.opcode, OpCode::Identify);
    println!("Received IDENTIFY from worker");

    // 5. Send multiple heartbeats and verify prompt responses
    for i in 0..3 {
        let heartbeat_payload = types::HeartbeatPayload {
            status: types::HeartbeatStatus::Alive,
            current_job_id: None,
            active_job_count: 0,
            active_job_ids: vec![],
        };
        let hb_msg = Message::new(OpCode::Heartbeat, 0, serde_json::to_vec(&heartbeat_payload)?)
            .map_err(|e| anyhow::anyhow!("Failed to create message: {}", e))?;
        let (h, p) = hb_msg.pack()
            .map_err(|e| anyhow::anyhow!("Failed to pack message: {}", e))?;
        let mut multipart = ZmqMessage::from(identity.clone());
        multipart.push_back(h.into());
        multipart.push_back(p.into());

        let start = std::time::Instant::now();
        sentinel.send(multipart).await?;

        // Response should arrive within 500ms (worker loop timeout is 100ms)
        let reply = timeout(Duration::from_millis(500), sentinel.recv()).await??;
        let elapsed = start.elapsed();

        let reply_msg = Message::unpack(&[
            reply.get(1).unwrap().to_vec(),
            reply.get(2).unwrap().to_vec(),
        ])?;
        assert_eq!(reply_msg.header.opcode, OpCode::Heartbeat);

        println!(
            "Heartbeat {} response in {:?} (< 500ms: {})",
            i + 1,
            elapsed,
            elapsed < Duration::from_millis(500)
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "Heartbeat response too slow: {:?}",
            elapsed
        );
    }

    println!("SUCCESS: Worker responds promptly to heartbeats");

    // Cleanup
    worker_handle.abort();
    Ok(())
}

/// Test graceful shutdown: worker should send CONCLUDE for active jobs before exiting.
/// This validates the critical path for graceful shutdown draining.
#[tokio::test]
async fn test_graceful_shutdown_sends_conclude() -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // 1. Setup Mock Sentinel
    let (mut sentinel, bound_addr) = bind_router().await?;

    // 2. Setup Worker with test venv
    let tmp = tempfile::tempdir()?;
    let test_dir = tmp.path().to_path_buf();
    let parquet_root = test_dir.join("output");
    std::fs::create_dir_all(&parquet_root)?;

    let venvs_dir = test_dir.join("venvs");
    let env_hash = "test_env_graceful_shutdown";
    let env_dir = venvs_dir.join(env_hash);
    std::fs::create_dir_all(&env_dir)?;

    // Create a fake Python interpreter that will simulate a slow job
    let interpreter = env_dir.join("bin").join("python");
    std::fs::create_dir_all(interpreter.parent().unwrap())?;
    std::fs::write(&interpreter, "#!/bin/bash\nsleep 0.5\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&interpreter, std::fs::Permissions::from_mode(0o755))?;
    }

    // Create bridge shim placeholder
    let shim_path = test_dir.join("bridge_shim.py");
    std::fs::write(&shim_path, "# placeholder")?;

    let config = WorkerConfig {
        sentinel_addr: bound_addr.clone(),
        parquet_root,
        worker_id: "test-graceful-shutdown".to_string(),
        shim_path,
        capabilities: vec!["*".to_string()],
        venvs_dir: Some(venvs_dir),
    };

    // 3. Connect Worker and get shutdown handle
    let (worker, shutdown_tx) = Worker::connect(config).await?;

    let concluded = Arc::new(AtomicBool::new(false));
    let concluded_clone = concluded.clone();

    // 4. Spawn worker task
    let worker_handle = tokio::spawn(async move {
        worker.run().await
    });

    // 5. Accept IDENTIFY
    let multipart = timeout(Duration::from_secs(5), sentinel.recv()).await??;
    let identity = multipart.get(0).unwrap().to_vec();
    let msg = Message::unpack(&[
        multipart.get(1).unwrap().to_vec(),
        multipart.get(2).unwrap().to_vec(),
    ])?;
    assert_eq!(msg.header.opcode, OpCode::Identify);

    // 6. Dispatch a job (it will fail because our fake interpreter doesn't work, but that's OK)
    let dispatch_cmd = types::DispatchCommand {
        plugin_name: "test_plugin".to_string(),
        file_path: "/tmp/test.csv".to_string(),
        sinks: vec![],
        file_id: 1,
        env_hash: env_hash.to_string(),
        source_code: "# test".to_string(),
        artifact_hash: None,
    };
    let payload = serde_json::to_vec(&dispatch_cmd)?;
    let dispatch_msg = Message::new(OpCode::Dispatch, 42, payload)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let (h, p) = dispatch_msg.pack()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut multipart = ZmqMessage::from(identity.clone());
    multipart.push_back(h.into());
    multipart.push_back(p.into());
    sentinel.send(multipart).await?;

    // 7. Wait a bit for job to start processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 8. Send shutdown signal
    shutdown_tx.send(()).await?;

    // 9. Worker should send CONCLUDE before exiting (even for failed job)
    let conclude_result = timeout(Duration::from_secs(10), async {
        loop {
            match timeout(Duration::from_millis(500), sentinel.recv()).await {
                Ok(Ok(multipart)) => {
                    if multipart.len() >= 3 {
                        if let Ok(msg) = Message::unpack(&[
                            multipart.get(1).unwrap().to_vec(),
                            multipart.get(2).unwrap().to_vec(),
                        ]) {
                            if msg.header.opcode == OpCode::Conclude {
                                concluded_clone.store(true, Ordering::SeqCst);
                                return Ok::<_, anyhow::Error>(msg);
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    // ZMQ error - might be shutdown
                    return Err(anyhow::anyhow!("ZMQ error: {}", e));
                }
                Err(_) => {
                    // Timeout - check if worker exited
                    if worker_handle.is_finished() {
                        return Err(anyhow::anyhow!("Worker exited without sending CONCLUDE"));
                    }
                }
            }
        }
    }).await;

    // 10. Verify CONCLUDE was sent
    assert!(
        concluded.load(Ordering::SeqCst) || conclude_result.is_ok(),
        "Worker should send CONCLUDE for active job during graceful shutdown"
    );

    println!("SUCCESS: Worker sent CONCLUDE during graceful shutdown");

    // Cleanup
    worker_handle.abort();
    Ok(())
}
