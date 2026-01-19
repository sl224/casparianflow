//! Publish command shared helper.

use anyhow::{Context, Result};

/// Publish a plugin to the Sentinel registry.
pub async fn run_publish(
    file: std::path::PathBuf,
    version: String,
    addr: Option<String>,
    publisher: Option<String>,
    email: Option<String>,
) -> Result<()> {
    use casparian_protocol::types::DeployCommand;
    use casparian_protocol::{Message, OpCode};
    use casparian_security::signing::sha256;
    use casparian_security::Gatekeeper;
    use zeromq::{Socket, SocketRecv, SocketSend, ZmqMessage};

    tracing::info!("Publishing plugin: {:?} v{}", file, version);

    // 1. Read plugin source code
    let source_code = std::fs::read_to_string(&file)
        .with_context(|| format!("Failed to read plugin file: {:?}", file))?;

    // 2. Validate with Gatekeeper (AST-based security checks)
    let gatekeeper = Gatekeeper::new();
    gatekeeper
        .validate(&source_code)
        .context("Plugin failed security validation")?;
    tracing::info!("✓ Security validation passed");

    // 3. Check for uv.lock, run `uv lock` if missing
    let plugin_dir = file
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Plugin file has no parent directory"))?;
    let lockfile_path = plugin_dir.join("uv.lock");

    if !lockfile_path.exists() {
        tracing::info!("No uv.lock found, running `uv lock` in {:?}...", plugin_dir);
        let output = std::process::Command::new("uv")
            .arg("lock")
            .current_dir(plugin_dir)
            .output()
            .context("Failed to run `uv lock` (is uv installed?)")?;

        if !output.status.success() {
            anyhow::bail!(
                "uv lock failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        tracing::info!("✓ Generated uv.lock");
    }

    let lockfile_content = std::fs::read_to_string(&lockfile_path)
        .context("Failed to read uv.lock after generation")?;

    // 4. Compute hashes
    let env_hash = sha256(lockfile_content.as_bytes());
    let artifact_content = format!("{}{}", source_code, lockfile_content);
    let artifact_hash = sha256(artifact_content.as_bytes());
    tracing::info!(
        "✓ Computed hashes (env: {}..., artifact: {}...)",
        &env_hash[..8],
        &artifact_hash[..8]
    );

    // 5. Extract plugin name from file
    let plugin_name = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Could not extract plugin name from file path"))?
        .to_string();

    // Get publisher name (default to system username)
    let publisher_name = publisher.unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    });

    // 6. Construct DeployCommand
    let deploy_cmd = DeployCommand {
        plugin_name: plugin_name.clone(),
        version: version.clone(),
        source_code,
        lockfile_content,
        env_hash,
        artifact_hash,
        publisher_name,
        publisher_email: email,
        azure_oid: None,
        system_requirements: None,
    };

    // 7. Send via ZMQ DEALER to Sentinel
    let sentinel_addr = addr.unwrap_or_else(crate::get_default_ipc_addr);
    tracing::info!("Connecting to Sentinel at {}", sentinel_addr);

    let mut socket = zeromq::DealerSocket::new();
    socket.connect(&sentinel_addr).await?;
    tracing::info!("✓ Connected to Sentinel");

    // Serialize payload
    let payload = serde_json::to_vec(&deploy_cmd)?;

    // Create protocol message
    let msg = Message::new(OpCode::Deploy, 0, payload)?;
    let (header_bytes, payload_bytes) = msg.pack()?;

    // Send message (multipart)
    let mut multipart = ZmqMessage::from(header_bytes);
    multipart.push_back(payload_bytes.into());
    socket.send(multipart).await?;
    tracing::info!("✓ Sent deployment request");

    // 8. Await ACK/ERR response
    let response_frames: ZmqMessage = socket.recv().await?;
    let response_msg = Message::unpack(
        &response_frames
            .into_vec()
            .iter()
            .map(|f| f.to_vec())
            .collect::<Vec<_>>(),
    )?;

    match response_msg.header.opcode {
        OpCode::Ack => {
            use casparian_protocol::types::DeployResponse;
            let deploy_response: DeployResponse = serde_json::from_slice(&response_msg.payload)?;

            if deploy_response.success {
                println!("✅ Deployed plugin '{}' v{}", plugin_name, version);
                Ok(())
            } else {
                anyhow::bail!("Deployment failed: {}", deploy_response.message)
            }
        }
        OpCode::Err => {
            use casparian_protocol::types::ErrorPayload;
            let error_payload: ErrorPayload = serde_json::from_slice(&response_msg.payload)?;
            anyhow::bail!("Deployment error: {}", error_payload.message)
        }
        _ => anyhow::bail!(
            "Unexpected response opcode: {:?}",
            response_msg.header.opcode
        ),
    }
}
