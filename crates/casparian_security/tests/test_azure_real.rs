//! Real Azure AD Integration Tests
//!
//! These tests run against the REAL Azure AD service and require actual credentials.
//!
//! # Setup
//!
//! 1. Register an application in Azure AD:
//!    - Go to Azure Portal → Azure Active Directory → App registrations
//!    - Create a new registration (any name)
//!    - Under "Authentication", enable "Allow public client flows"
//!    - Copy the "Application (client) ID" and "Directory (tenant) ID"
//!
//! 2. Set environment variables:
//!    ```bash
//!    export AZURE_TENANT_ID="your-tenant-id"
//!    export AZURE_CLIENT_ID="your-client-id"
//!    ```
//!
//! 3. Run the tests:
//!    ```bash
//!    # Run ONLY the real Azure tests (requires browser authentication)
//!    cargo test -p casparian_security --test test_azure_real -- --ignored --nocapture
//!    ```
//!
//! # Why This Matters
//!
//! - Validates implementation against real Azure AD API
//! - Catches breaking changes in Microsoft's endpoints
//! - Tests actual OAuth flows end-to-end
//! - Verifies token format and validation

use casparian_security::AzureProvider;

/// Check if real Azure credentials are configured
fn has_real_credentials() -> bool {
    std::env::var("AZURE_TENANT_ID").is_ok() && std::env::var("AZURE_CLIENT_ID").is_ok()
}

/// Get Azure credentials from environment
fn get_credentials() -> (String, String, Option<String>) {
    let tenant_id = std::env::var("AZURE_TENANT_ID")
        .expect("AZURE_TENANT_ID not set - see test file header for setup");
    let client_id = std::env::var("AZURE_CLIENT_ID")
        .expect("AZURE_CLIENT_ID not set - see test file header for setup");
    let client_secret = std::env::var("AZURE_CLIENT_SECRET").ok();
    (tenant_id, client_id, client_secret)
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_real_device_code_flow_interactive() {
    if !has_real_credentials() {
        eprintln!("⚠️  Skipping real Azure test - credentials not configured");
        eprintln!("   Set AZURE_TENANT_ID and AZURE_CLIENT_ID to enable");
        return;
    }

    let (tenant_id, client_id, client_secret) = get_credentials();

    println!("\n🔵 Testing REAL Azure Device Code Flow");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut provider = AzureProvider::new(tenant_id, client_id);
    if let Some(secret) = client_secret {
        provider = provider.with_client_secret(secret);
        println!("   Using confidential client (with client_secret)");
    } else {
        println!("   Using public client (device code flow only)");
    }

    // Initialize - fetch real OpenID configuration
    println!("1. Fetching OpenID configuration from Azure AD...");
    provider
        .initialize()
        .await
        .expect("Failed to fetch OpenID config");
    println!("   ✓ OpenID config retrieved");

    // Start device code flow
    println!("\n2. Requesting device code...");
    let device_code = provider
        .start_device_code_flow("https://graph.microsoft.com/.default")
        .await
        .expect("Failed to start device code flow");

    println!("   ✓ Device code received: {}", device_code.user_code);
    println!("\n🔐 AUTHENTICATION REQUIRED");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("1. Open your browser and go to:");
    println!("   {}", device_code.verification_uri);
    println!("\n2. Enter this code:");
    println!("   {}", device_code.user_code);
    println!("\n3. Complete sign-in in the browser");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Poll for token
    println!("\n⏳ Waiting for you to authenticate (timeout: {}s)...", device_code.expires_in_secs());
    let poll_interval = std::time::Duration::from_secs(device_code.interval_secs());
    let timeout = std::time::Duration::from_secs(device_code.expires_in_secs());

    let token = provider
        .poll_for_token(&device_code.device_code, poll_interval, timeout)
        .await
        .expect("Failed to get token - did you complete authentication?");

    println!("\n✅ SUCCESS! Received access token");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Token type: {}", token.token_type);
    println!("Expires in: {} seconds", token.expires_in_secs());
    println!("Access token (first 50 chars): {}...", &token.access_token[..50.min(token.access_token.len())]);

    if let Some(refresh_token) = &token.refresh_token {
        println!("Refresh token: Present ({} chars)", refresh_token.len());
    }

    if let Some(id_token) = &token.id_token {
        println!("ID token: Present ({} chars)", id_token.len());
    }

    // Validate token structure
    assert_eq!(token.token_type, "Bearer", "Expected Bearer token");
    assert!(!token.access_token.is_empty(), "Access token should not be empty");
    assert!(token.expires_in_secs() > 0, "Token should have expiration");

    println!("\n✓ All validations passed!");
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_real_authenticate_helper() {
    if !has_real_credentials() {
        eprintln!("⚠️  Skipping real Azure test - credentials not configured");
        return;
    }

    let (tenant_id, client_id, client_secret) = get_credentials();

    println!("\n🔵 Testing authenticate() helper method");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut provider = AzureProvider::new(tenant_id, client_id);
    if let Some(secret) = client_secret {
        provider = provider.with_client_secret(secret);
    }

    // This will print instructions and wait for user to authenticate
    let token = provider
        .authenticate("https://graph.microsoft.com/.default")
        .await
        .expect("Authentication failed");

    println!("\n✅ Authentication successful!");
    println!("Token type: {}", token.token_type);
    assert_eq!(token.token_type, "Bearer");
}

#[tokio::test]
#[ignore]
async fn test_real_openid_config_structure() {
    if !has_real_credentials() {
        eprintln!("⚠️  Skipping - credentials not configured");
        return;
    }

    let (tenant_id, client_id, _client_secret) = get_credentials();
    let mut provider = AzureProvider::new(tenant_id, client_id);

    provider.initialize().await.expect("Failed to initialize");

    println!("\n✅ Successfully fetched and parsed OpenID configuration");
    println!("   This validates that Azure's API structure matches our expectations");
}

#[test]
fn test_credentials_documentation() {
    // This test always runs to remind developers about credential setup
    if !has_real_credentials() {
        println!("\n💡 TIP: Real Azure integration tests are available!");
        println!("   To enable them, set environment variables:");
        println!();
        println!("   export AZURE_TENANT_ID=\"your-tenant-id\"");
        println!("   export AZURE_CLIENT_ID=\"your-client-id\"");
        println!();
        println!("   Then run: cargo test -p casparian_security --test test_azure_real -- --ignored --nocapture");
        println!();
    } else {
        println!("\n✓ Azure credentials configured!");
        println!("  Run: cargo test -p casparian_security --test test_azure_real -- --ignored --nocapture");
    }
}
