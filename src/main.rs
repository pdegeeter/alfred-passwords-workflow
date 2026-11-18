//! `alfred-passwords` — Alfred workflow entry point.
//!
//! Three subcommands, dispatched by Alfred:
//!   * `search [query]`   — script filter (`pw`): accounts for a domain.
//!   * `generate [query]` — script filter (`pwgen`): generate, optionally save.
//!   * `run`              — action: perform the picked item's action, read from
//!     the Alfred variables in the environment, and print a notification.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use powerpack::Item;

use alfred_passwords::actions::{self, Action};
use alfred_passwords::apw::Apw;
use alfred_passwords::clipboard::SystemClipboard;
use alfred_passwords::query::{self, GenerateQuery, SearchQuery};
use alfred_passwords::runner::SystemRunner;
use alfred_passwords::{alfred, generator};

/// Workflow setting that overrides where `apw` is looked up.
const APW_BIN_VAR: &str = "APW_BIN";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let query = args.next().unwrap_or_default();

    let runner = SystemRunner;
    let bin = Apw::resolve_bin(env::var(APW_BIN_VAR).ok().as_deref(), Path::exists);
    let apw = Apw::new(&runner, bin);

    match command.as_str() {
        "search" => output(search(&apw, &query)),
        "generate" => output(generate(&apw, &query)),
        "run" => run(&apw),
        other => {
            eprintln!("unknown command: `{other}` (expected `search`, `generate` or `run`)");

            ExitCode::FAILURE
        }
    }
}

fn search(apw: &Apw, query: &str) -> Vec<Item> {
    match query::parse_search(query) {
        SearchQuery::Empty => alfred::empty_search(),
        SearchQuery::Incomplete(input) => alfred::incomplete_search(&input),
        SearchQuery::PinRequest => alfred::pin_request(),
        SearchQuery::PinSubmit(pin) => alfred::pin_submit(&pin),
        SearchQuery::Domain { domain, filter } => match apw.list_accounts(&domain) {
            Ok(accounts) => alfred::accounts(&domain, &filter, accounts),
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
/// included, so the user always sees what happened.
fn run(apw: &Apw) -> ExitCode {
    let result = Action::from_variables(|name| env::var(name).ok())
        .and_then(|action| actions::run(&action, apw, &SystemClipboard, &generator::generate));

    match result {
        Ok(message) => {
            println!("{message}");

            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("{err:#}");

            ExitCode::FAILURE
        }
    }
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
