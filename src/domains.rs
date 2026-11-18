//! Local index of the domains already looked up, for autocompletion.
//!
//! Apple Passwords can only be queried for one exact domain: there is no way to
//! list every account. So the workflow remembers the domains that returned
//! accounts and suggests them while the user types (`pw git` → `github.com`).
//!
//! Only domain names are stored, never usernames or passwords, in
//! `domains.json` under Alfred's workflow data folder. A missing or malformed
//! file is an empty index, never an error: suggestions are best-effort.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// What the index knows about one domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// How many times an account of this domain was used (password or
    /// username copied, password saved).
    pub uses: u32,
    /// Unix time, in seconds, of the last use or of the first lookup.
    pub last_seen: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DomainIndex {
    domains: BTreeMap<String, Entry>,
}

impl DomainIndex {
    /// File name of the index in the workflow data folder.
    const FILE_NAME: &'static str = "domains.json";

    /// Age, in days, after which a domain's weight is halved.
    const HALF_LIFE_DAYS: f64 = 30.0;

    const SECONDS_PER_DAY: f64 = 86_400.0;

    /// Where the index lives, given Alfred's `alfred_workflow_data` folder.
    pub fn path(data_dir: Option<&Path>) -> Option<PathBuf> {
        data_dir
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join(DomainIndex::FILE_NAME))
    }

    /// Read the index from `path`; missing or malformed means empty.
    pub fn load(path: &Path) -> DomainIndex {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write the index to `path` atomically, creating its folder if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }

        let temporary = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self).context("failed to encode the index")?;

        fs::write(&temporary, json)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        fs::rename(&temporary, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;

        Ok(())
    }

    /// The known domains, as read-only entries.
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.domains
    }

    /// Add `domain` if it is not known yet. Returns whether the index changed,
    /// so a lookup repeated on every keystroke does not rewrite the file.
    pub fn remember(&mut self, domain: &str, now: u64) -> bool {
        if self.domains.contains_key(domain) {
            return false;
        }

        self.domains.insert(
            domain.to_owned(),
            Entry {
                uses: 0,
                last_seen: now,
            },
        );

        true
    }

    /// Update the index after a lookup of `domain`: remember it when it has
    /// accounts, forget it when it has none (wrong domain, or every account
    /// deleted). Returns whether the index changed.
    pub fn observe(&mut self, domain: &str, has_accounts: bool, now: u64) -> bool {
        if has_accounts {
            return self.remember(domain, now);
        }

        self.forget(domain)
    }

    /// Count one use of `domain`, adding it if needed.
    pub fn record_use(&mut self, domain: &str, now: u64) {
        let entry = self.domains.entry(domain.to_owned()).or_insert(Entry {
            uses: 0,
            last_seen: now,
        });

        entry.uses = entry.uses.saturating_add(1);
        entry.last_seen = entry.last_seen.max(now);
    }

    /// Drop `domain`, e.g. once it no longer has any account. Returns whether
    /// the index changed.
    pub fn forget(&mut self, domain: &str) -> bool {
        self.domains.remove(domain).is_some()
    }

    /// Known domains matching `input`, best first, at most `limit`.
    ///
    /// A domain matches when it contains `input` (case-insensitive). Domains
    /// where one of the dot-separated labels starts with `input` come first
    /// (`hub` ranks `hub.docker.com` above `github.com`), then by frecency:
    /// uses plus one, halved every 30 days since last seen. An empty `input`
    /// lists every domain by frecency.
    pub fn suggest(&self, input: &str, now: u64, limit: usize) -> Vec<String> {
        let needle = input.trim().to_ascii_lowercase();

        let mut matches: Vec<(bool, f64, &String)> = self
            .domains
            .iter()
            .filter(|(domain, _)| domain.contains(&needle))
            .map(|(domain, entry)| {
                let label_prefix = domain.split('.').any(|label| label.starts_with(&needle))
                    || domain.starts_with(&needle);

                (label_prefix, DomainIndex::score(entry, now), domain)
            })
            .collect();

        matches.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.total_cmp(&a.1))
                .then_with(|| a.2.cmp(b.2))
        });

        matches
            .into_iter()
            .take(limit)
            .map(|(_, _, domain)| domain.clone())
            .collect()
    }

    fn score(entry: &Entry, now: u64) -> f64 {
        let age_days = now.saturating_sub(entry.last_seen) as f64 / DomainIndex::SECONDS_PER_DAY;

        (f64::from(entry.uses) + 1.0) * 0.5f64.powf(age_days / DomainIndex::HALF_LIFE_DAYS)
    }
}
