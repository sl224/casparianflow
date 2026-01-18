use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use casparian::tui_extraction::{
    extract_path_archetypes, group_variant_archetypes, VariantGroup,
};

fn assert_group_contains(templates: &[String], expected: &[&str]) {
    for &needle in expected {
        assert!(templates.iter().any(|t| t.contains(needle)), "missing variant: {needle}");
    }
}

fn has_group_with(expected: &[&str], groups: &[VariantGroup]) -> bool {
    groups.iter().any(|group| {
        let templates: Vec<String> = group.variants.iter().map(|v| v.template.clone()).collect();
        expected.iter().all(|needle| templates.iter().any(|t| t.contains(needle)))
    })
}

fn hl7_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for i in 0..24 {
        paths.push(format!("/health/hl7/admission_in/ADT_202401{:02}_{}.hl7", (i % 28) + 1, i));
    }
    for i in 0..22 {
        paths.push(format!("/health/hl7/adm_in/ADT_202401{:02}_{}.hl7", (i % 28) + 1, i));
    }
    for i in 0..20 {
        paths.push(format!("/health/hl7/lab_in/ORU_202401{:02}_{}.hl7", (i % 28) + 1, i));
    }
    for i in 0..18 {
        paths.push(format!("/health/hl7/lab_out/ORU_2024-01-{:02}_{}.hl7", (i % 28) + 1, i));
    }
    for i in 0..16 {
        paths.push(format!("/health/hl7/facility_a/inbound/ADT_{:04}{:02}{:02}_{}.hl7", 2024, 1, (i % 28) + 1, i));
    }
    for i in 0..14 {
        paths.push(format!("/health/hl7/facility_a/in/ADT_{:04}{:02}{:02}_{}.hl7", 2024, 1, (i % 28) + 1, i));
    }
    paths
}

fn defense_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for i in 0..28 {
        paths.push(format!("/defense/mission_{:03}/satA/2024/01/15/telemetry_{:04}.csv", i, i));
    }
    for i in 0..26 {
        paths.push(format!("/defense/msn_{:03}/satA/2024/01/15/telemetry_{:04}.csv", i, i));
    }
    for i in 0..18 {
        paths.push(format!("/defense/mission_{:03}/satA/2024/01/15/telemetry_{:04}.json", i, i));
    }
    for i in 0..16 {
        paths.push(format!("/defense/patrol_{:03}/uav_01/2024/01/15/telemetry_{:04}.csv", i, i));
    }
    for i in 0..16 {
        paths.push(format!("/defense/ptl_{:03}/uav_01/2024/01/15/telemetry_{:04}.csv", i, i));
    }
    for i in 0..12 {
        paths.push(format!("/defense/mission_{:03}/satA/2024/01/15/imagery_{:04}.tif", i, i));
    }
    paths
}

fn finance_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for i in 1..=20 {
        paths.push(format!("/finance/netsuite/exports/2024/01/fin_export_202401{:02}.csv", i));
    }
    for i in 1..=18 {
        paths.push(format!("/finance/ns/exports/2024/01/fin_export_202401{:02}.xlsx", i));
    }
    for i in 1..=16 {
        paths.push(format!("/finance/saved_search/ap_aging/2024/01/transactions_202401{:02}.csv", i));
    }
    for i in 1..=16 {
        paths.push(format!("/finance/saved_search/ap_age/2024/01/transactions_202401{:02}.xlsx", i));
    }
    for i in 1..=12 {
        paths.push(format!("/finance/payroll/exports/2024/01/payroll_202401{:02}.csv", i));
    }
    for i in 1..=10 {
        paths.push(format!("/finance/gl/exports/2024/01/general_ledger_202401{:02}.csv", i));
    }
    paths
}

fn write_tree_dump(label: &str, paths: &[String]) {
    let dump_dir = PathBuf::from("output/variant_grouping_fixtures");
    let _ = fs::create_dir_all(&dump_dir);
    let file_path = dump_dir.join(format!("{label}.tree.txt"));

    let mut tree = TreeNode::default();
    for path in paths {
        tree.insert(path);
    }

    let mut output = String::new();
    output.push_str(&format!("{label}\n"));
    tree.render(&mut output, "");
    let _ = fs::write(&file_path, output);
}

fn write_debug_dump(label: &str, archetypes: &[casparian::tui_extraction::PathArchetype], groups: &[VariantGroup]) {
    let dump_dir = PathBuf::from("output/variant_grouping_fixtures");
    let _ = fs::create_dir_all(&dump_dir);
    let file_path = dump_dir.join(format!("{label}.debug.txt"));

    let mut output = String::new();
    output.push_str(&format!("{label}\n\nArchetypes:\n"));
    for arch in archetypes {
        let sample = arch.sample_paths.first().cloned().unwrap_or_default();
        output.push_str(&format!(
            "- template: {}\n  files: {}\n  sample: {}\n",
            arch.template, arch.file_count, sample
        ));
    }

    output.push_str("\nGroups:\n");
    for group in groups {
        output.push_str(&format!("- {}\n", group.label));
        for variant in &group.variants {
            output.push_str(&format!(
                "  - template: {}\n    files: {}\n    sample: {}\n    ext: {}\n    filename_template: {}\n",
                variant.template,
                variant.file_count,
                variant.sample_path,
                variant.extension.clone().unwrap_or_else(|| "none".to_string()),
                variant.filename_template
            ));
        }
    }

    let _ = fs::write(&file_path, output);
}

#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
}

impl TreeNode {
    fn insert(&mut self, path: &str) {
        let trimmed = path.trim_start_matches('/');
        let mut node = self;
        for part in trimmed.split('/').filter(|p| !p.is_empty()) {
            node = node.children.entry(part.to_string()).or_default();
        }
    }

    fn render(&self, output: &mut String, prefix: &str) {
        let total = self.children.len();
        for (idx, (name, child)) in self.children.iter().enumerate() {
            let is_last = idx + 1 == total;
            let branch = if is_last { "└── " } else { "├── " };
            output.push_str(prefix);
            output.push_str(branch);
            output.push_str(name);
            output.push('\n');
            let next_prefix = if is_last { format!("{prefix}    ") } else { format!("{prefix}│   ") };
            child.render(output, &next_prefix);
        }
    }
}

#[test]
fn variant_grouping_healthcare_hl7() {
    let paths = hl7_paths();
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_tree_dump("healthcare_hl7", &paths);
    }
    let start = Instant::now();
    let archetypes = extract_path_archetypes(&paths, 50);
    let groups = group_variant_archetypes(&archetypes);
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_debug_dump("healthcare_hl7", &archetypes, &groups);
    }
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(300), "hl7 grouping too slow: {elapsed:?}");
    assert!(has_group_with(&["admission_in", "adm_in"], &groups));
    assert!(has_group_with(&["inbound", "in"], &groups));
    assert!(!has_group_with(&["admission_in", "lab_in"], &groups));
}

#[test]
fn variant_grouping_defense_telemetry() {
    let paths = defense_paths();
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_tree_dump("defense_telemetry", &paths);
    }
    let start = Instant::now();
    let archetypes = extract_path_archetypes(&paths, 50);
    let groups = group_variant_archetypes(&archetypes);
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_debug_dump("defense_telemetry", &archetypes, &groups);
    }
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(300), "defense grouping too slow: {elapsed:?}");
    assert!(has_group_with(&["mission_<n>", "msn_<n>"], &groups));
    assert!(has_group_with(&["patrol_<n>", "ptl_<n>"], &groups));
}

#[test]
fn variant_grouping_finance_exports() {
    let paths = finance_paths();
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_tree_dump("finance_exports", &paths);
    }
    let start = Instant::now();
    let archetypes = extract_path_archetypes(&paths, 50);
    let groups = group_variant_archetypes(&archetypes);
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_debug_dump("finance_exports", &archetypes, &groups);
    }
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(300), "finance grouping too slow: {elapsed:?}");
    assert!(has_group_with(&["netsuite", "ns"], &groups));
    assert!(has_group_with(&["ap_aging", "ap_age"], &groups));
}

#[test]
fn variant_grouping_real_world_summary() {
    let mut paths = Vec::new();
    paths.extend(hl7_paths());
    paths.extend(defense_paths());
    paths.extend(finance_paths());
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_tree_dump("combined", &paths);
    }

    let start = Instant::now();
    let archetypes = extract_path_archetypes(&paths, 100);
    let groups = group_variant_archetypes(&archetypes);
    if env::var("DUMP_FIXTURES").ok().as_deref() == Some("1") {
        write_debug_dump("combined", &archetypes, &groups);
    }
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(400), "combined grouping too slow: {elapsed:?}");
    let templates: Vec<String> = groups.iter().flat_map(|g| g.variants.iter().map(|v| v.template.clone())).collect();
    assert_group_contains(&templates, &["admission_in", "adm_in", "mission_<n>", "msn_<n>", "netsuite", "ns", "ap_aging", "ap_age"]);
}
