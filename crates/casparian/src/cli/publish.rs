//! Publish command shared helper.

use anyhow::Result;
use crate::publish::prepare_publish;
use casparian_protocol::types::DeployCommand;
use casparian_protocol::{JobId, Message, OpCode};
use zmq::Context;

/// Publish a plugin to the Sentinel registry.
pub fn run_publish(
    file: std::path::PathBuf,
    version: String,
    addr: Option<String>,
    publisher: Option<String>,
    email: Option<String>,
) -> Result<()> {
    tracing::info!("Publishing plugin: {:?} v{}", file, version);

    let artifact = prepare_publish(&file)?;

    if artifact.manifest.version != version {
        anyhow::bail!(
            "Version mismatch: CLI version '{}' does not match manifest version '{}'",
            version,
            artifact.manifest.version
        );
    }

    let plugin_name = artifact.plugin_name.clone();

    // Get publisher name (default to system username)
    let publisher_name = publisher.unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    });

    // Construct DeployCommand
    let deploy_cmd = DeployCommand {
        plugin_name: plugin_name.clone(),
        version: version.clone(),
        source_code: artifact.source_code,
        lockfile_content: artifact.lockfile_content,
        env_hash: artifact.env_hash,
        artifact_hash: artifact.artifact_hash,
        manifest_json: artifact.manifest_json,
        protocol_version: artifact.manifest.protocol_version,
        schema_artifacts_json: artifact.schema_artifacts_json,
        publisher_name,
        publisher_email: email,
        azure_oid: None,
        system_requirements: None,
    };

    // 7. Send via ZMQ DEALER to Sentinel
    let sentinel_addr = addr.unwrap_or_else(crate::get_default_ipc_addr);
    tracing::info!("Connecting to Sentinel at {}", sentinel_addr);

    let context = Context::new();
    let socket = context.socket(zmq::DEALER)?;
    socket.connect(&sentinel_addr)?;
    tracing::info!("✓ Connected to Sentinel");

    // Serialize payload
    let payload = serde_json::to_vec(&deploy_cmd)?;

    // Create protocol message
    let msg = Message::new(OpCode::Deploy, JobId::new(0), payload)?;
    let (header_bytes, payload_bytes) = msg.pack()?;

    // Send message (multipart)
    socket.send_multipart(vec![header_bytes, payload_bytes], 0)?;
    tracing::info!("✓ Sent deployment request");

    // 8. Await ACK/ERR response
    let response_frames = socket.recv_multipart(0)?;
    let mut start = 0;
    while start < response_frames.len() && response_frames[start].is_empty() {
        start += 1;
    }
    let response_msg = Message::unpack(&response_frames[start..])?;

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
