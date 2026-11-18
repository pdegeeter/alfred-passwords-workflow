//! Parsing of what the user types after the Alfred keywords.

use crate::generator::{Config, Rules, Style};

/// What the `pw` keyword's query asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchQuery {
    /// Nothing typed yet.
    Empty,
    /// Something that is not a domain yet (`githu`, `github.c`).
    Incomplete(String),
    /// A domain, plus an optional filter on the usernames.
    Domain { domain: String, filter: String },
    /// `pin`: ask the daemon for a pairing PIN.
    PinRequest,
    /// `pin <6 digits>`: answer the pairing challenge.
    PinSubmit(String),
}

/// What the `pwg` keyword's query asks for: options for this generation,
/// and what to do with the password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateQuery {
    pub options: Options,
    pub target: Target,
    /// Options that were typed but are invalid, as messages for the user.
    pub problems: Vec<String>,
}

/// What to do with a generated password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Generate and copy only.
    CopyOnly,
    /// A domain without a username yet: copy only, and hint at saving.
    NeedsUsername { domain: String },
    /// Generate, save for `username` on `domain`, and copy.
    Save { domain: String, username: String },
}

/// Options typed in a `pwg` query, overriding the configured style for one
/// generation: a number sets the length, `apple` picks the Apple format, and
/// `+l`/`-l`, `+d`/`-d`, `+s`/`-s`, `+a`/`-a` turn letters, digits, symbols
/// and look-alike characters on or off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub apple: bool,
    pub length: Option<usize>,
    pub letters: Option<bool>,
    pub digits: Option<bool>,
    pub symbols: Option<bool>,
    pub similar: Option<bool>,
}

impl Options {
    /// The style for this generation. A length or a character-set option
    /// means a custom password built on the configured rules, even when the
    /// configured style is Apple; `apple` alone picks the Apple format.
    pub fn apply(&self, config: &Config) -> Style {
        let custom = self.length.is_some()
            || self.letters.is_some()
            || self.digits.is_some()
            || self.symbols.is_some()
            || self.similar.is_some();

        if !custom {
            return if self.apple {
                Style::Apple
            } else {
                config.style()
            };
        }

        let base = config.rules;

        Style::Custom(Rules {
            length: self.length.unwrap_or(base.length),
            letters: self.letters.unwrap_or(base.letters),
            digits: self.digits.unwrap_or(base.digits),
            symbols: self.symbols.unwrap_or(base.symbols),
            similar: self.similar.unwrap_or(base.similar),
        })
    }

    /// Apply `word` if it is an option. Returns `None` when it is not one,
    /// `Some(Err)` when it looks like one but is invalid.
    fn parse(&mut self, word: &str) -> Option<Result<(), String>> {
        if word.eq_ignore_ascii_case("apple") {
            self.apple = true;

            return Some(Ok(()));
        }

        if word.chars().all(|c| c.is_ascii_digit()) {
            return Some(match word.parse::<usize>() {
                Ok(length) if Style::LENGTHS.contains(&length) => {
                    self.length = Some(length);

                    Ok(())
                }
                _ => Err(format!(
                    "Length {word} is out of range: {}–{}",
                    Style::LENGTHS.start(),
                    Style::LENGTHS.end()
                )),
            });
        }

        let mut chars = word.chars();
        let on = match chars.next() {
            Some('+') => true,
            Some('-') => false,
            _ => return None,
        };
        let flag = chars.next();

        if chars.next().is_some() {
            return None;
        }

        let slot = match flag {
            Some('l') => &mut self.letters,
            Some('d') => &mut self.digits,
            Some('s') => &mut self.symbols,
            Some('a') => &mut self.similar,
            _ => return Some(Err(format!("Unknown option {word}: use ±l, ±d, ±s or ±a"))),
        };
        *slot = Some(on);

        Some(Ok(()))
    }
}

/// The word that switches the `pw` keyword to pairing.
const PIN_KEYWORD: &str = "pin";

/// Number of digits in the PIN macOS shows when pairing.
const PIN_LEN: usize = 6;

pub fn parse_search(query: &str) -> SearchQuery {
    let mut words = query.split_whitespace();
    let Some(first) = words.next() else {
        return SearchQuery::Empty;
    };

    if first.eq_ignore_ascii_case(PIN_KEYWORD) {
        return match words.next() {
            Some(pin) if is_pin(pin) => SearchQuery::PinSubmit(pin.to_owned()),
            _ => SearchQuery::PinRequest,
        };
    }

    match normalize_domain(first) {
        Some(domain) => SearchQuery::Domain {
            domain,
            filter: words.collect::<Vec<_>>().join(" "),
        },
        None => SearchQuery::Incomplete(first.to_owned()),
    }
}

/// Parse a `pwg` query: options anywhere, then an optional domain and the
/// username to save the password for (`pwg 24 -s github.com me`).
pub fn parse_generate(query: &str) -> GenerateQuery {
    let mut options = Options::default();
    let mut problems = Vec::new();
    let mut domain = None;
    let mut username = None;

    for word in query.split_whitespace() {
        match options.parse(word) {
            Some(Ok(())) => continue,
            Some(Err(problem)) => {
                problems.push(problem);

                continue;
            }
            None => {}
        }

        if domain.is_none() {
            domain = normalize_domain(word);
        } else if username.is_none() {
            username = Some(word.to_owned());
        }
    }

    let target = match (domain, username) {
        (Some(domain), Some(username)) => Target::Save { domain, username },
        (Some(domain), None) => Target::NeedsUsername { domain },
        (None, _) => Target::CopyOnly,
    };

    GenerateQuery {
        options,
        target,
        problems,
    }
}

/// Turn what the user typed (`GitHub.com`, `https://github.com/login`) into a
/// bare lowercase host, or `None` while it does not look like a domain yet: it
/// needs a dot and a top-level domain of at least two letters, which also keeps
/// Alfred from calling `apw` on every keystroke of `githu…`.
pub fn normalize_domain(input: &str) -> Option<String> {
    let lower = input.trim().to_ascii_lowercase();
    let without_scheme = lower
        .split_once("://")
        .map_or(lower.as_str(), |(_, rest)| rest);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = host.split(':').next().unwrap_or_default();

    let valid_chars = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    let (name, tld) = host.rsplit_once('.')?;

    let looks_like_domain = valid_chars
        && !name.is_empty()
        && !name.starts_with('.')
        && tld.len() >= 2
        && tld.chars().all(|c| c.is_ascii_alphabetic());

    looks_like_domain.then(|| host.to_owned())
}

fn is_pin(input: &str) -> bool {
    input.len() == PIN_LEN && input.chars().all(|c| c.is_ascii_digit())
}
