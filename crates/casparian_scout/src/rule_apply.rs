//! Shared rule-application helpers for tagging files.

use super::error::{Result, ScoutError};
use super::patterns;
use super::types::TaggingRuleId;
use globset::GlobMatcher;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RuleApplyFile {
    pub id: i64,
    pub path: String,
    pub rel_path: String,
    pub size: i64,
}

#[derive(Debug, Clone)]
pub struct RuleApplyRule {
    pub id: TaggingRuleId,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub file_id: i64,
    pub rel_path: String,
    pub size_bytes: u64,
    pub rule_id: TaggingRuleId,
    pub tag: String,
    pub pattern: String,
}

/// Summary of tagging operation
#[derive(Debug, Default)]
pub struct TaggingSummary {
    /// Pattern -> (tag, file_count, total_bytes)
    pub matches: HashMap<String, (String, usize, u64)>,
    /// Files that would be queued
    pub would_queue: usize,
    /// New files in queue
    pub new_in_queue: usize,
    /// Untagged files (no pattern matched)
    pub untagged: usize,
}

struct CompiledRule {
    rule: RuleApplyRule,
    matcher: GlobMatcher,
}

/// Match files to tagging rules (first match wins).
pub fn match_rules_to_files(
    files: &[RuleApplyFile],
    rules: &[RuleApplyRule],
) -> Result<(Vec<RuleMatch>, TaggingSummary)> {
    let mut compiled = Vec::with_capacity(rules.len());
    for rule in rules {
        let normalized = patterns::normalize_glob_pattern(&rule.pattern);
        let matcher = patterns::build_matcher(&normalized)
            .map_err(|e| ScoutError::Pattern(format!("{}: {}", rule.pattern, e)))?;
        compiled.push(CompiledRule {
            rule: rule.clone(),
            matcher,
        });
    }

    let mut summary = TaggingSummary::default();
    let mut matches = Vec::new();

    for file in files {
        let mut matched = false;
        for compiled_rule in &compiled {
            if compiled_rule.matcher.is_match(&file.rel_path) {
                let entry = summary
                    .matches
                    .entry(compiled_rule.rule.pattern.clone())
                    .or_insert((compiled_rule.rule.tag.clone(), 0, 0));
                entry.1 += 1;
                entry.2 += file.size as u64;

                matches.push(RuleMatch {
                    file_id: file.id,
                    rel_path: file.rel_path.clone(),
                    size_bytes: file.size as u64,
                    rule_id: compiled_rule.rule.id,
                    tag: compiled_rule.rule.tag.clone(),
                    pattern: compiled_rule.rule.pattern.clone(),
                });
                matched = true;
                break;
            }
        }
        if !matched {
            summary.untagged += 1;
        }
    }

    summary.would_queue = matches.len();
    summary.new_in_queue = matches.len();

    Ok((matches, summary))
}
