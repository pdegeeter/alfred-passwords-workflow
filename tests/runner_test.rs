//! Tests for `SystemRunner` against throwaway shell scripts.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alfred_passwords::runner::{CommandRunner, NotFound, SystemRunner, TimedOut};

const TIMEOUT: Duration = Duration::from_secs(5);

fn script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("fake-apw");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    path
}

#[test]
fn run_captures_stdout_and_passes_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), r#"echo "args: $*""#);

    let output = SystemRunner
        .run(
            program.to_str().unwrap(),
            &["pw", "list", "github.com"],
            None,
            TIMEOUT,
        )
        .unwrap();

    assert!(output.success);
    assert_eq!(output.stdout, "args: pw list github.com\n");
}

#[test]
fn run_feeds_stdin_and_closes_it() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "cat");

    let output = SystemRunner
        .run(program.to_str().unwrap(), &[], Some("s3cret"), TIMEOUT)
        .unwrap();

    assert_eq!(output.stdout, "s3cret");
}

#[test]
fn run_reports_a_failure_with_its_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), r#"echo '{"status":9}' >&2; exit 9"#);

    let output = SystemRunner
        .run(program.to_str().unwrap(), &[], None, TIMEOUT)
        .unwrap();

    assert!(!output.success);
    assert_eq!(output.stderr, "{\"status\":9}\n");
}

#[test]
fn run_reports_a_missing_program() {
    let err = SystemRunner
        .run("/nonexistent/apw", &[], None, TIMEOUT)
        .unwrap_err();

    assert!(err.downcast_ref::<NotFound>().is_some());
}

#[test]
fn run_kills_a_command_that_outlives_its_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "exec sleep 30");
    let started = Instant::now();

    let err = SystemRunner
        .run(
            program.to_str().unwrap(),
            &[],
            None,
            Duration::from_millis(200),
        )
        .unwrap_err();

    assert!(err.downcast_ref::<TimedOut>().is_some());
    assert!(started.elapsed() < Duration::from_secs(5));
}
