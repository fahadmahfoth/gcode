//! gcode — a local-first, offline natural-language-to-shell-command tool.
//!
//! This crate is a library target as well as a binary. The split is not
//! cosmetic: `src/safety/` has to be a pure function from a command string to a
//! verdict (ADR 0004), and `tests/` has to be able to `use gcode::safety::...`
//! rather than shelling out to a built binary. Neither is possible from a
//! binary-only crate.
//!
//! The dependency rule that keeps the core honest: `ui` may depend on anything,
//! and nothing may depend on `ui`. That is what lets the core be tested
//! headlessly and keeps `--json` a serialisation of data rather than a scrape
//! of the human-facing renderer.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod cli;
pub mod config;
pub mod error;
pub mod utils;

pub use error::{Error, Result};
