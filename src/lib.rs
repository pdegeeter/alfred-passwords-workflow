//! Core library for the `alfred-passwords` Alfred workflow.
//!
//! The binary is a thin dispatcher over these modules. Parsing, item building
//! and password generation are pure and unit tested; the impure boundaries are
//! [`runner::CommandRunner`] (spawning `apw`) and [`clipboard::Clipboard`],
//! both replaced by fakes in tests.

pub mod actions;
pub mod alfred;
pub mod apw;
pub mod clipboard;
pub mod generator;
pub mod query;
pub mod runner;
