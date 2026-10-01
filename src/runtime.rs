//! The run loop: request in, [`Output`] out.
//!
//! # Why `run` takes a `Consenter` instead of asking
//!
//! The safety invariants in AGENTS.md §3 are about what happens when a command is
//! dangerous, and none of them can be tested if the answer depends on whether a
//! terminal happened to be attached. So the decision to proceed is a parameter,
//! not a side effect. `ui::prompt::Prompt` implements [`Consenter`] for real
//! use; [`DenyAll`] implements it for a pipe or a CI job.
//!
//! This is also what keeps the dependency rule in `lib.rs` honest: the core
//! depends on the [`Consenter`] trait, never on `ui`. A test can construct a
//! `Consenter` that answers either way without a terminal, and a caller can
//! never accidentally wire the prompt into `--json`.
//!
//! # The order of operations is the safety property
//!
//! Classify → refuse if blocked → consent unless `--yes` → then act. Nothing may
//! reorder these. In particular the classifier runs *before* the prompt, so the
//! prompt can show the level, and it runs before anything that could have a side
//! effect, so no side effect is possible before the level is known.

use crate::cli::{Mode, Parsed};
use crate::error::{Error, Result};
use std::sync::Arc;

use crate::inference::{GenParams, InferenceEngine};
use crate::safety::{self, Risk};

/// A [`Output`] re-export is defined in `lib.rs`; this module owns its
/// construction.
use crate::Output;

/// What the user decided, and the command to act on.
///
/// The command travels *with* the decision rather than the caller assuming it is
/// unchanged, because a prompt may hand back an edited one. Returning
/// [`Decision::Granted`] with a string the caller then ignores is how a tool ends
/// up classifying one command and running another, so the type makes that
/// impossible to express.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Proceed, with this exact command. The core re-classifies it before use.
    Granted(String),
    /// Do not proceed. The default at every level, and the only answer available
    /// without a terminal.
    Denied,
}

/// Something that can ask the user whether to proceed.
///
/// The trait exists so the run loop is testable and so `ui` stays a leaf. An
/// implementation must treat "no answer available" as [`Decision::Denied`];
/// there is no third variant, so forgetting that decision is a compile error
/// rather than a silent `true`.
pub trait Consenter {
    /// Asks about `command`, already classified as `verdict`.
    ///
    /// May be called more than once per run: a prompt that loops on `?` or lets
    /// the user edit and retry calls it again with a fresh classification.
    fn ask(&mut self, command: &str, verdict: &safety::Verdict) -> Decision;
}

/// A [`Consenter`] that always says no.
///
/// This is what a pipe, a CI job, and `--json` get. Passing this is how the run
/// loop fails closed without knowing anything about the terminal: if there is no
/// one to ask, nothing is confirmed, so nothing above `SAFE` runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAll;

impl Consenter for DenyAll {
    fn ask(&mut self, _command: &str, _verdict: &safety::Verdict) -> Decision {
        Decision::Denied
    }
}

/// Runs gcode for an already-parsed command line.
///
/// # Errors
///
/// Returns an error for a blocked command, for a command above `SAFE` that was
/// not confirmed, for a mode with no engine yet, and for any engine failure.
///
/// Note what is *not* an error: a `SAFE` command never needs consent, so it runs
/// without a `Consenter` at all.
pub fn run(
    parsed: &Parsed,
    engine: Option<Arc<dyn InferenceEngine>>,
    consenter: &mut dyn Consenter,
    always_confirm: Risk,
) -> Result<Output> {
    match &parsed.mode {
        Mode::Explain { command } => Ok(explain(command, parsed)),
        Mode::Generate { request } => {
            let engine = engine.ok_or(Error::NoEngine)?;
            let params = GenParams::default();
            let command =
                crate::inference::generate_command(&engine, request, &params).map_err(|e| {
                    Error::Inference {
                        message: e.to_string(),
                    }
                })?;
            gate(command, parsed, consenter, always_confirm)
        }
        Mode::Complete { partial } => {
            let engine = engine.ok_or(Error::NoEngine)?;
            let params = GenParams::default();
            let generated =
                crate::inference::generate_command(&engine, partial, &params).map_err(|e| {
                    Error::Inference {
                        message: e.to_string(),
                    }
                })?;
            gate(generated, parsed, consenter, always_confirm)
        }
        Mode::Fix | Mode::Interactive => Err(Error::ModeNotWired {
            mode: parsed.mode.to_string(),
        }),
    }
}

/// Converts the config layer's risk level into the classifier's.
///
/// Deliberately lives here and not in `src/safety/`. Two enums with the same five
/// names exist because the classifier must import nothing (ADR 0004) while
/// `config` needs its own for `#[serde(rename_all = "UPPERCASE")]`. Putting the
/// conversion in the layer that already knows about both keeps
/// `src/safety/` a pure function of a string, and keeps the mapping in one
/// place instead of at every comparison site.
impl From<crate::config::RiskLevel> for Risk {
    fn from(level: crate::config::RiskLevel) -> Self {
        match level {
            crate::config::RiskLevel::Safe => Self::Safe,
            crate::config::RiskLevel::Low => Self::Low,
            crate::config::RiskLevel::Medium => Self::Medium,
            crate::config::RiskLevel::High => Self::High,
            crate::config::RiskLevel::Critical => Self::Critical,
        }
    }
}

/// Whether a command at `level` must be confirmed.
///
/// Prompt when the level is at or above `always_confirm`, and `--yes` was not
/// given. `--yes` suppresses the prompt at every level and does nothing else:
/// classification, the reasons, and the block all still apply, which is
/// AGENTS.md §3 invariant 5 and the roadmap's `--yes` test ("`--yes` on a HIGH
/// command runs").
///
/// # A contradiction in the existing docs, and how it was resolved
///
/// `config.rs` described this field as "the lowest risk that always prompts,
/// **regardless of `--yes`**". That cannot be implemented together with the
/// roadmap's own test for 3.7, because the default value is `MEDIUM` and
/// making the floor survive `--yes` would mean `--yes` could never suppress a
/// prompt at all — the flag would be decoration on every default install.
///
/// The roadmap is the build contract (AGENTS.md §1), so `--yes` wins and the
/// field comment was corrected. `always_confirm` remains fully meaningful: it
/// decides *which* levels prompt, and raising it to `HIGH` makes `MEDIUM`
/// commands run unattended. Only the "regardless of `--yes`" clause was wrong.
#[must_use]
pub fn needs_confirmation(level: Risk, always_confirm: Risk, yes: bool) -> bool {
    !yes && level >= always_confirm
}

/// The default prompt threshold, mirroring the config default.
///
/// Named here rather than imported so the run loop's rule reads in one place. The
/// two must agree, and a test in `config` asserts that they do.
pub const DEFAULT_CONFIRM_AT: Risk = Risk::Medium;

/// Classifies `command` and decides whether it may proceed.
///
/// This is the single choke point through which every generated command passes.
/// `--yes` and `--dry-run` change what happens *after* this point; they cannot
/// change the level, and they cannot unblock.
fn gate(
    command: String,
    parsed: &Parsed,
    consenter: &mut dyn Consenter,
    always_confirm: Risk,
) -> Result<Output> {
    let verdict = safety::classify(&command);

    // CRITICAL is refused here, before the prompt, before the executor. There is
    // no flag, no config key, and no branch after this that runs the command.
    if !verdict.level.is_runnable() {
        return Err(Error::RiskBlocked {
            level: verdict.level,
            reasons: verdict.reason_messages(),
        });
    }

    // `--dry-run` stops here: print and exit 0, having classified. A dry run that
    // asked for consent would defeat the point of the flag.
    if parsed.dry_run {
        return Ok(output(&command, &verdict, &parsed.mode.to_string(), false));
    }

    // Consent. Suppressed by `--yes`, and only by `--yes`.
    //
    // A prompt may return a *different* command than the one offered, so the
    // returned string is re-classified from scratch before anything else looks at
    // it. This is invariant 3 — editing re-runs classification — and it lives
    // here rather than in `ui`, so no consenter implementation can skip it.
    let mut command = command;
    let mut verdict = verdict;
    if needs_confirmation(verdict.level, always_confirm, parsed.yes) {
        match consenter.ask(&command, &verdict) {
            Decision::Denied => {
                return Err(Error::ConsentDenied {
                    level: verdict.level,
                });
            }
            Decision::Granted(edited) => {
                if edited != command {
                    let rechecked = safety::classify(&edited);
                    if !rechecked.level.is_runnable() {
                        return Err(Error::RiskBlocked {
                            level: rechecked.level,
                            reasons: rechecked.reason_messages(),
                        });
                    }
                    command = edited;
                    verdict = rechecked;
                }
            }
        }
    }

    // Execution. A real shell invocation is Phase 2.3; until then this returns
    // the verdict rather than pretending the command ran, and `executed: false`
    // says so in `--json` so nothing downstream can mistake it for a success.
    //
    // The engine is deliberately *not* a parameter here. An earlier draft carried
    // it through and discarded it with `let _ = engine;`, which made it look like
    // the executor was already wired. When 2.3 needs the engine, it will take it,
    // and the change will be visible in the diff.
    Ok(output(&command, &verdict, &parsed.mode.to_string(), false))
}

/// Builds an [`Output`] from a command and its verdict.
fn output(command: &str, verdict: &safety::Verdict, mode: &str, executed: bool) -> Output {
    Output {
        command: command.to_owned(),
        level: verdict.level,
        mode: mode.to_owned(),
        reasons: verdict.reason_messages(),
        segments: verdict.segments.iter().map(|s| s.raw.clone()).collect(),
        explanation: None,
        executed,
    }
}

/// Implements `--explain`: classify, describe, never execute, never suggest.
///
/// This mode needs no model and no consent, because it runs nothing. That makes
/// it the one mode that is fully usable before the inference pipeline lands.
fn explain(command: &str, _parsed: &Parsed) -> Output {
    let verdict = safety::classify(command);
    let mut out = output(command, &verdict, "explain", false);
    out.explanation = Some(verdict.explanation());
    // An explain of a CRITICAL command is not an error. The user asked what it
    // does; refusing to answer would make the tool useless for the command most
    // worth understanding. Only `run` decides what is an error.
    out
}
