//! Assertion helpers for TUI flows.

use crate::cli::tui::flow::FlowAssertion;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct FlowAssertError {
    pub failures: Vec<String>,
}

impl FlowAssertError {
    pub fn new(failures: Vec<String>) -> Self {
        Self { failures }
    }
}

pub fn assert_flow(
    assertion: &FlowAssertion,
    plain: &str,
    mask: &str,
    layout_signature: &str,
    ui_signature_key: &str,
) -> Result<(), FlowAssertError> {
    let mut failures = Vec::new();

    failures.extend(check_patterns(
        &assertion.plain_contains,
        plain,
        true,
        "plain_contains",
    ));
    failures.extend(check_patterns(
        &assertion.plain_not_contains,
        plain,
        false,
        "plain_not_contains",
    ));
    failures.extend(check_patterns(
        &assertion.mask_contains,
        mask,
        true,
        "mask_contains",
    ));
    failures.extend(check_patterns(
        &assertion.mask_not_contains,
        mask,
        false,
        "mask_not_contains",
    ));
    failures.extend(check_patterns(
        &assertion.layout_contains,
        layout_signature,
        true,
        "layout_contains",
    ));
    failures.extend(check_patterns(
        &assertion.layout_not_contains,
        layout_signature,
        false,
        "layout_not_contains",
    ));
    if let Some(expected) = assertion.ui_signature_key.as_ref() {
        if expected != ui_signature_key {
            failures.push(format!(
                "ui_signature_key expected '{}' got '{}'",
                expected, ui_signature_key
            ));
        }
    }
    failures.extend(check_patterns(
        &assertion.ui_signature_contains,
        ui_signature_key,
        true,
        "ui_signature_contains",
    ));
    failures.extend(check_patterns(
        &assertion.ui_signature_not_contains,
        ui_signature_key,
        false,
        "ui_signature_not_contains",
    ));

    if failures.is_empty() {
        Ok(())
    } else {
        Err(FlowAssertError::new(failures))
    }
}

fn check_patterns(
    patterns: &[String],
    haystack: &str,
    should_match: bool,
    label: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    for pattern in patterns {
        match Regex::new(pattern) {
            Ok(regex) => {
                let is_match = regex.is_match(haystack);
                if should_match && !is_match {
                    failures.push(format!("{} missing /{}/", label, pattern));
                }
                if !should_match && is_match {
                    failures.push(format!("{} matched /{}/", label, pattern));
                }
            }
            Err(err) => failures.push(format!("invalid regex {}: {}", label, err)),
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_flow_plain_contains() {
        let assertion = FlowAssertion {
            plain_contains: vec!["hello".to_string()],
            ..FlowAssertion::default()
        };
        assert!(assert_flow(&assertion, "hello world", "", "", "").is_ok());
    }
}
