//! Azure Active Directory Authentication
//!
//! Implements Device Code Flow for CLI authentication without browser access.
//! Uses raw HTTP requests (no heavy SDK dependencies).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Azure OpenID Configuration
#[derive(Debug, Deserialize)]
pub struct OpenIdConfig {
    pub token_endpoint: String,
    pub device_authorization_endpoint: String,
    pub jwks_uri: String,
    pub issuer: String,
}

/// Device Code Response
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    #[serde(alias = "verification_url")]
    pub verification_uri: String,
    pub expires_in: String, // Azure returns as string
    pub interval: String,   // Azure returns as string
    #[serde(default)]
    pub message: Option<String>,
}

impl DeviceCodeResponse {
    pub fn expires_in_secs(&self) -> u64 {
        self.expires_in.parse().unwrap_or(900)
    }

    pub fn interval_secs(&self) -> u64 {
        self.interval.parse().unwrap_or(5)
    }
}

/// Token Response
#[derive(Debug, Deserialize, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: String, // Azure returns as string
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
}

impl TokenResponse {
    pub fn expires_in_secs(&self) -> u64 {
        self.expires_in.parse().unwrap_or(3600)
    }
}

/// Token Error Response
#[derive(Debug, Deserialize)]
pub struct TokenError {
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

/// Azure Identity Provider
pub struct AzureProvider {
    tenant_id: String,
    client_id: String,
    client_secret: Option<String>,
    http_client: reqwest::Client,
    config: Option<OpenIdConfig>,
}

impl AzureProvider {
    /// Create a new Azure provider
    pub fn new(tenant_id: String, client_id: String) -> Self {
        Self {
            tenant_id,
            client_id,
            client_secret: None,
            http_client: reqwest::Client::new(),
            config: None,
        }
    }

    /// Set client secret for confidential client flow
    pub fn with_client_secret(mut self, client_secret: String) -> Self {
        self.client_secret = Some(client_secret);
        self
    }

    /// Initialize the provider by fetching OpenID configuration
    pub async fn initialize(&mut self) -> Result<()> {
        let config_url = format!(
            "https://login.microsoftonline.com/{}/.well-known/openid-configuration",
            self.tenant_id
        );

        tracing::info!("Fetching Azure OpenID configuration from {}", config_url);

        let config: OpenIdConfig = self
            .http_client
            .get(&config_url)
            .send()
            .await
            .context("Failed to fetch OpenID configuration")?
            .json()
            .await
            .context("Failed to parse OpenID configuration")?;

        tracing::debug!("OpenID config: {:?}", config);
        self.config = Some(config);
        Ok(())
    }

    /// Get the OpenID configuration (must call initialize first)
    fn get_config(&self) -> Result<&OpenIdConfig> {
        self.config
            .as_ref()
            .context("Provider not initialized - call initialize() first")
    }

    /// Start device code flow
    pub async fn start_device_code_flow(&self, scope: &str) -> Result<DeviceCodeResponse> {
        let config = self.get_config()?;

        let params = [
            ("client_id", self.client_id.as_str()),
            ("scope", scope),
        ];

        tracing::info!("Starting device code flow with scope: {}", scope);

        let response = self
            .http_client
            .post(&config.device_authorization_endpoint)
            .form(&params)
            .send()
            .await
            .context("Failed to request device code")?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Device code request failed: {}", error_text);
        }

        let device_code: DeviceCodeResponse = response
            .json()
            .await
            .context("Failed to parse device code response")?;

        Ok(device_code)
    }

    /// Poll for token (call after user has entered code)
    pub async fn poll_for_token(
        &self,
        device_code: &str,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<TokenResponse> {
        let config = self.get_config()?;

        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                anyhow::bail!("Device code flow timed out after {:?}", timeout);
            }

            tracing::debug!("Polling token endpoint...");

            // Build form params
            let mut form = vec![
                ("client_id", self.client_id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("code", device_code),
            ];

            // Add client_secret for confidential clients
            if let Some(ref secret) = self.client_secret {
                form.push(("client_secret", secret.as_str()));
            }

            let response = self
                .http_client
                .post(&config.token_endpoint)
                .form(&form)
                .send()
                .await
                .context("Failed to poll token endpoint")?;

            if response.status().is_success() {
                let token: TokenResponse = response
                    .json()
                    .await
                    .context("Failed to parse token response")?;
                tracing::info!("Successfully obtained access token");
                return Ok(token);
            }

            // Check for error response
            let error: TokenError = response
                .json()
                .await
                .context("Failed to parse error response")?;

            match error.error.as_str() {
                "authorization_pending" => {
                    // User hasn't completed auth yet, continue polling
                    tracing::debug!("Authorization pending, continuing to poll...");
                }
                "slow_down" => {
                    // Increase polling interval
                    tracing::warn!("Received slow_down, doubling poll interval");
                    tokio::time::sleep(poll_interval).await;
                }
                "authorization_declined" => {
                    anyhow::bail!("User declined authorization");
                }
                "expired_token" => {
                    anyhow::bail!("Device code expired");
                }
                _ => {
                    anyhow::bail!(
                        "Token request failed: {} - {:?}",
                        error.error,
                        error.error_description
                    );
                }
            }

            // Sleep before next poll (at end of loop for better responsiveness)
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Complete device code flow (combines start and poll)
    pub async fn authenticate(&mut self, scope: &str) -> Result<TokenResponse> {
        // Initialize if not already done
        if self.config.is_none() {
            self.initialize().await?;
        }

        // Start device code flow
        let device_code = self.start_device_code_flow(scope).await?;

        // Print instructions for user
        println!("\n🔐 Azure Authentication Required");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("1. Open your browser and navigate to:");
        println!("   {}", device_code.verification_uri);
        println!("\n2. Enter the code:");
        println!("   {}", device_code.user_code);
        println!("\n3. Complete the sign-in process");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("⏳ Waiting for authentication (timeout: {}s)...\n", device_code.expires_in_secs());

        // Poll for token
        let poll_interval = Duration::from_secs(device_code.interval_secs());
        let timeout = Duration::from_secs(device_code.expires_in_secs());

        self.poll_for_token(&device_code.device_code, poll_interval, timeout)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_azure_provider_creation() {
        let provider = AzureProvider::new(
            "test-tenant".to_string(),
            "test-client".to_string(),
        );
        assert_eq!(provider.tenant_id, "test-tenant");
        assert_eq!(provider.client_id, "test-client");
        assert!(provider.config.is_none());
    }

    #[test]
    fn test_get_config_before_init() {
        let provider = AzureProvider::new(
            "test-tenant".to_string(),
            "test-client".to_string(),
        );
        let result = provider.get_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }
}
