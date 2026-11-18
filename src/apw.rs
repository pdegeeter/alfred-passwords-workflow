//! Client for the `apw` CLI, the bridge to Apple Passwords.
//!
//! macOS only lets a browser launch the Passwords helper, so `apw` runs a
//! headless Chromium with Apple's extension behind a daemon. This module calls
//! its documented CLI (`apw pw …`, `apw auth …`) and decodes the JSON it prints:
//! results on stdout, and `{"error", "status"}` on stderr with the status as
//! exit code when it fails.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::runner::{CommandRunner, NotFound, TimedOut};

/// One account stored in Apple Passwords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub username: String,
    pub domain: String,
    pub title: Option<String>,
}

/// Why a call to `apw` failed, in terms the user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApwError {
    /// The `apw` binary is missing.
    NotInstalled,
    /// The daemon (and its headless browser) is not running.
    NotRunning,
    /// The daemon runs but was never paired with a PIN since it started.
    NotPaired,
    /// `apw` did not answer in time.
    TimedOut,
    /// Any other failure, with the message `apw` gave.
    Failed(String),
}

impl fmt::Display for ApwError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApwError::NotInstalled => write!(f, "apw is not installed"),
            ApwError::NotRunning => write!(f, "the apw daemon is not running"),
            ApwError::NotPaired => write!(f, "apw is not paired with Apple Passwords"),
            ApwError::TimedOut => write!(f, "apw did not answer in time"),
            ApwError::Failed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ApwError {}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    username: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
struct Failure {
    #[serde(default)]
    error: String,
    #[serde(default)]
    status: i64,
}

/// Calls `apw` through a [`CommandRunner`].
pub struct Apw<'a> {
    runner: &'a dyn CommandRunner,
    bin: String,
}

impl<'a> Apw<'a> {
    /// How long one `apw` call may take. Fetching a password can wait on a
    /// Touch ID prompt, so this is generous.
    pub const TIMEOUT: Duration = Duration::from_secs(30);

    /// Where Homebrew installs `apw`, in order of preference. Alfred does not
    /// give workflows the login shell's `PATH`, so a bare `apw` is the last
    /// resort.
    const CANDIDATES: [&'static str; 2] = ["/opt/homebrew/bin/apw", "/usr/local/bin/apw"];

    /// `apw` status for "no results".
    const STATUS_NO_RESULTS: i64 = 3;

    /// `apw` status for a daemon that is unreachable or not paired.
    const STATUS_INVALID_SESSION: i64 = 9;

    /// Error text the daemon's bridge returns when it runs but is not paired.
    const UNPAIRED: &'static str = "unpaired";

    pub fn new(runner: &'a dyn CommandRunner, bin: impl Into<String>) -> Self {
        Self {
            runner,
            bin: bin.into(),
        }
    }

    /// Pick the `apw` binary: `configured` (the workflow's `APW_BIN` setting)
    /// when set, else the first Homebrew location that `exists`, else `apw`.
    pub fn resolve_bin(configured: Option<&str>, exists: impl Fn(&Path) -> bool) -> String {
        if let Some(configured) = configured.map(str::trim).filter(|path| !path.is_empty()) {
            return configured.to_owned();
        }

        Apw::CANDIDATES
            .iter()
            .find(|candidate| exists(Path::new(candidate)))
            .map(|candidate| (*candidate).to_owned())
            .unwrap_or_else(|| "apw".to_owned())
    }

    /// The accounts saved for `domain`.
    pub fn list_accounts(&self, domain: &str) -> Result<Vec<Account>, ApwError> {
        let entries = match self.call(&["pw", "list", domain], None)? {
            Some(stdout) => Apw::parse_entries(&stdout)?,
            None => Vec::new(),
        };

        Ok(entries
            .into_iter()
            .map(|entry| Account {
                username: entry.username,
                domain: entry.domain.unwrap_or_else(|| domain.to_owned()),
                title: entry.title.filter(|title| !title.is_empty()),
            })
            .collect())
    }

    /// The password of `username` on `domain`.
    pub fn password(&self, domain: &str, username: &str) -> Result<String, ApwError> {
        let missing = || ApwError::Failed(format!("No password found for {username} on {domain}"));

        let Some(stdout) = self.call(&["pw", "get", domain, username], None)? else {
            return Err(missing());
        };
        let entries = Apw::parse_entries(&stdout)?;

        let exact = entries
            .iter()
            .find(|entry| entry.username == username && entry.password.is_some());

        exact
            .or_else(|| entries.iter().find(|entry| entry.password.is_some()))
            .and_then(|entry| entry.password.clone())
            .ok_or_else(missing)
    }

    /// Create the account, or replace its password when it already exists.
    /// The password goes through stdin so it never shows in the process list.
    pub fn save(&self, domain: &str, username: &str, password: &str) -> Result<(), ApwError> {
        self.call(&["pw", "save", "--stdin", domain, username], Some(password))?
            .map(drop)
            .ok_or_else(|| ApwError::Failed(format!("apw could not save {username}")))
    }

    /// Ask the daemon for a pairing challenge: macOS shows a PIN.
    pub fn request_pin(&self) -> Result<(), ApwError> {
        self.call(&["auth", "request"], None)?
            .map(drop)
            .ok_or_else(|| ApwError::Failed("apw refused the pairing request".to_owned()))
    }

    /// Answer the pairing challenge with the PIN macOS showed.
    pub fn submit_pin(&self, pin: &str) -> Result<(), ApwError> {
        self.call(&["auth", "response", "--pin", pin], None)?
            .map(drop)
            .ok_or_else(|| ApwError::Failed("apw rejected the PIN".to_owned()))
    }

    /// Run `apw` and return its stdout, mapping its failures to [`ApwError`].
    /// `None` is `apw`'s "no results" status, which some callers treat as an
    /// empty answer.
    fn call(&self, args: &[&str], stdin: Option<&str>) -> Result<Option<String>, ApwError> {
        let output = self
            .runner
            .run(&self.bin, args, stdin, Apw::TIMEOUT)
            .map_err(|err| {
                if err.downcast_ref::<NotFound>().is_some() {
                    ApwError::NotInstalled
                } else if err.downcast_ref::<TimedOut>().is_some() {
                    ApwError::TimedOut
                } else {
                    ApwError::Failed(format!("{err:#}"))
                }
            })?;

        if output.success {
            return Ok(Some(output.stdout));
        }

        let Ok(failure) = serde_json::from_str::<Failure>(output.stderr.trim()) else {
            let message = output.stderr.trim();

            return Err(ApwError::Failed(if message.is_empty() {
                "apw failed without an error message".to_owned()
            } else {
                message.to_owned()
            }));
        };

        match failure.status {
            Apw::STATUS_NO_RESULTS => Ok(None),
            Apw::STATUS_INVALID_SESSION if failure.error.trim() == Apw::UNPAIRED => {
                Err(ApwError::NotPaired)
            }
            Apw::STATUS_INVALID_SESSION => Err(ApwError::NotRunning),
            _ => Err(ApwError::Failed(failure.error)),
        }
    }

    fn parse_entries(stdout: &str) -> Result<Vec<Entry>, ApwError> {
        serde_json::from_str::<Response>(stdout.trim())
            .map(|response| response.results)
            .map_err(|err| ApwError::Failed(format!("unexpected apw output: {err}")))
    }
}
