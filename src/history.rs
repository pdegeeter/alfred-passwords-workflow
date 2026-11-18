//! The last generated passwords, so `pwg` can copy one again.
//!
//! This module is the pure model: entries, the 20-entry cap, the masked
//! preview and the age shown in Alfred. Storage, encrypted on disk with a key
//! from the Keychain, is in [`crate::vault`].

use serde::{Deserialize, Serialize};

/// One generated password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Random identifier, carried by the Alfred item instead of the password.
    pub id: String,
    pub password: String,
    /// Unix time, in seconds.
    pub created_at: u64,
    /// The style it was generated with ([`crate::generator::Style::summary`]).
    pub style: String,
    /// `"<username> (<domain>)"` when it was saved to Apple Passwords.
    pub saved_for: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// Newest first.
    entries: Vec<Entry>,
}

impl History {
    /// How many passwords are kept.
    pub const MAX_ENTRIES: usize = 20;

    /// Characters of the password shown before the mask.
    const PREVIEW_CHARS: usize = 4;

    /// Fixed mask, so the preview does not reveal the length.
    const MASK: &'static str = "****";

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Add `entry` as the newest, dropping the oldest beyond the cap.
    pub fn push(&mut self, entry: Entry) {
        self.entries.insert(0, entry);
        self.entries.truncate(History::MAX_ENTRIES);
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The first four characters, then a fixed `****`.
    pub fn preview(password: &str) -> String {
        let shown: String = password.chars().take(History::PREVIEW_CHARS).collect();

        format!("{shown}{}", History::MASK)
    }

    /// How long ago `created_at` was, from `now`, for humans.
    pub fn age(created_at: u64, now: u64) -> String {
        let seconds = now.saturating_sub(created_at);

        match seconds {
            0..=59 => "just now".to_owned(),
            60..=3_599 => format!("{} min ago", seconds / 60),
            3_600..=86_399 => format!("{} h ago", seconds / 3_600),
            86_400..=172_799 => "yesterday".to_owned(),
            _ => format!("{} days ago", seconds / 86_400),
        }
    }
}
