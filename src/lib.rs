//! Core library for the `alfred-passwords` Alfred workflow.
//!
//! The binary is a thin dispatcher over these modules. Parsing, item building,
//! password generation and the history model are pure and unit tested; the
//! impure boundaries are [`runner::CommandRunner`] (spawning `apw`),
//! [`clipboard::Clipboard`] and the Keychain behind [`vault::KeyStore`], all
//! replaced by fakes in tests.

pub mod actions;
pub mod alfred;
pub mod apw;
pub mod clipboard;
pub mod domains;
pub mod generator;
pub mod history;
pub mod query;
pub mod runner;
pub mod vault;
