//! Tests for the Apple-style password generator.

use alfred_passwords::generator;

/// A deterministic source cycling through `values`, clamped to the bound.
fn cycling(values: &'static [u32]) -> impl FnMut(u32) -> u32 {
    let mut index = 0;

    move |bound| {
        let value = values[index % values.len()] % bound;
        index += 1;

        value
    }
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
        chars.iter().all(|c| c.is_ascii_alphanumeric()),
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
fn generate_with_produces_the_apple_format() {
    let password = generator::generate_with(&mut cycling(&[0, 5, 11, 17, 22, 3]));

    assert_apple_format(&password);
}

#[test]
fn generate_with_is_deterministic_for_a_given_source() {
    let first = generator::generate_with(&mut cycling(&[7, 1, 9]));
    let second = generator::generate_with(&mut cycling(&[7, 1, 9]));

    assert_eq!(first, second);
}

#[test]
fn the_digit_and_the_uppercase_letter_never_collide() {
    // Every draw returns 0: the digit lands on position 0 and the uppercase
    // letter must move to the next position.
    let password = generator::generate_with(&mut |_| 0);

    assert_eq!(password, "2Aaaaa-aaaaaa-aaaaaa");
}

#[test]
fn the_uppercase_offset_wraps_around_the_end() {
    // Letters draw 0, the digit lands on the last position (17), and the
    // uppercase offset is the largest one (16): (17 + 1 + 16) % 18 = 16, right
    // before the digit, never on it.
    let mut draws = vec![0u32; 18];
    draws.extend([17, 16, 0]);
    let mut iter = draws.into_iter();

    let password = generator::generate_with(&mut |bound| iter.next().unwrap() % bound);

    assert_eq!(password, "aaaaaa-aaaaaa-aaaaA2");
}

#[test]
fn generate_uses_the_system_random_source() {
    let first = generator::generate().unwrap();
    let second = generator::generate().unwrap();

    assert_apple_format(&first);
    assert_apple_format(&second);
    assert_ne!(first, second);
}
