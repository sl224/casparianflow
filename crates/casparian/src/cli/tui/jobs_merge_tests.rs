use crate::cli::tui::app::{
    BacktestInfo, JobFailure, JobInfo, JobOrigin, JobStatus, JobType, JobsState,
};
use chrono::{Local, TimeZone};

fn mk_job(id: i64, job_type: JobType, status: JobStatus, origin: JobOrigin) -> JobInfo {
    JobInfo {
        id,
        file_id: None,
        job_type,
        origin,
        name: "x".to_string(),
        version: None,
        status,
        started_at: Local.timestamp_millis_opt(1_700_000_000_000).unwrap(),
        completed_at: None,
        pipeline_run_id: None,
        logical_date: None,
        selection_snapshot_hash: None,
        quarantine_rows: None,
        items_total: 0,
        items_processed: 0,
        items_failed: 0,
        output_path: None,
        output_size_bytes: None,
        backtest: None,
        failures: vec![],
        violations: vec![],
        top_violations_loaded: false,
        selected_violation_index: 0,
    }
}

#[test]
fn local_scan_job_is_not_dropped_by_refresh() {
    let mut state = JobsState::default();

    // Insert ephemeral/local scan job (what add_scan_job does)
    let scan = mk_job(
        9_999_999_999_999,
        JobType::Scan,
        JobStatus::Running,
        JobOrigin::Ephemeral,
    );
    state.push_job(scan);

    // Loaded jobs from DB/control are Parse-only snapshot
    let loaded = vec![
        mk_job(1, JobType::Parse, JobStatus::Pending, JobOrigin::Persistent),
        mk_job(2, JobType::Parse, JobStatus::Running, JobOrigin::Persistent),
    ];

    state.merge_loaded_jobs(loaded);

    assert!(state.jobs.iter().any(|job| job.job_type == JobType::Scan));
    assert!(state.jobs.iter().any(|job| job.id == 1));
    assert!(state.jobs.iter().any(|job| job.id == 2));
}

#[test]
fn pending_started_at_does_not_jump_forward() {
    let mut state = JobsState::default();
    let mut old = mk_job(1, JobType::Parse, JobStatus::Pending, JobOrigin::Persistent);
    old.started_at = Local.timestamp_millis_opt(1_600_000_000_000).unwrap();
    state.jobs.push(old);

    // Simulate refresh that used Local::now fallback (later timestamp)
    let mut refreshed = mk_job(1, JobType::Parse, JobStatus::Pending, JobOrigin::Persistent);
    refreshed.started_at = Local.timestamp_millis_opt(1_700_000_000_000).unwrap();

    state.merge_loaded_jobs(vec![refreshed]);

    let after = state.jobs.iter().find(|job| job.id == 1).unwrap();
    assert_eq!(after.started_at.timestamp_millis(), 1_600_000_000_000);
}

#[test]
fn failures_preserved_when_refresh_missing_errors() {
    let mut state = JobsState::default();
    let mut old = mk_job(1, JobType::Parse, JobStatus::Failed, JobOrigin::Persistent);
    old.failures.push(JobFailure {
        file_path: "/tmp/a.csv".to_string(),
        error: "boom".to_string(),
        line: None,
    });
    state.jobs.push(old);

    let refreshed = mk_job(1, JobType::Parse, JobStatus::Failed, JobOrigin::Persistent);
    state.merge_loaded_jobs(vec![refreshed]);

    let after = state.jobs.iter().find(|job| job.id == 1).unwrap();
    assert!(!after.failures.is_empty());
}

#[test]
fn backtest_detail_preserved_when_refresh_missing() {
    let mut state = JobsState::default();
    let mut old = mk_job(2, JobType::Backtest, JobStatus::Completed, JobOrigin::Persistent);
    old.backtest = Some(BacktestInfo {
        pass_rate: 0.9,
        iteration: 1,
        high_failure_passed: 0,
    });
    state.jobs.push(old);

    let refreshed = mk_job(2, JobType::Backtest, JobStatus::Completed, JobOrigin::Persistent);
    state.merge_loaded_jobs(vec![refreshed]);

    let after = state.jobs.iter().find(|job| job.id == 2).unwrap();
    assert!(after.backtest.is_some());
}
