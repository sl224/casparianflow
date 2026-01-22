//! End-to-end test for Pathfinder
//!
//! This test verifies the Pathfinder pattern analysis and code generation.
//! Tests PathAnalyzer, YamlRuleGenerator, and PythonGenerator with a mock LLM callback.
//!
//! For real LLM testing, use `cargo run --features local-llm --example pathfinder_demo`

use std::path::PathBuf;

/// Check if the LLM model is available (for integration tests)
fn model_path() -> Option<PathBuf> {
    let models_dir = dirs::home_dir()?.join(".casparian_flow").join("models");
    let model_file = "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf";
    let path = models_dir.join(model_file);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[test]
fn test_llm_model_exists() {
    // This test verifies the model was downloaded
    let path = model_path();
    if path.is_none() {
        println!("LLM model not found. Run: cargo run --features local-llm --example download_model");
        println!("Skipping model verification test");
        return;
    }

    let path = path.unwrap();
    let metadata = std::fs::metadata(&path).expect("Failed to read model metadata");
    let size_mb = metadata.len() / 1_000_000;

    // Qwen 1.5B Q4 should be ~1GB
    assert!(
        size_mb > 500,
        "Model file too small ({} MB), may be corrupted",
        size_mb
    );

    println!("Model found: {:?} ({} MB)", path, size_mb);
}

#[test]
fn test_pathfinder_analyzer() {
    use casparian::ai::pathfinder::PathAnalyzer;

    // Test paths with structured patterns
    let paths = vec![
        "/data/2024-01-15/client_001/report.csv".to_string(),
        "/data/2024-02-20/client_002/report.csv".to_string(),
        "/data/2024-03-10/client_003/report.csv".to_string(),
    ];

    let analyzer = PathAnalyzer::new();
    let pattern = analyzer.analyze(&paths).expect("Analysis failed");

    println!("Detected pattern:");
    println!("  Glob: {}", pattern.glob);
    println!("  Confidence: {:.2}", pattern.confidence);
    println!("  Segments: {}", pattern.segments.len());

    // Should detect date and client patterns
    assert!(pattern.confidence > 0.5, "Confidence too low");
    assert!(pattern.glob.contains("*"), "Glob should contain wildcard");

    // Test extraction
    let extracted: Vec<(String, String)> =
        pattern.extract("/data/2024-01-15/client_001/report.csv");
    println!("\nExtracted fields:");
    for (name, value) in &extracted {
        println!("  {}: {}", name, value);
    }
    assert!(!extracted.is_empty(), "No fields extracted");
}

#[test]
fn test_yaml_rule_generator() {
    use casparian::ai::pathfinder::{PathAnalyzer, YamlRuleGenerator};

    let paths = vec![
        "/data/2024-01-15/client_001/report.csv".to_string(),
        "/data/2024-02-20/client_002/report.csv".to_string(),
        "/data/2024-03-10/client_003/report.csv".to_string(),
    ];

    let analyzer = PathAnalyzer::new();
    let pattern = analyzer.analyze(&paths).expect("Analysis failed");

    let yaml_gen = YamlRuleGenerator::new();
    let rule = yaml_gen
        .generate(&pattern, Some("daily client reports"))
        .expect("YAML generation failed");

    let yaml_output: String = rule.to_yaml();
    println!("Generated YAML:\n{}", yaml_output);

    // Verify YAML structure
    assert!(yaml_output.contains("name:"), "YAML should have name field");
    assert!(yaml_output.contains("glob:"), "YAML should have glob field");
    assert!(yaml_output.contains("extract:"), "YAML should have extract field");
}

#[test]
fn test_python_validator() {
    use casparian::ai::pathfinder::PythonValidator;

    let validator = PythonValidator::new();

    // Valid Python code
    let valid_code = r#"
import re
from pathlib import Path

def extract(path: str) -> dict:
    """Extract fields from path."""
    pattern = re.compile(r'/data/(\d{4})/(.+)')
    match = pattern.match(path)
    if match:
        return {
            'year': match.group(1),
            'filename': match.group(2),
        }
    return {}
"#;

    let result = validator.validate(valid_code).unwrap();
    assert!(result.is_valid, "Expected valid code: {:?}", result.errors);

    // Code with forbidden import
    let forbidden_code = r#"
import subprocess

def run_cmd():
    subprocess.run(['ls'])
"#;

    let result = validator.validate(forbidden_code).unwrap();
    assert!(!result.is_valid, "Should reject subprocess import");
    assert!(result.errors.iter().any(|e| e.contains("subprocess")));
}

#[test]
fn test_python_generator_with_mock_llm() {
    use casparian::ai::pathfinder::{PathAnalyzer, PythonGenerator};

    // Create a mock LLM callback that returns reasonable Python code
    let mock_llm: casparian::ai::pathfinder::python_gen::LlmGenerateFn = Box::new(|_prompt| {
        Ok(r#"```python
import re
from pathlib import Path

def extract(path: str) -> dict:
    """Extract metadata from log file path."""
    pattern = re.compile(r'/logs/app_(\d{4}-\d{2}-\d{2})_(node\d+)_v([\d.]+)\.log')
    match = pattern.match(path)
    if match:
        return {
            'date': match.group(1),
            'node': match.group(2),
            'version': match.group(3),
        }
    return {}
```"#.to_string())
    });

    let python_gen = PythonGenerator::new(mock_llm);

    let paths = vec![
        "/logs/app_2024-01-15_node1_v2.3.1.log".to_string(),
        "/logs/app_2024-01-16_node2_v2.3.1.log".to_string(),
        "/logs/app_2024-01-17_node1_v2.3.2.log".to_string(),
    ];

    let analyzer = PathAnalyzer::new();
    let pattern = analyzer.analyze(&paths).expect("Analysis failed");

    let result = python_gen
        .generate(&paths, &pattern, Some("extract date, node, and version"));

    match result {
        Ok(code) => {
            println!("Generated Python code:\n{}", code);

            // Verify it looks like Python
            assert!(
                code.contains("def") || code.contains("import"),
                "Output doesn't look like Python: {}",
                code
            );
        }
        Err(e) => {
            panic!("Python generation failed: {}", e);
        }
    }
}

#[test]
fn test_complex_path_patterns() {
    use casparian::ai::pathfinder::PathAnalyzer;

    // Test with more complex patterns
    let paths = vec![
        "/projects/PROJECT-123/data/2024/Q1/sales.csv".to_string(),
        "/projects/PROJECT-456/data/2024/Q2/sales.csv".to_string(),
        "/projects/PROJECT-789/data/2024/Q3/sales.csv".to_string(),
    ];

    let analyzer = PathAnalyzer::new();
    let pattern = analyzer.analyze(&paths).expect("Analysis failed");

    println!("Complex pattern detected:");
    println!("  Glob: {}", pattern.glob);
    println!("  Confidence: {:.2}", pattern.confidence);

    // Should have high confidence for this structured pattern
    assert!(pattern.confidence > 0.6, "Confidence should be high for structured paths");

    // Test extraction
    let extracted: Vec<(String, String)> =
        pattern.extract("/projects/PROJECT-123/data/2024/Q1/sales.csv");
    println!("\nExtracted fields:");
    for (name, value) in &extracted {
        println!("  {}: {}", name, value);
    }

    // Should extract project ID and year
    let has_project = extracted.iter().any(|(_, v)| v.contains("PROJECT") || v.contains("123"));
    let has_year = extracted.iter().any(|(_, v)| v == "2024");
    assert!(has_project || has_year, "Should extract project or year from path");
}
