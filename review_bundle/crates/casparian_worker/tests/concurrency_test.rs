use anyhow::{anyhow, Result};
use casparian_protocol::types::{self};
use casparian_protocol::{JobId, Message, OpCode};
use casparian_worker::{Worker, WorkerConfig};
use std::time::{Duration, Instant};
use zmq::Context;

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

fn bind_router(context: &Context) -> Result<(zmq::Socket, String)> {
    let mut last_err = None;
    for _ in 0..25 {
        let sentinel = context.socket(zmq::ROUTER)?;
        let port = random_test_port();
        let bound_addr = format!("tcp://127.0.0.1:{}", port);
        match sentinel.bind(&bound_addr) {
            Ok(_) => return Ok((sentinel, bound_addr)),
            Err(err) => {
                last_err = Some(err);
            }
        }
    }

    Err(anyhow!(
        "Failed to bind mock sentinel after multiple attempts: {:?}",
        last_err
    ))
}

fn recv_multipart_with_timeout(
    socket: &zmq::Socket,
    timeout: Duration,
) -> Result<Option<Vec<Vec<u8>>>> {
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    socket.set_rcvtimeo(timeout_ms)?;
    match socket.recv_multipart(0) {
        Ok(parts) => Ok(Some(parts)),
        Err(zmq::Error::EAGAIN) => Ok(None),
        Err(err) => Err(anyhow!("ZMQ error: {}", err)),
    }
}

/// Test that worker responds to heartbeat messages promptly.
/// This validates the worker's event loop is non-blocking.
#[test]
fn test_worker_heartbeat_responsiveness() -> Result<()> {
    // 1. Setup Mock Sentinel (Router) with random port
    let context = Context::new();
    let (sentinel, bound_addr) = bind_router(&context)?;
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

    let (worker, worker_handle) = Worker::connect(config).expect("Worker failed to connect");
    let worker_thread = std::thread::spawn(move || {
        worker.run().expect("Worker run failed");
    });

    // 4. Accept IDENTIFY from worker
    let multipart = recv_multipart_with_timeout(&sentinel, Duration::from_secs(5))?
        .ok_or_else(|| anyhow!("Timeout waiting for IDENTIFY"))?;
    let identity = multipart.get(0).unwrap().to_vec();
    let mut cursor = 1;
    if multipart.get(1).map(|p| p.is_empty()).unwrap_or(false) {
        cursor += 1;
    }
    let msg = Message::unpack(&[
        multipart.get(cursor).unwrap().to_vec(),
        multipart.get(cursor + 1).unwrap().to_vec(),
    ])?;
    assert_eq!(msg.header.opcode, OpCode::Identify);
    println!("Received IDENTIFY from worker");

    // 5. Send multiple heartbeats and verify prompt responses
    for i in 0..3 {
        let heartbeat_payload = types::HeartbeatPayload {
            status: types::HeartbeatStatus::Alive,
            active_job_count: 0,
            active_job_ids: vec![],
        };
        let hb_msg = Message::new(
            OpCode::Heartbeat,
            JobId::new(0),
            serde_json::to_vec(&heartbeat_payload)?,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create message: {}", e))?;
        let (h, p) = hb_msg
            .pack()
            .map_err(|e| anyhow::anyhow!("Failed to pack message: {}", e))?;
        let frames = vec![identity.clone(), h.to_vec(), p.to_vec()];

        let start = Instant::now();
        sentinel.send_multipart(&frames, 0)?;

        // Response should arrive within 500ms (worker loop timeout is 100ms)
        let reply = recv_multipart_with_timeout(&sentinel, Duration::from_millis(500))?
            .ok_or_else(|| anyhow!("Timeout waiting for heartbeat response"))?;
        let elapsed = start.elapsed();

        let mut reply_cursor = 1;
        if reply.get(1).map(|p| p.is_empty()).unwrap_or(false) {
            reply_cursor += 1;
        }
        let reply_msg = Message::unpack(&[
            reply.get(reply_cursor).unwrap().to_vec(),
            reply.get(reply_cursor + 1).unwrap().to_vec(),
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
    let _ = worker_handle.shutdown();
    let _ = worker_thread.join();
    Ok(())
}

/// Test graceful shutdown: worker should send CONCLUDE for active jobs before exiting.
/// This validates the critical path for graceful shutdown draining.
#[test]
fn test_graceful_shutdown_sends_conclude() -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // 1. Setup Mock Sentinel
    let context = Context::new();
    let (sentinel, bound_addr) = bind_router(&context)?;

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
    let (worker, shutdown_handle) = Worker::connect(config)?;

    let concluded = Arc::new(AtomicBool::new(false));
    let concluded_clone = concluded.clone();

    // 4. Spawn worker task
    let worker_thread = std::thread::spawn(move || worker.run().expect("Worker run failed"));

    // 5. Accept IDENTIFY
    let multipart = recv_multipart_with_timeout(&sentinel, Duration::from_secs(5))?
        .ok_or_else(|| anyhow!("Timeout waiting for IDENTIFY"))?;
    let identity = multipart.get(0).unwrap().to_vec();
    let mut cursor = 1;
    if multipart.get(1).map(|p| p.is_empty()).unwrap_or(false) {
        cursor += 1;
    }
    let msg = Message::unpack(&[
        multipart.get(cursor).unwrap().to_vec(),
        multipart.get(cursor + 1).unwrap().to_vec(),
    ])?;
    assert_eq!(msg.header.opcode, OpCode::Identify);

    // 6. Dispatch a job (it will fail because our fake interpreter doesn't work, but that's OK)
    let dispatch_cmd = types::DispatchCommand {
        plugin_name: "test_plugin".to_string(),
        parser_version: Some("1.0.0".to_string()),
        file_path: "/tmp/test.csv".to_string(),
        sinks: vec![],
        file_id: 1,
        runtime_kind: types::RuntimeKind::PythonShim,
        entrypoint: "test_plugin.py:Handler".to_string(),
        platform_os: None,
        platform_arch: None,
        signature_verified: false,
        signer_id: None,
        env_hash: Some(env_hash.to_string()),
        source_code: Some("# test".to_string()),
        artifact_hash: "artifact_hash_test".to_string(),
    };
    let payload = serde_json::to_vec(&dispatch_cmd)?;
    let dispatch_msg = Message::new(OpCode::Dispatch, JobId::new(42), payload)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let (h, p) = dispatch_msg.pack().map_err(|e| anyhow::anyhow!("{}", e))?;

    let frames = vec![identity.clone(), h.to_vec(), p.to_vec()];
    sentinel.send_multipart(&frames, 0)?;

    // 7. Wait a bit for job to start processing
    std::thread::sleep(Duration::from_millis(100));

    // 8. Send shutdown signal
    shutdown_handle.shutdown_gracefully(Duration::from_secs(5))?;

    // 9. Worker should send CONCLUDE before exiting (even for failed job)
    let start = Instant::now();
    let mut conclude_result = None;
    while start.elapsed() < Duration::from_secs(10) {
        match recv_multipart_with_timeout(&sentinel, Duration::from_millis(500))? {
            Some(multipart) => {
                if multipart.len() >= 3 {
                    let mut cursor = 1;
                    if multipart.get(1).map(|p| p.is_empty()).unwrap_or(false) {
                        cursor += 1;
                    }
                    if let Ok(msg) = Message::unpack(&[
                        multipart.get(cursor).unwrap().to_vec(),
                        multipart.get(cursor + 1).unwrap().to_vec(),
                    ]) {
                        if msg.header.opcode == OpCode::Conclude {
                            concluded_clone.store(true, Ordering::SeqCst);
                            conclude_result = Some(msg);
                            break;
                        }
                    }
                }
            }
            None => {
                continue;
            }
        }
    }

    // 10. Verify CONCLUDE was sent
    assert!(
        concluded.load(Ordering::SeqCst) || conclude_result.is_some(),
        "Worker should send CONCLUDE for active job during graceful shutdown"
    );

    println!("SUCCESS: Worker sent CONCLUDE during graceful shutdown");

    // Cleanup
    let _ = worker_thread.join();
    Ok(())
}
