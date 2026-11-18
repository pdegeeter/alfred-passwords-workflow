//! Parsing of what the user types after the Alfred keywords.

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

/// What the `pwgen` keyword's query asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateQuery {
    /// Generate and copy only.
    CopyOnly,
    /// A domain without a username yet: copy only, and hint at saving.
    NeedsUsername { domain: String },
    /// Generate, save for `username` on `domain`, and copy.
    Save { domain: String, username: String },
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

pub fn parse_generate(query: &str) -> GenerateQuery {
    let mut words = query.split_whitespace();
    let Some(domain) = words.next().and_then(normalize_domain) else {
        return GenerateQuery::CopyOnly;
    };

    match words.next() {
        Some(username) => GenerateQuery::Save {
            domain,
            username: username.to_owned(),
        },
        None => GenerateQuery::NeedsUsername { domain },
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
