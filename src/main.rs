//! `alfred-passwords` — Alfred workflow entry point.
//!
//! Three subcommands, dispatched by Alfred:
//!   * `search [query]`   — script filter (`pw`): accounts for a domain, and
//!     the known domains while the query is not one yet.
//!   * `generate [query]` — script filter (`pwgen`): generate, optionally save.
//!   * `run`              — action: perform the picked item's action, read from
//!     the Alfred variables in the environment, and print a notification.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use powerpack::Item;

use alfred_passwords::actions::{self, Action};
use alfred_passwords::apw::Apw;
use alfred_passwords::clipboard::SystemClipboard;
use alfred_passwords::domains::DomainIndex;
use alfred_passwords::query::{self, GenerateQuery, SearchQuery};
use alfred_passwords::runner::SystemRunner;
use alfred_passwords::{alfred, generator};

/// Workflow setting that overrides where `apw` is looked up.
const APW_BIN_VAR: &str = "APW_BIN";

/// Alfred's persistent data folder for this workflow, where the domain index
/// lives.
const DATA_DIR_VAR: &str = "alfred_workflow_data";

/// How many known domains are suggested at once.
const MAX_SUGGESTIONS: usize = 9;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let query = args.next().unwrap_or_default();

    let runner = SystemRunner;
    let bin = Apw::resolve_bin(env::var(APW_BIN_VAR).ok().as_deref(), Path::exists);
    let apw = Apw::new(&runner, bin);
    let index_path = DomainIndex::path(env::var_os(DATA_DIR_VAR).map(PathBuf::from).as_deref());

    match command.as_str() {
        "search" => output(search(&apw, &query, index_path.as_deref())),
        "generate" => output(generate(&apw, &query)),
        "run" => run(&apw, index_path.as_deref()),
        other => {
            eprintln!("unknown command: `{other}` (expected `search`, `generate` or `run`)");

            ExitCode::FAILURE
        }
    }
}

fn search(apw: &Apw, query: &str, index_path: Option<&Path>) -> Vec<Item> {
    let mut index = index_path.map(DomainIndex::load).unwrap_or_default();
    let now = now();

    match query::parse_search(query) {
        SearchQuery::Empty => alfred::empty_search(index.suggest("", now, MAX_SUGGESTIONS)),
        SearchQuery::Incomplete(input) => {
            alfred::incomplete_search(&input, index.suggest(&input, now, MAX_SUGGESTIONS))
        }
        SearchQuery::PinRequest => alfred::pin_request(),
        SearchQuery::PinSubmit(pin) => alfred::pin_submit(&pin),
        SearchQuery::Domain { domain, filter } => match apw.list_accounts(&domain) {
            Ok(accounts) => {
                if index.observe(&domain, !accounts.is_empty(), now) {
                    save_index(index_path, &index);
                }

                // A complete but wrong domain (`github.co`) still suggests the
                // known ones it is part of (`github.com`).
                let mut items = Vec::new();

                if accounts.is_empty() {
                    items =
                        alfred::domain_suggestions(index.suggest(&domain, now, MAX_SUGGESTIONS));
                }

                items.extend(alfred::accounts(&domain, &filter, accounts));

                items
            }
            Err(err) => vec![alfred::error(&err)],
        },
    }
}

fn generate(apw: &Apw, query: &str) -> Vec<Item> {
    match query::parse_generate(query) {
        GenerateQuery::CopyOnly => vec![alfred::generate(None)],
        GenerateQuery::NeedsUsername { domain } => vec![alfred::generate(Some(&domain))],
        GenerateQuery::Save { domain, username } => {
            let exists = apw
                .list_accounts(&domain)
                .map(|accounts| accounts.iter().any(|account| account.username == username));

            alfred::generate_and_save(&domain, &username, exists)
        }
    }
}

/// Action: the result is printed for the downstream notification, failures
/// included, so the user always sees what happened. A successful action on an
/// account counts as a use of its domain in the index.
fn run(apw: &Apw, index_path: Option<&Path>) -> ExitCode {
    let action = match Action::from_variables(|name| env::var(name).ok()) {
        Ok(action) => action,
        Err(err) => {
            println!("{err:#}");

            return ExitCode::FAILURE;
        }
    };

    match actions::run(&action, apw, &SystemClipboard, &generator::generate) {
        Ok(message) => {
            if let (Some(domain), Some(path)) = (action.domain(), index_path) {
                let mut index = DomainIndex::load(path);
                index.record_use(domain, now());
                save_index(Some(path), &index);
            }

            println!("{message}");

            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("{err:#}");

            ExitCode::FAILURE
        }
    }
}

/// Save the domain index. A failure only loses suggestions, so it goes to
/// Alfred's debugger and is never shown as an error.
fn save_index(path: Option<&Path>, index: &DomainIndex) {
    let Some(path) = path else {
        return;
    };

    if let Err(err) = index.save(path) {
        eprintln!("failed to save the domain index: {err:#}");
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn output(items: Vec<Item>) -> ExitCode {
    match powerpack::output(items) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("failed to write Alfred output: {err}");

            ExitCode::FAILURE
        }
    }
}
