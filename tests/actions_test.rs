//! Tests for encoding actions as Alfred variables and running them against
//! fake `apw`, clipboard and history doubles.

mod common;

use std::collections::HashMap;

use alfred_passwords::actions::{self, Action, Context};
use alfred_passwords::apw::{Apw, ApwError};
use alfred_passwords::generator::{Rules, Style};
use alfred_passwords::history::{Entry, History};
use anyhow::{anyhow, Result};
use common::{FakeClipboard, FakeRunner, FakeVault, LoadMode, NOT_RUNNING, UNPAIRED};

const BIN: &str = "apw";
const NOW: u64 = 1_800_000_000;
const GENERATED: &str = "abcdef-ghjkmN-pqrst2";

fn fixed_password(_style: &Style) -> Result<String> {
    Ok(GENERATED.to_owned())
}

fn fixed_id() -> Result<String> {
    Ok("0011223344556677".to_owned())
}

fn custom() -> Style {
    Style::Custom(Rules {
        length: 24,
        letters: true,
        digits: true,
        symbols: false,
        similar: false,
    })
}

/// Run `action` with the given doubles and the fixed generator.
fn run(
    action: &Action,
    runner: &FakeRunner,
    clipboard: &FakeClipboard,
    vault: &FakeVault,
) -> Result<String> {
    let apw = Apw::new(runner, BIN);
    let ctx = Context {
        apw: &apw,
        clipboard,
        generate: &fixed_password,
        history: vault,
        new_id: &fixed_id,
        now: NOW,
    };

    actions::run(action, &ctx)
}

fn decode(variables: Vec<(&'static str, String)>) -> Result<Action> {
    let map: HashMap<&str, String> = variables.into_iter().collect();

    Action::from_variables(|name| map.get(name).cloned())
}

fn account() -> (String, String) {
    ("github.com".to_owned(), "me".to_owned())
}

#[test]
fn every_action_round_trips_through_its_variables() {
    let all = [
        Action::CopyPassword {
            domain: account().0,
            username: account().1,
        },
        Action::CopyUsername {
            domain: account().0,
            username: account().1,
        },
        Action::Generate {
            style: Style::Apple,
        },
        Action::Generate { style: custom() },
        Action::GenerateAndSave {
            domain: account().0,
            username: account().1,
            style: custom(),
        },
        Action::CopyGenerated {
            id: "a1b2".to_owned(),
        },
        Action::RequestPin,
        Action::SubmitPin("123456".to_owned()),
    ];

    for action in all {
        assert_eq!(decode(action.variables()).unwrap(), action);
    }
}

#[test]
fn only_account_actions_have_a_domain() {
    let account = Action::CopyUsername {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    assert_eq!(account.domain(), Some("github.com"));
    assert_eq!(
        Action::Generate {
            style: Style::Apple
        }
        .domain(),
        None
    );
    assert_eq!(Action::SubmitPin("123456".to_owned()).domain(), None);
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

    let bad_style = decode(vec![
        ("action", "generate".to_owned()),
        ("style", "custom:99:l".to_owned()),
    ])
    .unwrap_err();
    assert_eq!(
        bad_style.to_string(),
        "invalid password length in `custom:99:l`"
    );
}

#[test]
fn copy_password_copies_a_concealed_password() {
    let runner = FakeRunner::new().with_success(
        "pw get github.com me",
        r#"{"results":[{"username":"me","password":"s3cret"}],"status":0}"#,
    );
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());
    let action = Action::CopyPassword {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let message = run(&action, &runner, &clipboard, &vault).unwrap();

    assert_eq!(message, "Password of me (github.com) copied");
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![("s3cret".to_owned(), true)]
    );
    // Fetched passwords are not generated ones: not kept in the history.
    assert_eq!(vault.saves.get(), 0);
}

#[test]
fn copy_username_copies_without_calling_apw() {
    let runner = FakeRunner::new();
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());
    let action = Action::CopyUsername {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let message = run(&action, &runner, &clipboard, &vault).unwrap();

    assert_eq!(message, "Username me (github.com) copied");
    assert_eq!(*clipboard.copies.borrow(), vec![("me".to_owned(), false)]);
    assert!(runner.calls.borrow().is_empty());
}

#[test]
fn generate_copies_a_concealed_password_and_keeps_it() {
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());

    let message = run(
        &Action::Generate { style: custom() },
        &FakeRunner::new(),
        &clipboard,
        &vault,
    )
    .unwrap();

    assert_eq!(message, "Strong password copied");
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![(GENERATED.to_owned(), true)]
    );
    assert_eq!(
        vault.history.borrow().entries(),
        &[Entry {
            id: "0011223344556677".to_owned(),
            password: GENERATED.to_owned(),
            created_at: NOW,
            style: "24 characters · Aa 345 · no look-alikes".to_owned(),
            saved_for: None,
        }]
    );
}

#[test]
fn generate_and_save_saves_copies_and_keeps_it() {
    let runner = FakeRunner::new().with_success("pw save --stdin github.com me", r#"{"status":0}"#);
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());
    let action = Action::GenerateAndSave {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
        style: Style::Apple,
    };

    let message = run(&action, &runner, &clipboard, &vault).unwrap();

    assert_eq!(message, "Password saved for me (github.com) and copied");
    assert_eq!(runner.calls.borrow()[0].stdin.as_deref(), Some(GENERATED));
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![(GENERATED.to_owned(), true)]
    );
    assert_eq!(
        vault.history.borrow().entries()[0].saved_for.as_deref(),
        Some("me (github.com)")
    );
}

#[test]
fn a_failed_save_copies_and_keeps_nothing() {
    let runner = FakeRunner::new().with_failure("pw save --stdin github.com me", UNPAIRED);
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());
    let action = Action::GenerateAndSave {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
        style: Style::Apple,
    };

    let err = run(&action, &runner, &clipboard, &vault).unwrap_err();

    assert_eq!(
        err.to_string(),
        "apw is not paired with Apple Passwords: type `pw pin` to pair it"
    );
    assert!(clipboard.copies.borrow().is_empty());
    assert_eq!(vault.saves.get(), 0);
}

#[test]
fn a_failed_generation_copies_nothing() {
    let runner = FakeRunner::new();
    let apw = Apw::new(&runner, BIN);
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());
    let failing = |_: &Style| -> Result<String> { Err(anyhow!("no entropy")) };
    let ctx = Context {
        apw: &apw,
        clipboard: &clipboard,
        generate: &failing,
        history: &vault,
        new_id: &fixed_id,
        now: NOW,
    };

    let err = actions::run(
        &Action::Generate {
            style: Style::Apple,
        },
        &ctx,
    )
    .unwrap_err();

    assert_eq!(err.to_string(), "no entropy");
    assert!(clipboard.copies.borrow().is_empty());
}

#[test]
fn a_history_failure_still_copies_the_password() {
    let clipboard = FakeClipboard::new();
    let mut vault = FakeVault::new(History::default());
    vault.load_mode = LoadMode::Fails;

    let message = run(
        &Action::Generate {
            style: Style::Apple,
        },
        &FakeRunner::new(),
        &clipboard,
        &vault,
    )
    .unwrap();

    assert_eq!(
        message,
        "Strong password copied (not kept in history: the user denied Keychain access)"
    );
    assert_eq!(clipboard.copies.borrow().len(), 1);
    assert_eq!(vault.saves.get(), 0);
}

#[test]
fn an_unreadable_history_is_replaced() {
    let clipboard = FakeClipboard::new();
    let mut vault = FakeVault::new(History::default());
    vault.load_mode = LoadMode::Unreadable;

    let message = run(
        &Action::Generate {
            style: Style::Apple,
        },
        &FakeRunner::new(),
        &clipboard,
        &vault,
    )
    .unwrap();

    assert_eq!(message, "Strong password copied");
    assert_eq!(vault.history.borrow().entries().len(), 1);
}

#[test]
fn copy_generated_copies_the_kept_password_again() {
    let mut history = History::default();
    history.push(Entry {
        id: "a1b2".to_owned(),
        password: "bevfzw-yuzdmg-edx4Fe".to_owned(),
        created_at: NOW,
        style: "Apple format".to_owned(),
        saved_for: None,
    });
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(history);

    let message = run(
        &Action::CopyGenerated {
            id: "a1b2".to_owned(),
        },
        &FakeRunner::new(),
        &clipboard,
        &vault,
    )
    .unwrap();

    assert_eq!(message, "Password bevf**** copied again");
    assert_eq!(
        *clipboard.copies.borrow(),
        vec![("bevfzw-yuzdmg-edx4Fe".to_owned(), true)]
    );
    assert_eq!(vault.saves.get(), 0);
}

#[test]
fn copy_generated_reports_an_expired_entry() {
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());

    let err = run(
        &Action::CopyGenerated {
            id: "gone".to_owned(),
        },
        &FakeRunner::new(),
        &clipboard,
        &vault,
    )
    .unwrap_err();

    assert_eq!(err.to_string(), "this password is no longer in the history");
    assert!(clipboard.copies.borrow().is_empty());
}

#[test]
fn pairing_actions_report_the_next_step() {
    let runner = FakeRunner::new()
        .with_success("auth request", r#"{"status":0}"#)
        .with_success("auth response --pin 123456", r#"{"status":0}"#);
    let clipboard = FakeClipboard::new();
    let vault = FakeVault::new(History::default());

    let requested = run(&Action::RequestPin, &runner, &clipboard, &vault).unwrap();
    let submitted = run(
        &Action::SubmitPin("123456".to_owned()),
        &runner,
        &clipboard,
        &vault,
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
    let vault = FakeVault::new(History::default());
    let action = Action::CopyPassword {
        domain: "github.com".to_owned(),
        username: "me".to_owned(),
    };

    let err = run(&action, &runner, &clipboard, &vault).unwrap_err();

    assert!(err.to_string().starts_with("the apw daemon is not running"));
    assert!(clipboard.copies.borrow().is_empty());
}
