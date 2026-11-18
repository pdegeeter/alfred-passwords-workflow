//! What happens when the user picks an item: the action is encoded in the
//! item's Alfred variables, which Alfred hands to `alfred-passwords run` as
//! environment variables.
//!
//! Secrets never go through Alfred: the password is fetched, generated or
//! read from the history here and written to the clipboard directly, and only
//! a notification message is printed back.

use anyhow::{bail, Context as _, Result};

use crate::apw::{Apw, ApwError};
use crate::clipboard::Clipboard;
use crate::generator::Style;
use crate::history::{Entry, History};
use crate::vault::{HistoryVault, Unreadable};

/// Variable carrying the action name.
pub const VAR_ACTION: &str = "action";
/// Variable carrying the account's domain.
pub const VAR_DOMAIN: &str = "domain";
/// Variable carrying the account's username.
pub const VAR_USERNAME: &str = "username";
/// Variable carrying the pairing PIN.
pub const VAR_PIN: &str = "pin";
/// Variable carrying the password style ([`Style::spec`]).
pub const VAR_STYLE: &str = "style";
/// Variable carrying a history entry's id.
pub const VAR_ID: &str = "id";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CopyPassword {
        domain: String,
        username: String,
    },
    CopyUsername {
        domain: String,
        username: String,
    },
    Generate {
        style: Style,
    },
    GenerateAndSave {
        domain: String,
        username: String,
        style: Style,
    },
    /// Copy a password from the history again.
    CopyGenerated {
        id: String,
    },
    RequestPin,
    SubmitPin(String),
}

/// What [`run`] needs from the outside world.
pub struct Context<'a> {
    pub apw: &'a Apw<'a>,
    pub clipboard: &'a dyn Clipboard,
    pub generate: &'a dyn Fn(&Style) -> Result<String>,
    pub history: &'a dyn HistoryVault,
    /// A fresh random id for a history entry.
    pub new_id: &'a dyn Fn() -> Result<String>,
    /// Unix time, in seconds.
    pub now: u64,
}

impl Action {
    /// The Alfred variables that encode this action on an item.
    pub fn variables(&self) -> Vec<(&'static str, String)> {
        let mut variables = vec![(VAR_ACTION, self.name().to_owned())];

        match self {
            Action::CopyPassword { domain, username }
            | Action::CopyUsername { domain, username } => {
                variables.push((VAR_DOMAIN, domain.clone()));
                variables.push((VAR_USERNAME, username.clone()));
            }
            Action::GenerateAndSave {
                domain,
                username,
                style,
            } => {
                variables.push((VAR_DOMAIN, domain.clone()));
                variables.push((VAR_USERNAME, username.clone()));
                variables.push((VAR_STYLE, style.spec()));
            }
            Action::Generate { style } => variables.push((VAR_STYLE, style.spec())),
            Action::CopyGenerated { id } => variables.push((VAR_ID, id.clone())),
            Action::SubmitPin(pin) => variables.push((VAR_PIN, pin.clone())),
            Action::RequestPin => {}
        }

        variables
    }

    /// Decode the action from the variables Alfred exported, read with `get`.
    pub fn from_variables(get: impl Fn(&str) -> Option<String>) -> Result<Action> {
        let required = |name: &str| {
            get(name)
                .filter(|value| !value.is_empty())
                .with_context(|| format!("missing the `{name}` variable"))
        };
        let style = || Style::from_spec(&required(VAR_STYLE)?);
        let action = required(VAR_ACTION)?;

        Ok(match action.as_str() {
            "copy-password" => Action::CopyPassword {
                domain: required(VAR_DOMAIN)?,
                username: required(VAR_USERNAME)?,
            },
            "copy-username" => Action::CopyUsername {
                domain: required(VAR_DOMAIN)?,
                username: required(VAR_USERNAME)?,
            },
            "generate" => Action::Generate { style: style()? },
            "generate-and-save" => Action::GenerateAndSave {
                domain: required(VAR_DOMAIN)?,
                username: required(VAR_USERNAME)?,
                style: style()?,
            },
            "copy-generated" => Action::CopyGenerated {
                id: required(VAR_ID)?,
            },
            "request-pin" => Action::RequestPin,
            "submit-pin" => Action::SubmitPin(required(VAR_PIN)?),
            other => bail!("unknown action `{other}`"),
        })
    }

    /// The domain whose account this action uses, if any.
    pub fn domain(&self) -> Option<&str> {
        match self {
            Action::CopyPassword { domain, .. }
            | Action::CopyUsername { domain, .. }
            | Action::GenerateAndSave { domain, .. } => Some(domain),
            Action::Generate { .. }
            | Action::CopyGenerated { .. }
            | Action::RequestPin
            | Action::SubmitPin(_) => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Action::CopyPassword { .. } => "copy-password",
            Action::CopyUsername { .. } => "copy-username",
            Action::Generate { .. } => "generate",
            Action::GenerateAndSave { .. } => "generate-and-save",
            Action::CopyGenerated { .. } => "copy-generated",
            Action::RequestPin => "request-pin",
            Action::SubmitPin(_) => "submit-pin",
        }
    }
}

/// Run `action` and return the message for the notification.
pub fn run(action: &Action, ctx: &Context) -> Result<String> {
    match action {
        Action::CopyPassword { domain, username } => {
            let password = ctx.apw.password(domain, username).map_err(explain)?;
            ctx.clipboard.copy(&password, true)?;

            Ok(format!("Password of {username} ({domain}) copied"))
        }
        Action::CopyUsername { domain, username } => {
            ctx.clipboard.copy(username, false)?;

            Ok(format!("Username {username} ({domain}) copied"))
        }
        Action::Generate { style } => {
            let password = (ctx.generate)(style)?;
            ctx.clipboard.copy(&password, true)?;

            Ok(with_history(
                "Strong password copied".to_owned(),
                keep(ctx, password, style, None),
            ))
        }
        Action::GenerateAndSave {
            domain,
            username,
            style,
        } => {
            let password = (ctx.generate)(style)?;
            ctx.apw.save(domain, username, &password).map_err(explain)?;
            ctx.clipboard.copy(&password, true)?;

            Ok(with_history(
                format!("Password saved for {username} ({domain}) and copied"),
                keep(ctx, password, style, Some(format!("{username} ({domain})"))),
            ))
        }
        Action::CopyGenerated { id } => {
            let history = ctx.history.load()?;
            let entry = history
                .find(id)
                .context("this password is no longer in the history")?;
            ctx.clipboard.copy(&entry.password, true)?;

            Ok(format!(
                "Password {} copied again",
                History::preview(&entry.password)
            ))
        }
        Action::RequestPin => {
            ctx.apw.request_pin().map_err(explain)?;

            Ok("Type `pw pin <code>` with the PIN macOS shows".to_owned())
        }
        Action::SubmitPin(pin) => {
            ctx.apw.submit_pin(pin).map_err(explain)?;

            Ok("apw is paired with Apple Passwords".to_owned())
        }
    }
}

/// Turn an [`ApwError`] into a message that says what to do next.
pub fn explain(err: ApwError) -> anyhow::Error {
    let hint = match err {
        ApwError::NotInstalled => "install it with `brew install bendews/homebrew-tap/apw`",
        ApwError::NotRunning => "start it with `brew services start apw`, then type `pw pin`",
        ApwError::NotPaired => "type `pw pin` to pair it",
        ApwError::TimedOut | ApwError::Failed(_) => return anyhow::Error::new(err),
    };

    anyhow::anyhow!("{err}: {hint}")
}

/// Add a generated password to the history. A history that can no longer be
/// decrypted is replaced, since its content is lost anyway; any other failure
/// (Keychain access denied) leaves it untouched.
fn keep(ctx: &Context, password: String, style: &Style, saved_for: Option<String>) -> Result<()> {
    let mut history = match ctx.history.load() {
        Ok(history) => history,
        Err(err) if err.downcast_ref::<Unreadable>().is_some() => History::default(),
        Err(err) => return Err(err),
    };

    history.push(Entry {
        id: (ctx.new_id)()?,
        password,
        created_at: ctx.now,
        style: style.summary(),
        saved_for,
    });

    ctx.history.save(&history)
}

/// The password is already copied: a history failure only adds a note.
fn with_history(message: String, kept: Result<()>) -> String {
    match kept {
        Ok(()) => message,
        Err(err) => format!("{message} (not kept in history: {err:#})"),
    }
}
