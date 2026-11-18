//! Writing to the macOS clipboard.
//!
//! Secrets are tagged `org.nspasteboard.ConcealedType`, the convention
//! clipboard managers (Alfred's clipboard history included) follow to leave an
//! entry out of their history.

use anyhow::{bail, Result};
use objc2_app_kit::NSPasteboard;
use objc2_foundation::NSString;

/// Abstraction over the clipboard, so actions can be tested offline.
pub trait Clipboard {
    /// Replace the clipboard with `text`. A `concealed` entry is kept out of
    /// clipboard histories.
    fn copy(&self, text: &str, concealed: bool) -> Result<()>;
}

/// The general pasteboard of the logged-in user.
pub struct SystemClipboard;

impl SystemClipboard {
    const PLAIN_TEXT: &'static str = "public.utf8-plain-text";
    const CONCEALED: &'static str = "org.nspasteboard.ConcealedType";
}

impl Clipboard for SystemClipboard {
    fn copy(&self, text: &str, concealed: bool) -> Result<()> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let value = NSString::from_str(text);

        pasteboard.clearContents();

        if concealed
            && !pasteboard
                .setString_forType(&value, &NSString::from_str(SystemClipboard::CONCEALED))
        {
            bail!("could not mark the clipboard entry as concealed");
        }

        if !pasteboard.setString_forType(&value, &NSString::from_str(SystemClipboard::PLAIN_TEXT)) {
            bail!("could not write to the clipboard");
        }

        Ok(())
    }
}
