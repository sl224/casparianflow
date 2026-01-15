//! Container lifecycle management: start, stop, health checks.

use crate::config::DbVersion;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};
use tracing::{debug, info};

/// Default timeout for container startup
const CONTAINER_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// Default interval between health check attempts
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// Get the path to the docker-compose.yml file.
pub fn docker_compose_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("docker").join("docker-compose.yml")
}

/// Check if Docker is available on the system.
pub fn is_docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if docker-compose is available on the system.
pub fn is_docker_compose_available() -> bool {
    // Try docker compose (v2) first
    let v2 = Command::new("docker")
        .args(["compose", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if v2 {
        return true;
    }

    // Fall back to docker-compose (v1)
    Command::new("docker-compose")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a docker-compose command.
fn docker_compose_cmd(args: &[&str]) -> Result<std::process::Output> {
    let compose_file = docker_compose_path();

    // Try docker compose (v2) first
    let output = Command::new("docker")
        .args(["compose", "-f"])
        .arg(&compose_file)
        .args(args)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            return Ok(out);
        }
    }

    // Fall back to docker-compose (v1)
    let output = Command::new("docker-compose")
        .arg("-f")
        .arg(&compose_file)
        .args(args)
        .output()
        .context("Failed to run docker-compose")?;

    Ok(output)
}

/// Check if a specific container is running.
pub fn is_container_running(version: DbVersion) -> bool {
    let service = version.service_name();

    let output = docker_compose_cmd(&["ps", "-q", service]);
    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Start a specific database container.
pub fn start_container(version: DbVersion) -> Result<()> {
    let service = version.service_name();
    info!("Starting container: {}", service);

    let output = docker_compose_cmd(&["up", "-d", service])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to start container {}: {}", service, stderr);
    }

    Ok(())
}

/// Stop a specific database container.
pub fn stop_container(version: DbVersion) -> Result<()> {
    let service = version.service_name();
    info!("Stopping container: {}", service);

    let output = docker_compose_cmd(&["stop", service])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to stop container {}: {}", service, stderr);
    }

    Ok(())
}

/// Stop all test containers.
pub fn stop_all_containers() -> Result<()> {
    info!("Stopping all test containers");

    let output = docker_compose_cmd(&["down", "-v"])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to stop containers: {}", stderr);
    }

    Ok(())
}

/// Check if a TCP port is accepting connections.
async fn check_port(host: &str, port: u16) -> bool {
    let addr = format!("{}:{}", host, port);
    TcpStream::connect(&addr).await.is_ok()
}

/// Wait for a database container to be healthy (accepting connections).
pub async fn wait_for_healthy(version: DbVersion) -> Result<()> {
    let port = version.port();
    let service = version.service_name();

    info!("Waiting for {} to be healthy on port {}", service, port);

    let start = std::time::Instant::now();

    loop {
        if check_port("localhost", port).await {
            // Additional check for PostgreSQL: try to connect
            if version.is_postgres() {
                if check_postgres_ready(version).await {
                    debug!("{} is healthy after {:?}", service, start.elapsed());
                    return Ok(());
                }
            } else {
                // For MSSQL, port being open is enough for initial check
                debug!("{} port is open after {:?}", service, start.elapsed());
                return Ok(());
            }
        }

        if start.elapsed() > CONTAINER_STARTUP_TIMEOUT {
            bail!(
                "Timeout waiting for {} to be healthy after {:?}",
                service,
                CONTAINER_STARTUP_TIMEOUT
            );
        }

        sleep(HEALTH_CHECK_INTERVAL).await;
    }
}

/// Check if PostgreSQL is ready to accept queries.
async fn check_postgres_ready(version: DbVersion) -> bool {
    use crate::config::TestDbConfig;
    use sqlx::postgres::PgPoolOptions;

    let config = TestDbConfig::new(version);
    let conn_str = config.postgres_connection_string();

    let result = timeout(
        Duration::from_secs(2),
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(1))
            .connect(&conn_str),
    )
    .await;

    match result {
        Ok(Ok(pool)) => {
            // Try a simple query
            let query_result = sqlx::query("SELECT 1").fetch_one(&pool).await;
            query_result.is_ok()
        }
        _ => false,
    }
}

/// Ensure a container is running and healthy.
///
/// If the container is not running, starts it and waits for it to be healthy.
pub async fn ensure_container_running(version: DbVersion) -> Result<()> {
    if !is_docker_available() {
        bail!("Docker is not available. Please install Docker to run these tests.");
    }

    if !is_docker_compose_available() {
        bail!("docker-compose is not available. Please install docker-compose.");
    }

    if !is_container_running(version) {
        start_container(version)?;
    }

    wait_for_healthy(version).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_compose_path() {
        let path = docker_compose_path();
        assert!(path.ends_with("docker/docker-compose.yml"));
    }
}
