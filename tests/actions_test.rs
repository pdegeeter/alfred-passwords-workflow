//! Tests for encoding actions as Alfred variables and running them against
//! fake `apw` and clipboard doubles.

mod common;

use std::collections::HashMap;

use alfred_passwords::actions::{self, Action};
use alfred_passwords::apw::{Apw, ApwError};
use anyhow::{anyhow, Result};
use common::{FakeClipboard, FakeRunner, NOT_RUNNING, UNPAIRED};

const BIN: &str = "apw";

fn fixed_password() -> Result<String> {
    Ok("abcdef-ghjkmN-pqrst2".to_owned())
}

fn decode(variables: Vec<(&'static str, String)>) -> Result<Action> {
    let map: HashMap<&str, String> = variables.into_iter().collect();

    Action::from_variables(|name| map.get(name).cloned())
}

#[test]
fn every_action_round_trips_through_its_variables() {
    let account = || ("github.com".to_owned(), "me".to_owned());
    let all = [
        Action::CopyPassword {
            domain: account().0,
            username: account().1,
        },
        Action::CopyUsername {
            domain: account().0,
            username: account().1,
        },
        Action::Generate,
        Action::GenerateAndSave {
            domain: account().0,
            username: account().1,
        },
        Action::RequestPin,
        Action::SubmitPin("123456".to_owned()),
    ];

    for action in all {
        assert_eq!(decode(action.variables()).unwrap(), action);
    }
}

#[test]
fn from_variables_rejects_missing_or_unknown_values() {
    let missing = Action::from_variables(|_| None).unwrap_err();
    assert_eq!(missing.to_string(), "missing the `action` variable");

    let unknown = decode(vec![("action", "delete".to_owned())]).unwrap_err();
    assert_eq!(unknown.to_string(), "unknown action `delete`");

    let incomplete = decode(vec![
        ("action", "copy-password".to_owned()),
        ("domain", "github.com".to_owned()),
        ("username", String::new()),
    ])
    .unwrap_err();
    assert_eq!(incomplete.to_string(), "missing the `username` variable");
}

#[test]
fn copy_password_copies_a_concealed_password() {
    let runner = FakeRunner::new().with_success(
        "pw get github.com me",
        r#"{"results":[{"username":"me","password":"s3cret"}],"status":0}"#,
    );
    let clipboard = FakeClipboard::new();
    let action = Action::CopyPassword {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let message = actions::run(
        &action,
        &Apw::new(&runner, BIN),
        &clipboard,
        &fixed_password,
    )
    .unwrap();

    assert_eq!(message, "Password of me (github.com) copied");
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![("s3cret".to_owned(), true)]
    );
}

#[test]
fn copy_username_copies_without_calling_apw() {
    let runner = FakeRunner::new();
    let clipboard = FakeClipboard::new();
    let action = Action::CopyUsername {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let message = actions::run(
        &action,
        &Apw::new(&runner, BIN),
        &clipboard,
        &fixed_password,
    )
    .unwrap();

    assert_eq!(message, "Username me (github.com) copied");
    assert_eq!(*clipboard.copies.borrow(), vec![("me".to_owned(), false)]);
    assert!(runner.calls.borrow().is_empty());
}

#[test]
fn generate_copies_a_concealed_password() {
    let runner = FakeRunner::new();
    let clipboard = FakeClipboard::new();

    let message = actions::run(
        &Action::Generate,
        &Apw::new(&runner, BIN),
        &clipboard,
        &fixed_password,
    )
    .unwrap();

    assert_eq!(message, "Strong password copied");
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![("abcdef-ghjkmN-pqrst2".to_owned(), true)]
    );
}

#[test]
fn generate_and_save_saves_then_copies() {
    let runner = FakeRunner::new().with_success("pw save --stdin github.com me", r#"{"status":0}"#);
    let clipboard = FakeClipboard::new();
    let action = Action::GenerateAndSave {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let message = actions::run(
        &action,
        &Apw::new(&runner, BIN),
        &clipboard,
        &fixed_password,
    )
    .unwrap();

    assert_eq!(message, "Password saved for me (github.com) and copied");
    assert_eq!(
        runner.calls.borrow()[0].stdin.as_deref(),
        Some("abcdef-ghjkmN-pqrst2")
    );
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![("abcdef-ghjkmN-pqrst2".to_owned(), true)]
    );
}

#[test]
fn a_failed_save_copies_nothing() {
    let runner = FakeRunner::new().with_failure("pw save --stdin github.com me", UNPAIRED);
    let clipboard = FakeClipboard::new();
    let action = Action::GenerateAndSave {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let err = actions::run(
        &action,
        &Apw::new(&runner, BIN),
        &clipboard,
        &fixed_password,
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "apw is not paired with Apple Passwords: type `pw pin` to pair it"
    );
    assert!(clipboard.copies.borrow().is_empty());
}

#[test]
fn a_failed_generation_copies_nothing() {
    let runner = FakeRunner::new();
    let clipboard = FakeClipboard::new();
    let failing = || -> Result<String> { Err(anyhow!("no entropy")) };

    let err = actions::run(
        &Action::Generate,
        &Apw::new(&runner, BIN),
        &clipboard,
        &failing,
    )
    .unwrap_err();

    assert_eq!(err.to_string(), "no entropy");
    assert!(clipboard.copies.borrow().is_empty());
}

#[test]
fn pairing_actions_report_the_next_step() {
    let runner = FakeRunner::new()
        .with_success("auth request", r#"{"status":0}"#)
        .with_success("auth response --pin 123456", r#"{"status":0}"#);
    let apw = Apw::new(&runner, BIN);
    let clipboard = FakeClipboard::new();

    let requested = actions::run(&Action::RequestPin, &apw, &clipboard, &fixed_password).unwrap();
    let submitted = actions::run(
        &Action::SubmitPin("123456".to_owned()),
        &apw,
        &clipboard,
        &fixed_password,
    )
    .unwrap();

    assert_eq!(requested, "Type `pw pin <code>` with the PIN macOS shows");
    assert_eq!(submitted, "apw is paired with Apple Passwords");
    assert!(clipboard.copies.borrow().is_empty());
}

#[test]
fn explain_adds_the_fix_to_actionable_errors() {
    assert_eq!(
        actions::explain(ApwError::NotRunning).to_string(),
        "the apw daemon is not running: start it with `brew services start apw`, then type `pw pin`"
    );
    assert_eq!(
        actions::explain(ApwError::NotInstalled).to_string(),
        "apw is not installed: install it with `brew install bendews/homebrew-tap/apw`"
    );
    assert_eq!(
        actions::explain(ApwError::Failed("boom".to_owned())).to_string(),
        "boom"
    );
}

#[test]
fn copy_password_explains_a_stopped_daemon() {
    let runner = FakeRunner::new().with_failure("pw get github.com me", NOT_RUNNING);
    let clipboard = FakeClipboard::new();
    let action = Action::CopyPassword {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let err = actions::run(
        &action,
        &Apw::new(&runner, BIN),
        &clipboard,
        &fixed_password,
    )
    .unwrap_err();

    assert!(err.to_string().starts_with("the apw daemon is not running"));
    assert!(clipboard.copies.borrow().is_empty());
}
