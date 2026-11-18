//! Tests for parsing the `pw` and `pwg` queries.

use alfred_passwords::generator::{Config, Rules, Style};
use alfred_passwords::query::{self, Options, SearchQuery, Target};

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

fn rules(length: usize, letters: bool, digits: bool, symbols: bool, similar: bool) -> Rules {
    Rules {
        length,
        letters,
        digits,
        symbols,
        similar,
    }
}

#[test]
fn parse_generate_without_a_domain_copies_only() {
    for input in ["", "githu"] {
        let parsed = query::parse_generate(input);

        assert_eq!(parsed.target, Target::CopyOnly, "{input}");
        assert_eq!(parsed.options, Options::default(), "{input}");
        assert!(parsed.problems.is_empty(), "{input}");
    }
}

#[test]
fn parse_generate_with_a_domain_waits_for_a_username() {
    assert_eq!(
        query::parse_generate("GitHub.com").target,
        Target::NeedsUsername {
            domain: "github.com".to_owned(),
        }
    );
}

#[test]
fn parse_generate_with_domain_and_username_saves() {
    assert_eq!(
        query::parse_generate("github.com me@example.com").target,
        Target::Save {
            domain: "github.com".to_owned(),
            username: "me@example.com".to_owned(),
        }
    );
}

#[test]
fn parse_generate_reads_options_anywhere() {
    let parsed = query::parse_generate("24 -s github.com +a me");

    assert_eq!(
        parsed.options,
        Options {
            length: Some(24),
            symbols: Some(false),
            similar: Some(true),
            ..Options::default()
        }
    );
    assert_eq!(
        parsed.target,
        Target::Save {
            domain: "github.com".to_owned(),
            username: "me".to_owned(),
        }
    );
}

#[test]
fn parse_generate_reports_invalid_options() {
    let parsed = query::parse_generate("3 +x -d 99");

    assert_eq!(
        parsed.problems,
        vec![
            "Length 3 is out of range: 4–40".to_owned(),
            "Unknown option +x: use ±l, ±d, ±s or ±a".to_owned(),
            "Length 99 is out of range: 4–40".to_owned(),
        ]
    );
    assert_eq!(parsed.options.digits, Some(false));
    assert_eq!(parsed.options.length, None);
}

#[test]
fn words_that_only_start_like_options_are_not_options() {
    // `-ab` is not an option: as the first word after the domain, it is the
    // username.
    let parsed = query::parse_generate("github.com -ab");

    assert!(parsed.problems.is_empty());
    assert_eq!(
        parsed.target,
        Target::Save {
            domain: "github.com".to_owned(),
            username: "-ab".to_owned(),
        }
    );
}

#[test]
fn without_options_the_configured_style_applies() {
    let apple = Config::default();
    let custom = Config {
        apple: false,
        rules: rules(16, true, true, false, false),
    };

    assert_eq!(Options::default().apply(&apple), Style::Apple);
    assert_eq!(
        Options::default().apply(&custom),
        Style::Custom(rules(16, true, true, false, false))
    );
}

#[test]
fn apple_option_picks_the_apple_format() {
    let custom = Config {
        apple: false,
        rules: rules(16, true, true, false, false),
    };

    assert_eq!(
        query::parse_generate("apple").options.apply(&custom),
        Style::Apple
    );
}

#[test]
fn custom_options_start_from_the_configured_rules_even_in_apple_mode() {
    let config = Config {
        apple: true,
        rules: rules(16, true, true, false, false),
    };

    assert_eq!(
        query::parse_generate("32 +s").options.apply(&config),
        Style::Custom(rules(32, true, true, true, false))
    );
    // A custom option wins over `apple`.
    assert_eq!(
        query::parse_generate("apple -d").options.apply(&config),
        Style::Custom(rules(16, true, false, false, false))
    );
}
