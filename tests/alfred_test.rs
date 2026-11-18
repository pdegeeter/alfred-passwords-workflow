//! Tests for the Alfred items.
//!
//! `powerpack::Item`'s fields are private, so we assert on its JSON shape — the
//! same JSON Alfred consumes.

use alfred_passwords::alfred;
use alfred_passwords::apw::{Account, ApwError};
use serde_json::{json, Value};

fn to_json(item: &powerpack::Item) -> Value {
    serde_json::to_value(item).expect("item should serialize")
}

fn account(username: &str, title: Option<&str>) -> Account {
    Account {
        username: username.to_owned(),
        domain: "github.com".to_owned(),
        title: title.map(str::to_owned),
    }
}

#[test]
fn account_items_copy_the_password_and_the_username_with_command() {
    let items = alfred::accounts("github.com", "", vec![account("me", Some("Work"))]);
    let item = to_json(&items[0]);

    assert_eq!(item["title"], "me");
    assert_eq!(
        item["subtitle"],
        "github.com · Work · ↵ copy password · ⌘↵ copy username"
    );
    assert_eq!(item["arg"], "me");
    assert_eq!(item["text"]["copy"], "me");
    assert_eq!(
        item["variables"],
        json!({"action": "copy-password", "domain": "github.com", "username": "me"})
    );
    assert_eq!(
        item["mods"]["cmd"]["variables"],
        json!({"action": "copy-username", "domain": "github.com", "username": "me"})
    );
    // The password itself never travels through Alfred.
    assert!(!item.to_string().contains("s3cret"));
    assert!(item.get("uid").is_none());
}

#[test]
fn account_items_are_filtered_on_username_and_title() {
    let accounts = vec![
        account("me@example.com", None),
        account("bot", Some("CI robot")),
    ];

    let by_title = alfred::accounts("github.com", "ROBOT", accounts.clone());
    assert_eq!(by_title.len(), 1);
    assert_eq!(to_json(&by_title[0])["title"], "bot");

    let by_username = alfred::accounts("github.com", "example me", accounts);
    assert_eq!(by_username.len(), 1);
    assert_eq!(to_json(&by_username[0])["title"], "me@example.com");
}

#[test]
fn no_matching_account_suggests_creating_one() {
    let items = alfred::accounts("github.com", "nobody", vec![account("me", None)]);
    let item = to_json(&items[0]);

    assert_eq!(item["title"], "No account found for github.com");
    assert_eq!(item["valid"], false);
}

#[test]
fn errors_say_how_to_fix_them() {
    let not_installed = to_json(&alfred::error(&ApwError::NotInstalled));
    assert_eq!(
        not_installed["subtitle"],
        "Install it with `brew install bendews/homebrew-tap/apw`"
    );
    assert_eq!(not_installed["valid"], false);

    let not_running = to_json(&alfred::error(&ApwError::NotRunning));
    assert_eq!(not_running["valid"], false);

    let failed = to_json(&alfred::error(&ApwError::Failed("boom".to_owned())));
    assert_eq!(failed["title"], "apw error");
    assert_eq!(failed["subtitle"], "boom");
}

#[test]
fn an_unpaired_daemon_offers_to_request_a_pin() {
    let item = to_json(&alfred::error(&ApwError::NotPaired));

    assert_eq!(item["variables"], json!({"action": "request-pin"}));
    assert!(item.get("valid").is_none());
}

#[test]
fn pin_items_encode_the_pairing_actions() {
    let request = to_json(&alfred::pin_request()[0]);
    assert_eq!(request["variables"], json!({"action": "request-pin"}));
    assert_eq!(request["autocomplete"], "pin ");

    let submit = to_json(&alfred::pin_submit("123456")[0]);
    assert_eq!(submit["title"], "Submit PIN 123456");
    assert_eq!(
        submit["variables"],
        json!({"action": "submit-pin", "pin": "123456"})
    );
}

#[test]
fn generate_item_hints_at_saving() {
    let plain = to_json(&alfred::generate(None));
    assert_eq!(plain["variables"], json!({"action": "generate"}));

    let with_domain = to_json(&alfred::generate(Some("github.com")));
    assert!(with_domain["subtitle"]
        .as_str()
        .unwrap()
        .ends_with("add a username to save it for github.com"));
}

#[test]
fn saving_a_new_account_is_the_default_action() {
    let items = alfred::generate_and_save("github.com", "me", Ok(false));
    let save = to_json(&items[0]);

    assert_eq!(save["title"], "Generate and save for me");
    assert_eq!(
        save["variables"],
        json!({"action": "generate-and-save", "domain": "github.com", "username": "me"})
    );
    assert_eq!(
        to_json(&items[1])["variables"],
        json!({"action": "generate"})
    );
}

#[test]
fn replacing_an_existing_password_needs_option() {
    let items = alfred::generate_and_save("github.com", "me", Ok(true));
    let save = to_json(&items[0]);

    assert_eq!(save["title"], "Replace the password of me");
    assert_eq!(save["valid"], false);
    assert!(save.get("variables").is_none());
    assert_eq!(save["mods"]["alt"]["valid"], true);
    assert_eq!(
        save["mods"]["alt"]["variables"],
        json!({"action": "generate-and-save", "domain": "github.com", "username": "me"})
    );
}

#[test]
fn saving_when_apw_fails_shows_the_error_and_still_offers_generation() {
    let items = alfred::generate_and_save("github.com", "me", Err(ApwError::NotRunning));

    assert_eq!(to_json(&items[0])["title"], "The apw daemon is not running");
    assert_eq!(
        to_json(&items[1])["variables"],
        json!({"action": "generate"})
    );
}

#[test]
fn hint_items_are_not_actionable() {
    let empty = alfred::empty_search(Vec::new());
    assert_eq!(empty.len(), 1);
    assert_eq!(to_json(&empty[0])["title"], "Search Apple Passwords");
    assert_eq!(to_json(&empty[0])["valid"], false);

    let incomplete = alfred::incomplete_search("githu", Vec::new());
    assert_eq!(incomplete.len(), 1);
    assert_eq!(
        to_json(&incomplete[0])["title"],
        "Keep typing a domain: githu…"
    );
    assert_eq!(to_json(&incomplete[0])["valid"], false);
}

#[test]
fn domain_suggestions_complete_the_query_to_the_domain() {
    let items = alfred::domain_suggestions(vec!["github.com".to_owned()]);
    let item = to_json(&items[0]);

    assert_eq!(item["title"], "github.com");
    assert_eq!(item["subtitle"], "↵ or ⇥ to list its accounts");
    // The trailing space makes the completed query a domain lookup.
    assert_eq!(item["autocomplete"], "github.com ");
    // Invalid, so ↵ completes the query instead of running an action.
    assert_eq!(item["valid"], false);
    assert!(item.get("variables").is_none());
}

#[test]
fn an_empty_search_lists_known_domains_before_the_hint() {
    let items = alfred::empty_search(vec!["github.com".to_owned(), "gitlab.com".to_owned()]);
    let titles: Vec<Value> = items
        .iter()
        .map(|item| to_json(item)["title"].clone())
        .collect();

    assert_eq!(
        titles,
        vec![
            json!("github.com"),
            json!("gitlab.com"),
            json!("Search Apple Passwords")
        ]
    );
}

#[test]
fn an_incomplete_search_lists_matching_domains_instead_of_the_hint() {
    let items = alfred::incomplete_search("git", vec!["github.com".to_owned()]);

    assert_eq!(items.len(), 1);
    assert_eq!(to_json(&items[0])["title"], "github.com");
}
