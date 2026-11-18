//! Tests for the generated-password history model.

use alfred_passwords::history::{Entry, History};

fn entry(id: &str, password: &str) -> Entry {
    Entry {
        id: id.to_owned(),
        password: password.to_owned(),
        created_at: 1_800_000_000,
        style: "Apple format".to_owned(),
        saved_for: None,
    }
}

#[test]
fn push_keeps_the_newest_first_and_at_most_twenty() {
    let mut history = History::default();

    for n in 0..25 {
        history.push(entry(&format!("id{n}"), &format!("password{n}")));
    }

    let ids: Vec<&str> = history
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(ids.len(), History::MAX_ENTRIES);
    assert_eq!(ids.first(), Some(&"id24"));
    assert_eq!(ids.last(), Some(&"id5"));
}

#[test]
fn find_looks_entries_up_by_id() {
    let mut history = History::default();
    history.push(entry("a1", "first"));
    history.push(entry("b2", "second"));

    assert_eq!(
        history.find("a1").map(|entry| entry.password.as_str()),
        Some("first")
    );
    assert!(history.find("zz").is_none());
}

#[test]
fn preview_shows_four_characters_and_a_fixed_mask() {
    assert_eq!(History::preview("bevfzw-yuzdmg-edx4Fe"), "bevf****");
    // The mask does not reveal the length.
    assert_eq!(History::preview("abcdefghij"), "abcd****");
    assert_eq!(History::preview("é@1"), "é@1****");
}

#[test]
fn age_reads_naturally() {
    let now = 1_800_000_000;

    assert_eq!(History::age(now - 5, now), "just now");
    assert_eq!(History::age(now - 125, now), "2 min ago");
    assert_eq!(History::age(now - 3 * 3_600, now), "3 h ago");
    assert_eq!(History::age(now - 30 * 3_600, now), "yesterday");
    assert_eq!(History::age(now - 9 * 86_400, now), "9 days ago");
    // A clock that moved back is not "in the future".
    assert_eq!(History::age(now + 60, now), "just now");
}
