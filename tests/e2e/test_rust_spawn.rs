//! Minimal Rust test to reproduce bridge subprocess issue.
//! Build: rustc test_rust_spawn.rs -o test_rust_spawn
//! Run: ./test_rust_spawn

use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::io::Read;

fn main() {
    let python = PathBuf::from(
        std::env::var("HOME").unwrap() + "/.casparian_flow/venvs/test_env_hash_123/bin/python"
    );
    let venv = PathBuf::from("/Users/shan/workspace/casparianflow/.venv");

    println!("Testing Rust subprocess spawn:");
    println!("  Python: {}", python.display());
    println!("  VIRTUAL_ENV: {}", venv.display());

    // Test 1: Simple import
    println!("\n--- Test 1: Simple pyarrow import ---");
    let output = Command::new(&python)
        .args(["-c", "import pyarrow; print('OK:', pyarrow.__version__)"])
        .env("VIRTUAL_ENV", &venv)
        .output()
        .expect("Failed to execute");

    println!("Exit code: {}", output.status.code().unwrap_or(-1));
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Test 2: With piped stdout/stderr (like bridge does)
    println!("\n--- Test 2: With piped IO (like bridge) ---");
    let mut child = Command::new(&python)
        .args(["-c", "import pyarrow; print('OK:', pyarrow.__version__)"])
        .env("VIRTUAL_ENV", &venv)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn");

    let status = child.wait().expect("Failed to wait");
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_string(&mut stdout).ok();
    }
    if let Some(mut err) = child.stderr.take() {
        err.read_to_string(&mut stderr).ok();
    }

    println!("Exit code: {}", status.code().unwrap_or(-1));
    println!("Stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("Stderr: {}", stderr);
    }

    // Test 3: Minimal env (like env_clear would do)
    println!("\n--- Test 3: With env_clear + selective inheritance ---");
    let output = Command::new(&python)
        .args(["-c", "import pyarrow; print('OK:', pyarrow.__version__)"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .env("VIRTUAL_ENV", &venv)
        .output()
        .expect("Failed to execute");

    println!("Exit code: {}", output.status.code().unwrap_or(-1));
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("\n--- Done ---");
}
