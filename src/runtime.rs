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

use crate::context::history::History;
use crate::context::prompt::{build_prompt, Context, HistoryEntry as PromptHistoryEntry};
use crate::inference::{GenParams, InferenceEngine};
use crate::safety::{self, Risk};
use crate::shell::install::{changed_lines, Installer};
use crate::utils::paths::Overrides;

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
            let prompt = build_prompt(request, &Context::default());
            let command =
                crate::inference::generate_command(&engine, &prompt, &params).map_err(|e| {
                    Error::Inference {
                        message: e.to_string(),
                    }
                })?;
            gate(command, parsed, consenter, always_confirm)
        }
        Mode::Complete { partial } => {
            let engine = engine.ok_or(Error::NoEngine)?;
            let params = GenParams::default();
            let prompt = build_prompt(&complete_request(partial), &Context::default());
            let generated =
                crate::inference::generate_command(&engine, &prompt, &params).map_err(|e| {
                    Error::Inference {
                        message: e.to_string(),
                    }
                })?;
            gate(generated, parsed, consenter, always_confirm)
        }
        Mode::Init | Mode::Check | Mode::Remove => shell_mode(parsed),
        Mode::Fix => {
            let history = History::with_overrides(&Overrides::from_env()?)?;
            fix(engine, parsed, consenter, always_confirm, &history)
        }
        Mode::Interactive => Err(Error::ModeNotWired {
            mode: parsed.mode.to_string(),
        }),
    }
}

/// How many recent entries are searched for a failure.
///
/// Wider than the prompt's own history window on purpose: the failed command may
/// be older than the last few commands, and reporting "no failed command" while
/// one sits just outside the window would be a lie. Reading backwards is cheap.
const FIX_SEARCH_DEPTH: usize = 200;

/// The instruction handed to the model when repairing.
const FIX_REQUEST: &str = "The most recent command in the history failed. Give one corrected \
     shell command that fixes it. Reply with the command only.";

/// Implements `--fix`.
///
/// Finds the most recent entry with a non-zero exit, assembles the prompt through
/// [`build_prompt`] so the failure is redacted and escaped exactly like every
/// other piece of context (ADR 0006), and feeds the model's answer through
/// [`gate`] like any other generated command. A clean history is not an error:
/// there is simply nothing to repair, so this returns an [`Output`] carrying that
/// message and no command.
///
/// # Errors
///
/// [`Error::NoEngine`] when a failure exists and no model is loaded,
/// [`Error::History`] when the store cannot be read, and the classification,
/// block, and consent errors [`gate`] can return.
pub fn fix(
    engine: Option<Arc<dyn InferenceEngine>>,
    parsed: &Parsed,
    consenter: &mut dyn Consenter,
    always_confirm: Risk,
    history: &History,
) -> Result<Output> {
    if parsed.no_history {
        return Ok(no_failure());
    }
    let entries = history.read_last(FIX_SEARCH_DEPTH)?;
    let Some(failed_at) = entries.iter().rposition(|entry| entry.exit != 0) else {
        return Ok(no_failure());
    };
    let engine = engine.ok_or(Error::NoEngine)?;

    // Only entries up to and including the failure. `build_prompt` keeps the
    // newest `MAX_HISTORY_ENTRIES`, so ending the slice at the failure guarantees
    // the failed command is shown even when it is older than that window.
    let context = Context {
        history: entries[..=failed_at]
            .iter()
            .map(|entry| {
                let mut prompt_entry = PromptHistoryEntry::new(&entry.cmd, entry.exit);
                if !entry.out.is_empty() {
                    prompt_entry.out = Some(entry.out.clone());
                }
                prompt_entry
            })
            .collect(),
        ..Context::default()
    };
    let prompt = build_prompt(FIX_REQUEST, &context);
    let params = GenParams::default();
    let generated = crate::inference::generate_command(&engine, &prompt, &params).map_err(|e| {
        Error::Inference {
            message: e.to_string(),
        }
    })?;

    let original = entries[failed_at].cmd.clone();
    let mut out = gate(generated, parsed, consenter, always_confirm)?;
    out.explanation = Some(fix_explanation(&original, &out.command));
    Ok(out)
}

/// The `Output` for a `--fix` with nothing to repair.
fn no_failure() -> Output {
    Output {
        command: String::new(),
        level: Risk::Safe,
        mode: Mode::Fix.to_string(),
        reasons: Vec::new(),
        segments: Vec::new(),
        explanation: Some("no failed command in history; nothing to fix".to_owned()),
        executed: false,
    }
}

/// How the repaired command is shown against the one that failed.
fn fix_explanation(original: &str, corrected: &str) -> String {
    if original == corrected {
        format!("the model returned the original command unchanged:\n  {original}")
    } else {
        format!("fix:\n  - {original}\n  + {corrected}")
    }
}

/// The instruction wrapped around a `--complete` partial.
///
/// The partial is the user's own text, so it goes through [`build_prompt`] and is
/// redacted like every other string from the shell (ADR 0006). The model returns
/// the completed command; the emitted string is that whole command, and it is the
/// whole command that is classified and gated, so invariant 1 holds.
const COMPLETE_REQUEST: &str =
    "Complete this partial shell command, keeping the text already typed. \
     Reply with the completed command only.\n";

/// The request body for `--complete`.
fn complete_request(partial: &str) -> String {
    format!("{COMPLETE_REQUEST}{partial}")
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

/// Runs one of the three shell-integration modes.
///
/// These are not command modes, so the [`Output`] they produce has no command and
/// no risk level: the report lives in `explanation` and `executed` is `false`,
/// because installing a hook runs nothing. They still go through `run` rather
/// than being special-cased in `main`, so that there is exactly one place where
/// a mode is dispatched.
///
/// # Errors
///
/// Returns [`Error::HomeDirUnavailable`] when no home directory is known,
/// [`Error::UnsupportedShell`] when `$SHELL` is neither bash nor zsh, the four
/// `Shell*` I/O variants when a file cannot be read or written, and
/// [`Error::ShellBlockMalformed`] when an rc file has an unterminated block.
fn shell_mode(parsed: &Parsed) -> Result<Output> {
    let kind = match parsed.shell {
        Some(kind) => kind,
        None => shell_kind_from_env()?,
    };
    let installer = Installer::resolve(kind, &Overrides::from_env()?)?;
    shell(&parsed.mode, &installer)
}

/// Runs one of the three hook modes against an explicit [`Installer`].
///
/// The seam that makes these modes testable. [`shell_mode`] resolves the shell
/// from the real `$SHELL` and the paths from the real home directory, which a test
/// must never touch; this takes both, so every behaviour worth asserting — the
/// report text, the exact lines printed, the failure modes — can be checked
/// against a temporary directory.
///
/// # Errors
///
/// Returns the `Shell*` I/O variants, [`Error::ShellBlockMalformed`] for an
/// unterminated block, and [`Error::ModeNotWired`] for a mode that is not one of
/// the three.
pub fn shell(mode: &Mode, installer: &Installer) -> Result<Output> {
    let explanation = match mode {
        Mode::Init => {
            let plan = installer.init()?;
            report(mode, &plan, installer)
        }
        Mode::Check => match installer.check()? {
            crate::shell::Status::Missing => format!(
                "not installed\n\nrun `gcode --init` to install it into {}",
                installer.rc_path.display()
            ),
            crate::shell::Status::Installed { version, current } => {
                let state = if current { "current" } else { "out of date" };
                format!(
                    "installed ({state}, version {version})\n{}",
                    installer.rc_path.display()
                )
            }
        },
        Mode::Remove => {
            let plan = installer.remove()?;
            report(mode, &plan, installer)
        }
        _ => {
            return Err(Error::ModeNotWired {
                mode: mode.to_string(),
            })
        }
    };

    Ok(Output {
        command: String::new(),
        level: Risk::Safe,
        mode: mode.to_string(),
        reasons: Vec::new(),
        segments: Vec::new(),
        explanation: Some(explanation),
        executed: false,
    })
}

/// Reads `$SHELL` and resolves it to a supported shell.
///
/// # Errors
///
/// Returns [`Error::UnsupportedShell`] when `$SHELL` is unset or names a shell
/// gcode has no hook for.
fn shell_kind_from_env() -> Result<crate::shell::Kind> {
    let value = std::env::var_os("SHELL").ok_or_else(|| Error::UnsupportedShell {
        shell: String::from("unset"),
    })?;
    crate::shell::Kind::from_shell_var(&value)
}

/// Renders an install or remove report.
///
/// The exact lines are included whenever anything was written, because a user who
/// is about to have their rc file edited deserves to see the diff in the terminal
/// rather than having to open the file to find out what changed.
fn report(mode: &Mode, plan: &crate::shell::Plan, installer: &Installer) -> String {
    let outcome = plan.outcome;
    let verb = match outcome {
        crate::shell::Outcome::Added => "installed",
        crate::shell::Outcome::Updated => "updated",
        crate::shell::Outcome::Removed => "removed",
        crate::shell::Outcome::Unchanged => "already installed",
        crate::shell::Outcome::NotInstalled => "not installed",
    };
    let what = if mode.is_shell_hook() && matches!(mode, Mode::Remove) {
        "from"
    } else {
        "in"
    };
    let mut text = format!(
        "{verb} the shell hook {what} {}\nhook: {}",
        installer.rc_path.display(),
        installer.hook_path.display()
    );
    let lines = changed_lines(plan);
    if !lines.is_empty() {
        let heading = if matches!(mode, Mode::Remove) {
            "removed:"
        } else {
            "added:"
        };
        text.push('\n');
        text.push_str(heading);
        text.push('\n');
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
    }
    text
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
