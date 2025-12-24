use anyhow::Result;
use casparian_worker::{Worker, WorkerConfig};
use cf_protocol::types::{self};
use cf_protocol::{Message, OpCode};
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

/// Test that worker responds to heartbeat messages promptly.
/// This validates the worker's event loop is non-blocking.
#[tokio::test]
async fn test_worker_heartbeat_responsiveness() -> Result<()> {
    // 1. Setup Mock Sentinel (Router) with random port
    let mut sentinel = RouterSocket::new();
    let port = random_test_port();
    let bound_addr = format!("tcp://127.0.0.1:{}", port);
    sentinel.bind(&bound_addr).await?;
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
            status: "CHECK".to_string(),
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
