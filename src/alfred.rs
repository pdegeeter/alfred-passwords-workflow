//! Translation of queries, accounts and errors into Alfred script filter items.
//!
//! No item sets a `uid`: Alfred would re-sort them by its own usage knowledge,
//! and the order here is deliberate (accounts as `apw` returns them, the main
//! action first).

use powerpack::{Item, Key, Modifier};

use crate::actions::Action;
use crate::apw::{Account, ApwError};
use crate::generator::Style;
use crate::history::{Entry, History};

/// Items shown before any query: the most used domains, then how to search.
pub fn empty_search(suggestions: Vec<String>) -> Vec<Item> {
    let mut items = domain_suggestions(suggestions);

    items.push(
        Item::new("Search Apple Passwords")
            .subtitle("Type a domain, e.g. github.com · `pw pin` pairs apw")
            .valid(false),
    );

    items
}

/// Items shown while the query is not a domain yet: the known domains that
/// match, or a hint when none does.
pub fn incomplete_search(input: &str, suggestions: Vec<String>) -> Vec<Item> {
    if !suggestions.is_empty() {
        return domain_suggestions(suggestions);
    }

    vec![Item::new(format!("Keep typing a domain: {input}…"))
        .subtitle("Apple Passwords is searched by domain, e.g. github.com")
        .valid(false)]
}

/// One item per known domain. ↵ or ⇥ completes the query to the domain, which
/// then lists its accounts.
pub fn domain_suggestions(domains: Vec<String>) -> Vec<Item> {
    domains
        .into_iter()
        .map(|domain| {
            Item::new(domain.clone())
                .subtitle("↵ or ⇥ to list its accounts")
                .autocomplete(format!("{domain} "))
                .valid(false)
        })
        .collect()
}

/// Items for the accounts saved on `domain` whose username or title contains
/// every word of `filter` (case-insensitive).
pub fn accounts(domain: &str, filter: &str, accounts: Vec<Account>) -> Vec<Item> {
    let terms: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();

    let items: Vec<Item> = accounts
        .into_iter()
        .filter(|account| {
            let haystack = format!(
                "{} {}",
                account.username,
                account.title.as_deref().unwrap_or_default()
            )
            .to_lowercase();

            terms.iter().all(|term| haystack.contains(term))
        })
        .map(account_item)
        .collect();

    if !items.is_empty() {
        return items;
    }

    vec![Item::new(format!("No account found for {domain}"))
        .subtitle(format!(
            "Create one with `pwg {domain} <username>`, or check the domain"
        ))
        .valid(false)]
}

/// Items for the pairing flow.
pub fn pin_request() -> Vec<Item> {
    vec![action_item(
        Item::new("Pair apw with Apple Passwords")
            .subtitle("↵ shows a PIN, then type `pw pin <code>`")
            .arg("pin")
            .autocomplete("pin "),
        &Action::RequestPin,
    )]
}

pub fn pin_submit(pin: &str) -> Vec<Item> {
    vec![action_item(
        Item::new(format!("Submit PIN {pin}"))
            .subtitle("Finish pairing apw with Apple Passwords")
            .arg(pin),
        &Action::SubmitPin(pin.to_owned()),
    )]
}

/// Item explaining an `apw` failure and, when possible, how to fix it.
pub fn error(err: &ApwError) -> Item {
    match err {
        ApwError::NotInstalled => Item::new("apw is not installed")
            .subtitle("Install it with `brew install bendews/homebrew-tap/apw`")
            .valid(false),
        ApwError::NotRunning => Item::new("The apw daemon is not running")
            .subtitle("Start it with `brew services start apw`, then type `pw pin`")
            .valid(false),
        ApwError::NotPaired => action_item(
            Item::new("apw is not paired with Apple Passwords")
                .subtitle("↵ shows a PIN, then type `pw pin <code>`")
                .arg("pin"),
            &Action::RequestPin,
        ),
        ApwError::TimedOut | ApwError::Failed(_) => Item::new("apw error")
            .subtitle(err.to_string())
            .valid(false),
    }
}

/// Item that generates a password of `style` and copies it. `save_hint`
/// names the domain the user could save it for once a username is typed.
pub fn generate(style: &Style, save_hint: Option<&str>) -> Item {
    if let Some(problem) = style.problem() {
        return Item::new("Cannot generate this password")
            .subtitle(problem)
            .valid(false);
    }

    let hint = match save_hint {
        Some(domain) => format!("add a username to save it for {domain}"),
        None => "copied, kept out of clipboard history".to_owned(),
    };

    action_item(
        Item::new("Generate a strong password")
            .subtitle(format!("{} · {hint}", style.summary()))
            .arg("generate"),
        &Action::Generate { style: *style },
    )
}

/// Items to generate a password of `style` and save it for `username` on
/// `domain`.
///
/// `exists` says whether that account is already saved: replacing its
/// password then needs ⌥↵, so a plain ↵ never overwrites one by accident.
pub fn generate_and_save(
    domain: &str,
    username: &str,
    style: &Style,
    exists: Result<bool, ApwError>,
) -> Vec<Item> {
    if style.problem().is_some() {
        return vec![generate(style, None)];
    }

    let action = Action::GenerateAndSave {
        domain: domain.to_owned(),
        username: username.to_owned(),
        style: *style,
    };

    let save = match exists {
        Ok(false) => action_item(
            Item::new(format!("Generate and save for {username}"))
                .subtitle(format!(
                    "New account on {domain} · {} · the password is copied",
                    style.summary()
                ))
                .arg(username),
            &action,
        ),
        Ok(true) => Item::new(format!("Replace the password of {username}"))
            .subtitle(format!(
                "{username} already exists on {domain} · hold ⌥ and press ↵ to replace it"
            ))
            .arg(username)
            .valid(false)
            .modifier(
                Modifier::new(Key::Option)
                    .subtitle(format!(
                        "Replace the password of {username} on {domain} and copy it"
                    ))
                    .arg(username)
                    .valid(true)
                    .variables(action.variables()),
            ),
        Err(err) => error(&err),
    };

    vec![save, generate(style, None)]
}

/// Items for options typed in `pwg` that are invalid.
pub fn option_problems(problems: &[String]) -> Vec<Item> {
    problems
        .iter()
        .map(|problem| {
            Item::new(problem.clone())
                .subtitle("Options: a length (4–40), apple, ±l letters, ±d digits, ±s symbols, ±a look-alikes")
                .valid(false)
        })
        .collect()
}

/// One item per generated password, newest first: a masked preview, never
/// the password itself. ↵ copies it again.
pub fn history(entries: &[Entry], now: u64) -> Vec<Item> {
    entries
        .iter()
        .map(|entry| {
            let mut subtitle = format!(
                "Generated {} · {}",
                History::age(entry.created_at, now),
                entry.style
            );

            if let Some(saved_for) = &entry.saved_for {
                subtitle.push_str(&format!(" · saved for {saved_for}"));
            }

            action_item(
                Item::new(History::preview(&entry.password))
                    .subtitle(format!("{subtitle} · ↵ copy again"))
                    .arg(entry.id.clone()),
                &Action::CopyGenerated {
                    id: entry.id.clone(),
                },
            )
        })
        .collect()
}

/// Item shown when the history cannot be read (Keychain access denied, …).
pub fn history_error(err: &anyhow::Error) -> Item {
    Item::new("Password history unavailable")
        .subtitle(format!("{err:#}"))
        .valid(false)
}

fn account_item(account: Account) -> Item {
    let password = Action::CopyPassword {
        domain: account.domain.clone(),
        username: account.username.clone(),
    };
    let username = Action::CopyUsername {
        domain: account.domain.clone(),
        username: account.username.clone(),
    };
    let otp = Action::CopyOtp {
        domain: account.domain.clone(),
        username: account.username.clone(),
    };
    let context = match &account.title {
        Some(title) => format!("{} · {title}", account.domain),
        None => account.domain.clone(),
    };

    action_item(
        Item::new(account.username.clone())
            .subtitle(format!(
                "{context} · ↵ password · ⌘↵ username · ⌥↵ one-time code"
            ))
            .arg(account.username.clone())
            .copy_text(account.username.clone())
            .modifier(
                Modifier::new(Key::Command)
                    .subtitle(format!("Copy the username {}", account.username))
                    .arg(account.username.clone())
                    .variables(username.variables()),
            )
            .modifier(
                Modifier::new(Key::Option)
                    .subtitle(format!("Copy the one-time code of {}", account.username))
                    .arg(account.username.clone())
                    .variables(otp.variables()),
            ),
        &password,
    )
}

fn action_item(item: Item, action: &Action) -> Item {
    item.variables(action.variables())
}
