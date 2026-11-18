//! Strong password generation in the format Apple Passwords suggests:
//! three groups of six characters joined by dashes (`abcdef-ghijkm-npqrst`),
//! lowercase letters with exactly one uppercase letter and one digit.
//!
//! Ambiguous characters (`i`, `l`, `o`, `0`, `1`) are left out so the password
//! can be read aloud or retyped. That leaves about 88 bits of entropy.

use anyhow::{anyhow, Result};

/// Lowercase letters, without `i`, `l` and `o`.
const LETTERS: &[u8] = b"abcdefghjkmnpqrstuvwxyz";

/// Digits, without `0` and `1`.
const DIGITS: &[u8] = b"23456789";

/// Number of groups and characters per group.
const GROUPS: usize = 3;
const GROUP_LEN: usize = 6;

/// Generate a password from the operating system's secure random source.
pub fn generate() -> Result<String> {
    let mut failure = None;
    let password = generate_with(&mut |bound| match uniform(bound) {
        Ok(value) => value,
        Err(err) => {
            failure.get_or_insert(err);

            0
        }
    });

    match failure {
        Some(err) => Err(err),
        None => Ok(password),
    }
}

/// Generate a password from `random`, which must return a uniformly
/// distributed value in `0..bound`.
pub fn generate_with(random: &mut dyn FnMut(u32) -> u32) -> String {
    let len = GROUPS * GROUP_LEN;
    let mut chars: Vec<u8> = (0..len)
        .map(|_| LETTERS[random(LETTERS.len() as u32) as usize])
        .collect();

    let digit_at = random(len as u32) as usize;
    let upper_at = (digit_at + 1 + random(len as u32 - 1) as usize) % len;

    chars[digit_at] = DIGITS[random(DIGITS.len() as u32) as usize];
    chars[upper_at] = chars[upper_at].to_ascii_uppercase();

    chars
        .chunks(GROUP_LEN)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// A uniform value in `0..bound` from the OS random source, by rejection
/// sampling so no value is favoured by a modulo.
fn uniform(bound: u32) -> Result<u32> {
    let zone = u32::MAX - u32::MAX % bound;

    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes)
            .map_err(|err| anyhow!("the system random source failed: {err}"))?;

        let value = u32::from_le_bytes(bytes);

        if value < zone {
            return Ok(value % bound);
        }
    }
}
