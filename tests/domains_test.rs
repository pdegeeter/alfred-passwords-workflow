//! Tests for the local index of looked-up domains.

use std::fs;
use std::path::Path;

use alfred_passwords::domains::DomainIndex;

const DAY: u64 = 86_400;
const NOW: u64 = 1_800_000_000;

fn index_with(domains: &[(&str, u32, u64)]) -> DomainIndex {
    let mut index = DomainIndex::default();

    for (domain, uses, last_seen) in domains {
        index.remember(domain, *last_seen);

        for _ in 0..*uses {
            index.record_use(domain, *last_seen);
        }
    }

    index
}

#[test]
fn path_is_in_the_workflow_data_folder() {
    assert_eq!(
        DomainIndex::path(Some(Path::new("/data"))),
        Some(Path::new("/data/domains.json").to_path_buf())
    );
    assert_eq!(DomainIndex::path(Some(Path::new(""))), None);
    assert_eq!(DomainIndex::path(None), None);
}

#[test]
fn remember_adds_a_domain_once() {
    let mut index = DomainIndex::default();

    assert!(index.remember("github.com", NOW));
    assert!(!index.remember("github.com", NOW + DAY));

    let entry = &index.entries()["github.com"];
    assert_eq!(entry.uses, 0);
    assert_eq!(entry.last_seen, NOW);
}

#[test]
fn observe_remembers_domains_with_accounts_and_forgets_the_others() {
    let mut index = DomainIndex::default();

    assert!(index.observe("github.com", true, NOW));
    assert!(!index.observe("github.com", true, NOW));
    assert!(!index.observe("github.co", false, NOW));
    assert!(index.observe("github.com", false, NOW));
    assert!(index.entries().is_empty());
}

#[test]
fn record_use_counts_and_refreshes_the_domain() {
    let mut index = DomainIndex::default();

    index.record_use("github.com", NOW);
    index.record_use("github.com", NOW + DAY);
    index.record_use("github.com", NOW);

    let entry = &index.entries()["github.com"];
    assert_eq!(entry.uses, 3);
    assert_eq!(entry.last_seen, NOW + DAY);
}

#[test]
fn suggest_matches_a_case_insensitive_substring() {
    let index = index_with(&[
        ("github.com", 0, NOW),
        ("gitlab.com", 0, NOW),
        ("aws.amazon.com", 0, NOW),
    ]);

    assert_eq!(
        index.suggest("GIT", NOW, 10),
        vec!["github.com", "gitlab.com"]
    );
    assert_eq!(index.suggest("hub.c", NOW, 10), vec!["github.com"]);
    assert!(index.suggest("bitbucket", NOW, 10).is_empty());
}

#[test]
fn suggest_ranks_label_prefixes_before_inner_matches() {
    let index = index_with(&[("github.com", 50, NOW), ("hub.docker.com", 0, NOW)]);

    assert_eq!(
        index.suggest("hub", NOW, 10),
        vec!["hub.docker.com", "github.com"]
    );
}

#[test]
fn suggest_ranks_by_uses_then_recency() {
    let index = index_with(&[
        ("rarely.com", 1, NOW),
        ("often.com", 8, NOW),
        ("stale.com", 8, NOW - 120 * DAY),
    ]);

    // often: 9; rarely: 2; stale: 9 halved four times = 0.56.
    assert_eq!(
        index.suggest("", NOW, 10),
        vec!["often.com", "rarely.com", "stale.com"]
    );
}

#[test]
fn suggest_breaks_ties_alphabetically_and_honours_the_limit() {
    let index = index_with(&[("b.com", 0, NOW), ("a.com", 0, NOW), ("c.com", 0, NOW)]);

    assert_eq!(index.suggest("", NOW, 2), vec!["a.com", "b.com"]);
}

#[test]
fn save_then_load_round_trips_and_creates_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/domains.json");
    let index = index_with(&[("github.com", 2, NOW)]);

    index.save(&path).unwrap();

    assert_eq!(DomainIndex::load(&path), index);
    assert!(!path.with_extension("json.tmp").exists());
}

#[test]
fn the_file_stores_domains_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("domains.json");

    index_with(&[("github.com", 1, NOW)]).save(&path).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"domains": {"github.com": {"uses": 1, "last_seen": NOW}}})
    );
}

#[test]
fn a_missing_or_malformed_file_is_an_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    let malformed = dir.path().join("domains.json");
    fs::write(&malformed, "{not json").unwrap();

    assert_eq!(
        DomainIndex::load(&dir.path().join("missing.json")),
        DomainIndex::default()
    );
    assert_eq!(DomainIndex::load(&malformed), DomainIndex::default());
}
