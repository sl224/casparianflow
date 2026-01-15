//! Simple script to download the default LLM model for AI Wizards
//! Run with: cargo run --features local-llm --example download_model

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Downloading Qwen2.5-Coder-1.5B model for AI Wizards...\n");

    let models_dir = dirs::home_dir()
        .map(|h| h.join(".casparian_flow").join("models"))
        .unwrap_or_else(|| PathBuf::from("./models"));

    let model_repo = "Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF";
    let model_file = "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf";
    let model_path = models_dir.join(model_file);

    // Check if already downloaded
    if model_path.exists() {
        println!("Model already exists at: {:?}", model_path);
        println!("Size: {} MB", std::fs::metadata(&model_path)?.len() / 1_000_000);
        return Ok(());
    }

    // Create models directory
    std::fs::create_dir_all(&models_dir)?;
    println!("Models directory: {:?}", models_dir);

    // Download from HuggingFace
    println!("Downloading from HuggingFace: {}/{}", model_repo, model_file);
    println!("This may take a few minutes (~1GB)...\n");

    let api = hf_hub::api::sync::Api::new()?;
    let repo = api.model(model_repo.to_string());
    let downloaded_path = repo.get(model_file)?;

    println!("Downloaded to HF cache: {:?}", downloaded_path);

    // Copy to our models directory
    println!("Copying to: {:?}", model_path);
    std::fs::copy(&downloaded_path, &model_path)?;

    let size = std::fs::metadata(&model_path)?.len();
    println!("\nModel downloaded successfully!");
    println!("Path: {:?}", model_path);
    println!("Size: {} MB", size / 1_000_000);

    Ok(())
}
