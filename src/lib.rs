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
pub mod context;
pub mod error;
pub mod inference;
pub mod model;

/// The run loop: request in, `Output` out.
pub mod runtime;

/// Shell hook installation. Writes only inside its own markers (ADR 0007).
pub mod shell;

/// Independent risk classification. Never imports the model (ADR 0004).
pub mod safety;
pub mod ui;
pub mod utils;

pub use error::{Error, Result};

pub use runtime::{run, Consenter, Decision, DenyAll};

/// What the tool decided to do, and why.
///
/// This is data, not text. `--json` serialises it, `ui/` renders it, and the
/// history store records it. Nothing here is pre-formatted for a terminal, which
/// is what keeps `--json` honest rather than a scrape of coloured output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The command, cleaned and classified. Empty for a mode that produces none.
    pub command: String,
    /// Its risk level. [`Risk::Safe`] when there was nothing to classify.
    pub level: safety::Risk,
    /// The mode that produced this, as a name a user would type.
    pub mode: String,
    /// Why the level is what it is, in the order the classifier found them.
    pub reasons: Vec<String>,
    /// The per-segment breakdown, for an explain.
    pub segments: Vec<String>,
    /// The plain-language explanation, for `--explain`.
    pub explanation: Option<String>,
    /// Whether the command was actually executed.
    ///
    /// Never `true` without a `Consent::Granted` and a level that permits it.
    pub executed: bool,
}

impl Output {
    /// The one JSON object `--json` prints, hand-built for the same reason
    /// `context/prompt.rs` hand-builds its prompt: no serde dependency in the
    /// safety or core path at MSRV 1.75.
    ///
    /// Escape order matters. `\\` first, then `"`, then the control characters,
    /// or a value containing `\"` would come out with a literal backslash in it.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(256);
        out.push_str("{\"command\":");
        json_string(&self.command, &mut out);
        out.push_str(",\"level\":");
        json_string(self.level.as_str(), &mut out);
        out.push_str(",\"mode\":");
        json_string(&self.mode, &mut out);
        out.push_str(",\"reasons\":[");
        for (i, reason) in self.reasons.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            json_string(reason, &mut out);
        }
        out.push_str("],\"segments\":[");
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            json_string(segment, &mut out);
        }
        out.push_str("],\"executed\":");
        out.push_str(if self.executed { "true" } else { "false" });
        // Always present, null when there is none. A key that appears only
        // sometimes makes a consumer guess whether it was omitted or empty, and
        // for the hook modes the explanation is the whole result — leaving it out
        // would make `gcode --json --init` print an object that says nothing.
        out.push_str(",\"explanation\":");
        match &self.explanation {
            Some(text) => json_string(text, &mut out),
            None => out.push_str("null"),
        }
        out.push('}');
        out
    }
}

/// Appends a JSON string literal, escaping what RFC 8259 requires.
///
/// `pub(crate)` so the model registry can render its `--list-models --json`
/// array with the same escaper this module uses for [`Output`], rather than a
/// second copy that could disagree on an edge case.
pub(crate) fn json_string(value: &str, out: &mut String) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::Write::write_fmt(out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
