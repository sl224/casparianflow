//! Snapshot regression tests for TUI rendering.

use super::snapshot::{
    buffer_to_bg_mask, buffer_to_plain_text, normalize_for_snapshot, render_app_to_buffer,
};
use super::snapshot_states::snapshot_cases;

const SNAPSHOT_TEST_SIZES: &[(u16, u16)] = &[(80, 24), (120, 40)];

#[test]
fn test_snapshot_cases_render_without_panic() {
    for case in snapshot_cases() {
        for &(width, height) in SNAPSHOT_TEST_SIZES {
            let app = (case.build)();
            let buffer = render_app_to_buffer(&app, width, height)
                .unwrap_or_else(|err| panic!("{} {}x{}: {}", case.name, width, height, err));
            let plain = buffer_to_plain_text(&buffer);
            let line_count = plain.lines().count();
            assert_eq!(
                line_count, height as usize,
                "{} {}x{} produced {} lines",
                case.name, width, height, line_count
            );
        }
    }
}

#[test]
fn test_snapshot_regressions() {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\b\d+[smhd] ago\b", "<REL>");
    settings.add_filter(r"\b\d+h \d+m\b", "<DUR>");
    settings.add_filter(r"\b\d+m \d+s\b", "<DUR>");
    settings.add_filter(r"\b\d+h\b", "<DUR>");
    settings.add_filter(r"\b\d+m\b", "<DUR>");
    settings.add_filter(r"\b\d+s\b", "<DUR>");
    settings.add_filter(r"\[[-\\|/]\]", "<SPIN>");
    settings.add_filter(r"[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]", "<SPIN>");

    settings.bind(|| {
        for case in snapshot_cases() {
            for &(width, height) in SNAPSHOT_TEST_SIZES {
                let app = (case.build)();
                let buffer = render_app_to_buffer(&app, width, height)
                    .unwrap_or_else(|err| panic!("{} {}x{}: {}", case.name, width, height, err));
                let plain = normalize_for_snapshot(&buffer_to_plain_text(&buffer));
                let mask = normalize_for_snapshot(&buffer_to_bg_mask(&buffer));

                let name_plain = format!("tui__{}__{}x{}__plain", case.name, width, height);
                let name_mask = format!("tui__{}__{}x{}__mask", case.name, width, height);

                insta::assert_snapshot!(name_plain, plain);
                insta::assert_snapshot!(name_mask, mask);
            }
        }
    });
}
