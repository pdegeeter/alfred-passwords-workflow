//! Tests for password styles, their configuration and generation.

use std::collections::HashMap;

use alfred_passwords::generator::{self, Config, Rules, Style};

/// A deterministic source cycling through `values`, clamped to the bound.
fn cycling(values: &'static [u32]) -> impl FnMut(u32) -> u32 {
    let mut index = 0;

    move |bound| {
        let value = values[index % values.len()] % bound;
        index += 1;

        value
    }
}

fn custom(length: usize, letters: bool, digits: bool, symbols: bool, similar: bool) -> Style {
    Style::Custom(Rules {
        length,
        letters,
        digits,
        symbols,
        similar,
    })
}

fn assert_apple_format(password: &str) {
    let groups: Vec<&str> = password.split('-').collect();
    assert_eq!(groups.len(), 3, "{password}");
    assert!(groups.iter().all(|group| group.len() == 6), "{password}");

    let chars: Vec<char> = password.chars().filter(|c| *c != '-').collect();
    assert_eq!(
        chars.iter().filter(|c| c.is_ascii_uppercase()).count(),
        1,
        "{password}"
    );
    assert_eq!(
        chars.iter().filter(|c| c.is_ascii_digit()).count(),
        1,
        "{password}"
    );
    assert!(
        !chars
            .iter()
            .any(|c| matches!(c.to_ascii_lowercase(), 'i' | 'l' | 'o') || matches!(c, '0' | '1')),
        "ambiguous character in {password}"
    );
}

#[test]
fn apple_passwords_have_the_apple_format() {
    let password =
        generator::generate_with(&Style::Apple, &mut cycling(&[0, 5, 11, 17, 22, 3])).unwrap();

    assert_apple_format(&password);
}

#[test]
fn apple_digit_and_uppercase_never_collide() {
    // Every draw returns 0: the digit lands on position 0 and the uppercase
    // letter must move to the next position.
    let password = generator::generate_with(&Style::Apple, &mut |_| 0).unwrap();

    assert_eq!(password, "2Aaaaa-aaaaaa-aaaaaa");
}

#[test]
fn apple_uppercase_offset_wraps_around_the_end() {
    // Letters draw 0, the digit lands on the last position (17), and the
    // uppercase offset is the largest one (16): (17 + 1 + 16) % 18 = 16.
    let mut draws = vec![0u32; 18];
    draws.extend([17, 16, 0]);
    let mut iter = draws.into_iter();

    let password =
        generator::generate_with(&Style::Apple, &mut |bound| iter.next().unwrap() % bound).unwrap();

    assert_eq!(password, "aaaaaa-aaaaaa-aaaaA2");
}

#[test]
fn custom_passwords_have_the_requested_length_and_every_enabled_set() {
    for length in [4, 20, 40] {
        let password = generator::generate(&custom(length, true, true, true, false)).unwrap();

        assert_eq!(password.chars().count(), length, "{password}");
        assert!(
            password.chars().any(|c| c.is_ascii_alphabetic()),
            "{password}"
        );
        assert!(password.chars().any(|c| c.is_ascii_digit()), "{password}");
        assert!(password.chars().any(|c| "@&$!#?".contains(c)), "{password}");
    }
}

#[test]
fn custom_passwords_only_use_enabled_sets() {
    let digits_only = generator::generate(&custom(30, false, true, false, true)).unwrap();
    assert!(
        digits_only.chars().all(|c| c.is_ascii_digit()),
        "{digits_only}"
    );

    let no_symbols = generator::generate(&custom(40, true, true, false, false)).unwrap();
    assert!(
        no_symbols.chars().all(|c| c.is_ascii_alphanumeric()),
        "{no_symbols}"
    );
}

#[test]
fn custom_passwords_leave_out_look_alikes_unless_allowed() {
    // Drawing index 0 from every set with look-alikes excluded can never
    // produce one: their first characters are `a`, `3` and `@`.
    let password =
        generator::generate_with(&custom(40, true, true, true, false), &mut |_| 0).unwrap();
    assert!(
        !password.chars().any(|c| "1lI|O0oZ2".contains(c)),
        "{password}"
    );

    // Over many draws, allowing them makes them appear.
    let allowed: String = (0..50)
        .map(|_| generator::generate(&custom(40, true, true, false, true)).unwrap())
        .collect();
    assert!(allowed.chars().any(|c| "1lIO0oZ2".contains(c)));
}

#[test]
fn invalid_styles_do_not_generate() {
    let no_sets = generator::generate(&custom(20, false, false, false, true)).unwrap_err();
    assert_eq!(no_sets.to_string(), "Enable letters, digits or symbols");

    let too_short = generator::generate(&custom(3, true, true, true, false)).unwrap_err();
    assert_eq!(too_short.to_string(), "The length must be between 4 and 40");
}

#[test]
fn generate_uses_the_system_random_source() {
    let first = generator::generate(&Style::Apple).unwrap();
    let second = generator::generate(&Style::Apple).unwrap();

    assert_apple_format(&first);
    assert_ne!(first, second);
}

#[test]
fn specs_round_trip() {
    for style in [
        Style::Apple,
        custom(4, true, false, false, false),
        custom(40, true, true, true, true),
        custom(12, false, true, true, false),
    ] {
        assert_eq!(Style::from_spec(&style.spec()).unwrap(), style);
    }

    assert_eq!(custom(24, true, true, false, true).spec(), "custom:24:lda");
}

#[test]
fn invalid_specs_are_rejected() {
    for spec in [
        "",
        "android",
        "custom:99:l",
        "custom:x:l",
        "custom:20:lz",
        "custom:20",
    ] {
        assert!(Style::from_spec(spec).is_err(), "{spec}");
    }
}

#[test]
fn summaries_describe_the_style() {
    assert_eq!(Style::Apple.summary(), "Apple format");
    assert_eq!(
        custom(24, true, true, false, false).summary(),
        "24 characters · Aa 345 · no look-alikes"
    );
    assert_eq!(
        custom(8, false, false, true, true).summary(),
        "8 characters · @&$ · look-alikes allowed"
    );
}

#[test]
fn config_defaults_to_apple_and_20_mixed_characters() {
    let config = Config::from_vars(|_| None);

    assert_eq!(config, Config::default());
    assert_eq!(config.style(), Style::Apple);
    assert_eq!(config.rules, Rules::default());
    assert_eq!(
        Rules::default(),
        Rules {
            length: 20,
            letters: true,
            digits: true,
            symbols: true,
            similar: false,
        }
    );
}

#[test]
fn config_reads_the_workflow_variables() {
    let vars: HashMap<&str, &str> = HashMap::from([
        ("PWG_STYLE", "custom"),
        ("PWG_LENGTH", "32"),
        ("PWG_LETTERS", "1"),
        ("PWG_DIGITS", "0"),
        ("PWG_SYMBOLS", "false"),
        ("PWG_SIMILAR", "true"),
    ]);

    let config = Config::from_vars(|name| vars.get(name).map(|value| (*value).to_owned()));

    assert_eq!(config.style(), custom(32, true, false, false, true));
}

#[test]
fn config_ignores_invalid_values() {
    let vars: HashMap<&str, &str> = HashMap::from([("PWG_LENGTH", "400"), ("PWG_DIGITS", "maybe")]);

    let config = Config::from_vars(|name| vars.get(name).map(|value| (*value).to_owned()));

    assert_eq!(config.rules.length, 20);
    assert!(config.rules.digits);
}
