//! Tests for the `apw` CLI client: the commands it runs and how it decodes
//! `apw`'s JSON results and failures.

mod common;

use std::path::Path;

use alfred_passwords::apw::{Account, Apw, ApwError};
use common::{FakeRunner, NOT_RUNNING, NO_RESULTS, UNPAIRED};

const BIN: &str = "/opt/homebrew/bin/apw";

#[test]
fn list_accounts_parses_usernames_domains_and_titles() {
    let runner = FakeRunner::new().with_success(
        "pw list github.com",
        r#"{"results":[
            {"username":"me@example.com","domain":"github.com","sites":["github.com"]},
            {"username":"bot","domain":"github.com","title":"CI bot"}
        ],"status":0}"#,
    );

    let accounts = Apw::new(&runner, BIN).list_accounts("github.com").unwrap();

    assert_eq!(
        accounts,
        vec![
            Account {
                username: "me@example.com".to_owned(),
                domain: "github.com".to_owned(),
                title: None,
            },
            Account {
                username: "bot".to_owned(),
                domain: "github.com".to_owned(),
                title: Some("CI bot".to_owned()),
            },
        ]
    );
    assert_eq!(runner.calls.borrow()[0].program, BIN);
}

#[test]
fn list_accounts_falls_back_to_the_queried_domain() {
    let runner = FakeRunner::new().with_success(
        "pw list github.com",
        r#"{"results":[{"username":"me"}],"status":0}"#,
    );

    let accounts = Apw::new(&runner, BIN).list_accounts("github.com").unwrap();

    assert_eq!(accounts[0].domain, "github.com");
}

#[test]
fn list_accounts_treats_no_results_as_empty() {
    let runner = FakeRunner::new().with_failure("pw list example.org", NO_RESULTS);

    let accounts = Apw::new(&runner, BIN).list_accounts("example.org").unwrap();

    assert!(accounts.is_empty());
}

#[test]
fn an_unreachable_daemon_is_not_running() {
    let runner = FakeRunner::new().with_failure("pw list github.com", NOT_RUNNING);

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert_eq!(err, ApwError::NotRunning);
}

#[test]
fn an_unpaired_daemon_is_not_paired() {
    let runner = FakeRunner::new().with_failure("pw list github.com", UNPAIRED);

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert_eq!(err, ApwError::NotPaired);
}

#[test]
fn a_missing_binary_is_not_installed() {
    let runner = FakeRunner::new().with_missing_program("pw list github.com");

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert_eq!(err, ApwError::NotInstalled);
}

#[test]
fn a_hung_apw_times_out() {
    let runner = FakeRunner::new().with_timeout("pw list github.com");

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert_eq!(err, ApwError::TimedOut);
}

#[test]
fn other_failures_keep_apw_message() {
    let runner = FakeRunner::new().with_failure(
        "pw list github.com",
        r#"{"error":"Invalid message format","status":6,"results":[]}"#,
    );

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert_eq!(err, ApwError::Failed("Invalid message format".to_owned()));
}

#[test]
fn non_json_failures_keep_stderr() {
    let runner = FakeRunner::new().with_failure("pw list github.com", "Segmentation fault\n");

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert_eq!(err, ApwError::Failed("Segmentation fault".to_owned()));
}

#[test]
fn unexpected_success_output_is_an_error() {
    let runner = FakeRunner::new().with_success("pw list github.com", "not json");

    let err = Apw::new(&runner, BIN)
        .list_accounts("github.com")
        .unwrap_err();

    assert!(
        matches!(err, ApwError::Failed(message) if message.starts_with("unexpected apw output"))
    );
}

#[test]
fn password_returns_the_requested_username_password() {
    let runner = FakeRunner::new().with_success(
        "pw get github.com me",
        r#"{"results":[
            {"username":"other","domain":"github.com","password":"wrong"},
            {"username":"me","domain":"github.com","password":"s3cret"}
        ],"status":0}"#,
    );

    let password = Apw::new(&runner, BIN).password("github.com", "me").unwrap();

    assert_eq!(password, "s3cret");
}

#[test]
fn password_falls_back_to_the_only_returned_password() {
    let runner = FakeRunner::new().with_success(
        "pw get github.com me",
        r#"{"results":[{"username":"Me","domain":"github.com","password":"s3cret"}],"status":0}"#,
    );

    let password = Apw::new(&runner, BIN).password("github.com", "me").unwrap();

    assert_eq!(password, "s3cret");
}

#[test]
fn password_without_any_password_is_an_error() {
    let runner = FakeRunner::new().with_success(
        "pw get github.com me",
        r#"{"results":[{"username":"me","domain":"github.com"}],"status":0}"#,
    );

    let err = Apw::new(&runner, BIN)
        .password("github.com", "me")
        .unwrap_err();

    assert_eq!(
        err,
        ApwError::Failed("No password found for me on github.com".to_owned())
    );
}

#[test]
fn password_with_no_results_is_an_error() {
    let runner = FakeRunner::new().with_failure("pw get github.com me", NO_RESULTS);

    let err = Apw::new(&runner, BIN)
        .password("github.com", "me")
        .unwrap_err();

    assert_eq!(
        err,
        ApwError::Failed("No password found for me on github.com".to_owned())
    );
}

#[test]
fn save_sends_the_password_on_stdin_only() {
    let runner = FakeRunner::new().with_success("pw save --stdin github.com me", r#"{"status":0}"#);

    Apw::new(&runner, BIN)
        .save("github.com", "me", "s3cret")
        .unwrap();

    let calls = runner.calls.borrow();
    assert_eq!(calls[0].stdin.as_deref(), Some("s3cret"));
    assert!(!calls[0].args.iter().any(|arg| arg.contains("s3cret")));
}

#[test]
fn save_reports_apw_failures() {
    let runner = FakeRunner::new().with_failure("pw save --stdin github.com me", UNPAIRED);

    let err = Apw::new(&runner, BIN)
        .save("github.com", "me", "s3cret")
        .unwrap_err();

    assert_eq!(err, ApwError::NotPaired);
}

#[test]
fn pairing_runs_auth_request_then_response() {
    let runner = FakeRunner::new()
        .with_success("auth request", r#"{"status":0}"#)
        .with_success("auth response --pin 123456", r#"{"status":0}"#);
    let apw = Apw::new(&runner, BIN);

    apw.request_pin().unwrap();
    apw.submit_pin("123456").unwrap();

    let args: Vec<Vec<String>> = runner
        .calls
        .borrow()
        .iter()
        .map(|call| call.args.clone())
        .collect();
    assert_eq!(
        args,
        vec![
            vec!["auth", "request"],
            vec!["auth", "response", "--pin", "123456"],
        ]
    );
}

#[test]
fn resolve_bin_prefers_the_configured_path() {
    let bin = Apw::resolve_bin(Some(" /custom/apw "), |_| true);

    assert_eq!(bin, "/custom/apw");
}

#[test]
fn resolve_bin_ignores_a_blank_setting() {
    let bin = Apw::resolve_bin(Some("  "), |path| path == Path::new("/usr/local/bin/apw"));

    assert_eq!(bin, "/usr/local/bin/apw");
}

#[test]
fn resolve_bin_prefers_apple_silicon_homebrew() {
    let bin = Apw::resolve_bin(None, |_| true);

    assert_eq!(bin, "/opt/homebrew/bin/apw");
}

#[test]
fn resolve_bin_falls_back_to_path_lookup() {
    let bin = Apw::resolve_bin(None, |_| false);

    assert_eq!(bin, "apw");
}
