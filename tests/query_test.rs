//! Tests for parsing the `pw` and `pwgen` queries.

use alfred_passwords::query::{self, GenerateQuery, SearchQuery};

#[test]
fn normalize_domain_strips_scheme_path_port_and_case() {
    assert_eq!(
        query::normalize_domain("HTTPS://GitHub.com:443/login?next=/"),
        Some("github.com".to_owned())
    );
    assert_eq!(
        query::normalize_domain("user@mail.example.co.uk"),
        Some("mail.example.co.uk".to_owned())
    );
}

#[test]
fn normalize_domain_rejects_what_is_not_a_domain_yet() {
    for input in [
        "github",
        "github.",
        "github.c",
        ".com",
        "git hub.com",
        "github.c0m",
        "",
    ] {
        assert_eq!(query::normalize_domain(input), None, "{input}");
    }
}

#[test]
fn parse_search_handles_empty_and_incomplete_queries() {
    assert_eq!(query::parse_search("   "), SearchQuery::Empty);
    assert_eq!(
        query::parse_search("githu"),
        SearchQuery::Incomplete("githu".to_owned())
    );
}

#[test]
fn parse_search_splits_domain_and_username_filter() {
    assert_eq!(
        query::parse_search("github.com  work  bot "),
        SearchQuery::Domain {
            domain: "github.com".to_owned(),
            filter: "work bot".to_owned(),
        }
    );
}

#[test]
fn parse_search_recognises_the_pin_commands() {
    assert_eq!(query::parse_search("pin"), SearchQuery::PinRequest);
    assert_eq!(query::parse_search("PIN 12"), SearchQuery::PinRequest);
    assert_eq!(
        query::parse_search("pin 123456"),
        SearchQuery::PinSubmit("123456".to_owned())
    );
    assert_eq!(query::parse_search("pin 12345a"), SearchQuery::PinRequest);
}

#[test]
fn parse_generate_without_a_domain_copies_only() {
    assert_eq!(query::parse_generate(""), GenerateQuery::CopyOnly);
    assert_eq!(query::parse_generate("githu"), GenerateQuery::CopyOnly);
}

#[test]
fn parse_generate_with_a_domain_waits_for_a_username() {
    assert_eq!(
        query::parse_generate("GitHub.com"),
        GenerateQuery::NeedsUsername {
            domain: "github.com".to_owned(),
        }
    );
}

#[test]
fn parse_generate_with_domain_and_username_saves() {
    assert_eq!(
        query::parse_generate("github.com me@example.com"),
        GenerateQuery::Save {
            domain: "github.com".to_owned(),
            username: "me@example.com".to_owned(),
        }
    );
}
