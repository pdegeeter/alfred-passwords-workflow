//! What happens when the user picks an item: the action is encoded in the
//! item's Alfred variables, which Alfred hands to `alfred-passwords run` as
//! environment variables.
//!
//! Secrets never go through Alfred: the password is fetched or generated here
//! and written to the clipboard directly, and only a notification message is
//! printed back.

use anyhow::{bail, Context, Result};

use crate::apw::{Apw, ApwError};
use crate::clipboard::Clipboard;

/// Variable carrying the action name.
pub const VAR_ACTION: &str = "action";
/// Variable carrying the account's domain.
pub const VAR_DOMAIN: &str = "domain";
/// Variable carrying the account's username.
pub const VAR_USERNAME: &str = "username";
/// Variable carrying the pairing PIN.
pub const VAR_PIN: &str = "pin";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CopyPassword { domain: String, username: String },
    CopyUsername { domain: String, username: String },
    Generate,
    GenerateAndSave { domain: String, username: String },
    RequestPin,
    SubmitPin(String),
}

impl Action {
    /// The Alfred variables that encode this action on an item.
    pub fn variables(&self) -> Vec<(&'static str, String)> {
        let mut variables = vec![(VAR_ACTION, self.name().to_owned())];

        match self {
            Action::CopyPassword { domain, username }
            | Action::CopyUsername { domain, username }
            | Action::GenerateAndSave { domain, username } => {
                variables.push((VAR_DOMAIN, domain.clone()));
                variables.push((VAR_USERNAME, username.clone()));
            }
            Action::SubmitPin(pin) => variables.push((VAR_PIN, pin.clone())),
            Action::Generate | Action::RequestPin => {}
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
            "generate" => Action::Generate,
            "generate-and-save" => Action::GenerateAndSave {
                domain: required(VAR_DOMAIN)?,
                username: required(VAR_USERNAME)?,
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
            Action::Generate | Action::RequestPin | Action::SubmitPin(_) => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Action::CopyPassword { .. } => "copy-password",
            Action::CopyUsername { .. } => "copy-username",
            Action::Generate => "generate",
            Action::GenerateAndSave { .. } => "generate-and-save",
            Action::RequestPin => "request-pin",
            Action::SubmitPin(_) => "submit-pin",
        }
    }
}

/// Run `action` and return the message for the notification.
pub fn run(
    action: &Action,
    apw: &Apw,
    clipboard: &dyn Clipboard,
    generate: &dyn Fn() -> Result<String>,
) -> Result<String> {
    match action {
        Action::CopyPassword { domain, username } => {
            let password = apw.password(domain, username).map_err(explain)?;
            clipboard.copy(&password, true)?;

            Ok(format!("Password of {username} ({domain}) copied"))
        }
        Action::CopyUsername { domain, username } => {
            clipboard.copy(username, false)?;

            Ok(format!("Username {username} ({domain}) copied"))
        }
        Action::Generate => {
            clipboard.copy(&generate()?, true)?;

            Ok("Strong password copied".to_owned())
        }
        Action::GenerateAndSave { domain, username } => {
            let password = generate()?;
            apw.save(domain, username, &password).map_err(explain)?;
            clipboard.copy(&password, true)?;

            Ok(format!(
                "Password saved for {username} ({domain}) and copied"
            ))
        }
        Action::RequestPin => {
            apw.request_pin().map_err(explain)?;

            Ok("Type `pw pin <code>` with the PIN macOS shows".to_owned())
        }
        Action::SubmitPin(pin) => {
            apw.submit_pin(pin).map_err(explain)?;

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
