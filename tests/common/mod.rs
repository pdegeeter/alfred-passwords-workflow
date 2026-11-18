//! Shared test helpers: deterministic, offline doubles for the impure
//! boundaries: [`CommandRunner`], [`Clipboard`], [`KeyStore`] (the Keychain)
//! and [`HistoryVault`].
//!
//! Included by multiple test crates; each uses a different subset, so unused
//! helpers are expected per-crate.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use alfred_passwords::clipboard::Clipboard;
use alfred_passwords::history::History;
use alfred_passwords::runner::{CommandOutput, CommandRunner, NotFound, TimedOut};
use alfred_passwords::vault::{HistoryVault, KeyStore, Unreadable};
use anyhow::{anyhow, bail, Result};

/// A [`KeyStore`] returning a fixed key, or failing like a denied Keychain
/// prompt; counts how often the key was asked for.
pub struct FakeKeyStore {
    key: Option<[u8; 32]>,
    pub reads: Cell<usize>,
}

impl FakeKeyStore {
    pub fn new(byte: u8) -> Self {
        Self {
            key: Some([byte; 32]),
            reads: Cell::new(0),
        }
    }

    pub fn denied() -> Self {
        Self {
            key: None,
            reads: Cell::new(0),
        }
    }
}

impl KeyStore for FakeKeyStore {
    fn key(&self) -> Result<[u8; 32]> {
        self.reads.set(self.reads.get() + 1);

        self.key
            .ok_or_else(|| anyhow!("the user denied Keychain access"))
    }
}

/// How [`FakeVault::load`] behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    Ok,
    Unreadable,
    Fails,
}

/// An in-memory [`HistoryVault`].
pub struct FakeVault {
    pub history: RefCell<History>,
    pub load_mode: LoadMode,
    pub save_fails: bool,
    pub saves: Cell<usize>,
}

impl FakeVault {
    pub fn new(history: History) -> Self {
        Self {
            history: RefCell::new(history),
            load_mode: LoadMode::Ok,
            save_fails: false,
            saves: Cell::new(0),
        }
    }
}

impl HistoryVault for FakeVault {
    fn load(&self) -> Result<History> {
        match self.load_mode {
            LoadMode::Ok => Ok(self.history.borrow().clone()),
            LoadMode::Unreadable => Err(anyhow!(Unreadable)),
            LoadMode::Fails => bail!("the user denied Keychain access"),
        }
    }

    fn save(&self, history: &History) -> Result<()> {
        if self.save_fails {
            bail!("disk full");
        }

        self.saves.set(self.saves.get() + 1);
        *self.history.borrow_mut() = history.clone();

        Ok(())
    }
}

/// A single recorded invocation of [`FakeRunner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
}

#[derive(Debug, Clone)]
enum Response {
    Output(CommandOutput),
    NotFound,
    TimedOut,
}

/// A [`CommandRunner`] that answers with canned output keyed by the
/// space-joined arguments (`"pw list github.com"`) and records every call.
pub struct FakeRunner {
    responses: HashMap<String, Response>,
    pub calls: RefCell<Vec<Call>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            calls: RefCell::new(Vec::new()),
        }
    }

    /// Make `args` succeed and print `stdout`.
    pub fn with_success(self, args: &str, stdout: &str) -> Self {
        self.with(
            args,
            Response::Output(CommandOutput {
                success: true,
                stdout: stdout.to_owned(),
                stderr: String::new(),
            }),
        )
    }

    /// Make `args` fail and print `stderr`.
    pub fn with_failure(self, args: &str, stderr: &str) -> Self {
        self.with(
            args,
            Response::Output(CommandOutput {
                success: false,
                stdout: String::new(),
                stderr: stderr.to_owned(),
            }),
        )
    }

    /// Make every call fail as if the program did not exist.
    pub fn with_missing_program(self, args: &str) -> Self {
        self.with(args, Response::NotFound)
    }

    /// Make `args` run past its timeout.
    pub fn with_timeout(self, args: &str) -> Self {
        self.with(args, Response::TimedOut)
    }

    fn with(mut self, args: &str, response: Response) -> Self {
        self.responses.insert(args.to_owned(), response);

        self
    }
}

impl CommandRunner for FakeRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<CommandOutput> {
        self.calls.borrow_mut().push(Call {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            stdin: stdin.map(str::to_owned),
        });

        let key = args.join(" ");

        match self.responses.get(&key) {
            Some(Response::Output(output)) => Ok(output.clone()),
            Some(Response::NotFound) => Err(anyhow!(NotFound {
                program: program.to_owned(),
            })),
            Some(Response::TimedOut) => Err(anyhow!(TimedOut {
                program: program.to_owned(),
                timeout,
            })),
            None => bail!("unexpected call: `{program} {key}`"),
        }
    }
}

/// A [`Clipboard`] that records what was copied and whether it was concealed.
pub struct FakeClipboard {
    pub copies: RefCell<Vec<(String, bool)>>,
}

impl FakeClipboard {
    pub fn new() -> Self {
        Self {
            copies: RefCell::new(Vec::new()),
        }
    }
}

impl Clipboard for FakeClipboard {
    fn copy(&self, text: &str, concealed: bool) -> Result<()> {
        self.copies.borrow_mut().push((text.to_owned(), concealed));

        Ok(())
    }
}

/// The JSON `apw` prints on stderr when its daemon is unreachable.
pub const NOT_RUNNING: &str = r#"{"error":"APW is not running or not authenticated, run `apw start` then `apw auth`","status":9,"results":[]}"#;

/// The JSON `apw` prints on stderr when its daemon runs but is not paired.
pub const UNPAIRED: &str = r#"{"error":"unpaired","status":9,"results":[]}"#;

/// The JSON `apw` prints on stderr when nothing matches.
pub const NO_RESULTS: &str = r#"{"error":"No results found","status":3,"results":[]}"#;
