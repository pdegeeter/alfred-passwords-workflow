//! Strong password generation, in two styles:
//!
//! - **Apple**: the format Apple Passwords suggests, three groups of six
//!   characters joined by dashes (`bevfzw-yuzdmg-edx4Fe`), lowercase letters
//!   with exactly one uppercase letter and one digit, no ambiguous characters
//!   (about 88 bits of entropy).
//! - **Custom**: a length from 4 to 40 and a choice of character sets: letters
//!   (`Aa`), digits (`345`), symbols (`@&$!#?`), and whether look-alike
//!   characters (`1lI|O0oZ2`) are allowed. Every enabled set appears at least
//!   once.
//!
//! The default style comes from the workflow configuration
//! ([`Style::from_config`]); a query can override it for one generation, and
//! the choice travels to the action as a compact spec ([`Style::spec`]).

use anyhow::{anyhow, bail, Context, Result};

/// The rules of a custom password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    pub length: usize,
    pub letters: bool,
    pub digits: bool,
    pub symbols: bool,
    /// Allow look-alike characters such as `1`, `l`, `|`, `O`, `0`.
    pub similar: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            length: Style::DEFAULT_LENGTH,
            letters: true,
            digits: true,
            symbols: true,
            similar: false,
        }
    }
}

/// How to generate a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Apple,
    Custom(Rules),
}

/// The workflow configuration: the default style, and the custom rules that
/// query options start from even when the default style is Apple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub apple: bool,
    pub rules: Rules,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            apple: true,
            rules: Rules::default(),
        }
    }
}

impl Config {
    /// Configuration variables (Alfred's "Configure Workflow" sheet).
    pub const STYLE_VAR: &'static str = "PWG_STYLE";
    pub const LENGTH_VAR: &'static str = "PWG_LENGTH";
    pub const LETTERS_VAR: &'static str = "PWG_LETTERS";
    pub const DIGITS_VAR: &'static str = "PWG_DIGITS";
    pub const SYMBOLS_VAR: &'static str = "PWG_SYMBOLS";
    pub const SIMILAR_VAR: &'static str = "PWG_SIMILAR";

    /// Read the configuration with `get`. Missing or invalid values fall back
    /// to the defaults: Apple style, 20 characters, letters, digits and
    /// symbols, no look-alikes.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Config {
        let defaults = Rules::default();
        let flag = |name: &str, default: bool| {
            get(name)
                .and_then(|value| Config::parse_flag(&value))
                .unwrap_or(default)
        };
        let length = get(Config::LENGTH_VAR)
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|length| Style::LENGTHS.contains(length))
            .unwrap_or(defaults.length);

        Config {
            apple: get(Config::STYLE_VAR)
                .map(|value| !value.trim().eq_ignore_ascii_case("custom"))
                .unwrap_or(true),
            rules: Rules {
                length,
                letters: flag(Config::LETTERS_VAR, defaults.letters),
                digits: flag(Config::DIGITS_VAR, defaults.digits),
                symbols: flag(Config::SYMBOLS_VAR, defaults.symbols),
                similar: flag(Config::SIMILAR_VAR, defaults.similar),
            },
        }
    }

    /// The default style this configuration selects.
    pub fn style(&self) -> Style {
        if self.apple {
            Style::Apple
        } else {
            Style::Custom(self.rules)
        }
    }

    /// Alfred checkboxes export `1` / `0`; accept the usual spellings.
    fn parse_flag(value: &str) -> Option<bool> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }
}

impl Style {
    /// Allowed lengths of a custom password.
    pub const LENGTHS: std::ops::RangeInclusive<usize> = 4..=40;

    pub const DEFAULT_LENGTH: usize = 20;

    /// Letters of an Apple password, without `i`, `l` and `o`.
    const APPLE_LETTERS: &'static [u8] = b"abcdefghjkmnpqrstuvwxyz";
    /// Digits of an Apple password, without `0` and `1`.
    const APPLE_DIGITS: &'static [u8] = b"23456789";
    const APPLE_GROUPS: usize = 3;
    const APPLE_GROUP_LEN: usize = 6;

    const LETTERS: &'static str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const DIGITS: &'static str = "0123456789";
    const SYMBOLS: &'static str = "@&$!#?";
    /// Characters easily mistaken for one another.
    const SIMILAR: &'static str = "1lI|O0oZ2";

    /// A compact, lossless encoding of the style, carried by Alfred items to
    /// the action: `apple`, or `custom:<length>:<flags>` where the flags are
    /// `l` (letters), `d` (digits), `s` (symbols), `a` (look-alikes).
    pub fn spec(&self) -> String {
        match self {
            Style::Apple => "apple".to_owned(),
            Style::Custom(rules) => {
                let flags: String = [
                    (rules.letters, 'l'),
                    (rules.digits, 'd'),
                    (rules.symbols, 's'),
                    (rules.similar, 'a'),
                ]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, flag)| *flag)
                .collect();

                format!("custom:{}:{flags}", rules.length)
            }
        }
    }

    /// Decode a [`Style::spec`].
    pub fn from_spec(spec: &str) -> Result<Style> {
        if spec == "apple" {
            return Ok(Style::Apple);
        }

        let mut parts = spec.splitn(3, ':');
        let (Some("custom"), Some(length), Some(flags)) =
            (parts.next(), parts.next(), parts.next())
        else {
            bail!("invalid password style `{spec}`");
        };

        let length: usize = length
            .parse()
            .ok()
            .filter(|length| Style::LENGTHS.contains(length))
            .with_context(|| format!("invalid password length in `{spec}`"))?;

        if let Some(unknown) = flags.chars().find(|flag| !"ldsa".contains(*flag)) {
            bail!("invalid password option `{unknown}` in `{spec}`");
        }

        Ok(Style::Custom(Rules {
            length,
            letters: flags.contains('l'),
            digits: flags.contains('d'),
            symbols: flags.contains('s'),
            similar: flags.contains('a'),
        }))
    }

    /// A one-line description for Alfred subtitles and the history.
    pub fn summary(&self) -> String {
        match self {
            Style::Apple => "Apple format".to_owned(),
            Style::Custom(rules) => {
                let sets: Vec<&str> = [
                    (rules.letters, "Aa"),
                    (rules.digits, "345"),
                    (rules.symbols, "@&$"),
                ]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, label)| *label)
                .collect();
                let similar = if rules.similar {
                    "look-alikes allowed"
                } else {
                    "no look-alikes"
                };

                format!(
                    "{} characters · {} · {similar}",
                    rules.length,
                    sets.join(" ")
                )
            }
        }
    }

    /// Why this style cannot generate a password, if it cannot.
    pub fn problem(&self) -> Option<String> {
        match self {
            Style::Apple => None,
            Style::Custom(rules) if !Style::LENGTHS.contains(&rules.length) => Some(format!(
                "The length must be between {} and {}",
                Style::LENGTHS.start(),
                Style::LENGTHS.end()
            )),
            Style::Custom(rules) if !(rules.letters || rules.digits || rules.symbols) => {
                Some("Enable letters, digits or symbols".to_owned())
            }
            Style::Custom(_) => None,
        }
    }

    /// The character sets a custom password draws from, look-alikes removed
    /// unless allowed. Empty sets are left out.
    fn character_sets(rules: &Rules) -> Vec<Vec<char>> {
        [
            (rules.letters, Style::LETTERS),
            (rules.digits, Style::DIGITS),
            (rules.symbols, Style::SYMBOLS),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, set)| {
            set.chars()
                .filter(|c| rules.similar || !Style::SIMILAR.contains(*c))
                .collect::<Vec<char>>()
        })
        .filter(|set| !set.is_empty())
        .collect()
    }
}

/// Generate a password of `style` from the OS secure random source.
pub fn generate(style: &Style) -> Result<String> {
    let mut failure = None;
    let password = generate_with(style, &mut |bound| match uniform(bound) {
        Ok(value) => value,
        Err(err) => {
            failure.get_or_insert(err);

            0
        }
    });

    match failure {
        Some(err) => Err(err),
        None => password,
    }
}

/// Generate a password of `style` from `random`, which must return a
/// uniformly distributed value in `0..bound`.
pub fn generate_with(style: &Style, random: &mut dyn FnMut(u32) -> u32) -> Result<String> {
    if let Some(problem) = style.problem() {
        bail!("{problem}");
    }

    match style {
        Style::Apple => Ok(apple(random)),
        Style::Custom(rules) => Ok(custom(rules, random)),
    }
}

fn apple(random: &mut dyn FnMut(u32) -> u32) -> String {
    let len = Style::APPLE_GROUPS * Style::APPLE_GROUP_LEN;
    let mut chars: Vec<u8> = (0..len)
        .map(|_| Style::APPLE_LETTERS[pick(random, Style::APPLE_LETTERS.len())])
        .collect();

    let digit_at = pick(random, len);
    let upper_at = (digit_at + 1 + pick(random, len - 1)) % len;

    chars[digit_at] = Style::APPLE_DIGITS[pick(random, Style::APPLE_DIGITS.len())];
    chars[upper_at] = chars[upper_at].to_ascii_uppercase();

    chars
        .chunks(Style::APPLE_GROUP_LEN)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// One character from each enabled set first, so every set is represented,
/// then characters from all of them; the result is shuffled so the guaranteed
/// ones are not always in front.
fn custom(rules: &Rules, random: &mut dyn FnMut(u32) -> u32) -> String {
    let sets = Style::character_sets(rules);
    let all: Vec<char> = sets.concat();

    let mut chars: Vec<char> = sets
        .iter()
        .take(rules.length)
        .map(|set| set[pick(random, set.len())])
        .collect();

    while chars.len() < rules.length {
        chars.push(all[pick(random, all.len())]);
    }

    for i in (1..chars.len()).rev() {
        chars.swap(i, pick(random, i + 1));
    }

    chars.into_iter().collect()
}

fn pick(random: &mut dyn FnMut(u32) -> u32, bound: usize) -> usize {
    random(bound as u32) as usize
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
