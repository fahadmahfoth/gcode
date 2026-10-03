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

use crate::context::history::{History, HistoryEntry};
use crate::context::prompt::{build_prompt, Context, HistoryEntry as PromptHistoryEntry};
use crate::exec::Executor;
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

/// Whether a config file that fails to load must stop the run.
///
/// A malformed file that is silently replaced by defaults drops the user's own
/// `always_confirm` and `blocklist` without a trace, so every mode that classifies
/// or generates fails closed. The modes that do not read the config for a
/// decision stay usable, because they are how a user diagnoses or removes a
/// broken install.
#[must_use]
pub fn config_failure_is_fatal(mode: &Mode) -> bool {
    !matches!(
        mode,
        Mode::Explain { .. } | Mode::ListModels | Mode::Check | Mode::Remove
    )
}

/// What the gate may use to run an approved command and record it.
///
/// Optional everywhere, and absent by default: with no `Runner`, no path in the
/// run loop starts a process, which is how a test, `--json`, and a pipe cannot
/// execute anything by accident.
#[derive(Clone, Copy)]
pub struct Runner<'a> {
    /// Starts the process.
    pub executor: &'a dyn Executor,
    /// Where the executed command is recorded. `None` records nothing.
    pub history: Option<&'a History>,
}

/// The sampling parameters the configuration asks for.
///
/// The seed and the wall-clock ceiling stay at their defaults: a fixed seed keeps a
/// bug report reproducible, and the timeout is a safety bound, not a preference.
#[must_use]
pub fn gen_params(config: &crate::config::EffectiveConfig) -> GenParams {
    GenParams {
        temperature: config.temperature,
        top_p: config.top_p,
        max_tokens: config.max_tokens,
        ..GenParams::default()
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
    run_configured(
        parsed,
        &crate::config::defaults(),
        engine,
        consenter,
        always_confirm,
        None,
        None,
        None,
    )
}

/// [`run`] with the resolved config and an optional download transport.
///
/// `main` calls this, because the download mode needs the config it already
/// loaded and an HTTP client it chooses by feature. The plain [`run`] keeps its
/// old signature for tests and passes no transport, so a test can never reach
/// the network by accident.
///
/// # Errors
///
/// The same errors [`run`] returns, plus the download errors: a model the
/// registry does not have, a build without the `download` feature, and a
/// transfer that fails or does not match its digest.
#[allow(clippy::too_many_arguments)]
pub fn run_configured<'a>(
    parsed: &Parsed,
    config: &'a crate::config::EffectiveConfig,
    engine: Option<Arc<dyn InferenceEngine>>,
    consenter: &mut dyn Consenter,
    always_confirm: Risk,
    transport: Option<&'a dyn crate::model::download::Transport>,
    on_progress: Option<&'a mut dyn FnMut(u64, u64)>,
    runner: Option<Runner<'a>>,
) -> Result<Output> {
    match &parsed.mode {
        Mode::Explain { command } => Ok(explain(command, parsed, &config.blocklist)),
        Mode::ListModels => list_models(),
        Mode::DownloadModel { name } => {
            let models_dir = crate::utils::paths::models_dir()?;
            download_model(name.as_deref(), config, &models_dir, transport, on_progress)
        }
        Mode::Generate { request } => {
            let engine = engine.ok_or(Error::NoEngine)?;
            let params = gen_params(config);
            let prompt = build_prompt(request, &Context::default());
            let command =
                crate::inference::generate_command(&engine, &prompt, &params).map_err(|e| {
                    Error::Inference {
                        message: e.to_string(),
                    }
                })?;
            gate(
                command,
                parsed,
                consenter,
                always_confirm,
                &config.blocklist,
                runner,
            )
        }
        Mode::Complete { partial } => {
            let engine = engine.ok_or(Error::NoEngine)?;
            let params = gen_params(config);
            let prompt = build_prompt(&complete_request(partial), &Context::default());
            let generated =
                crate::inference::generate_command(&engine, &prompt, &params).map_err(|e| {
                    Error::Inference {
                        message: e.to_string(),
                    }
                })?;
            gate(
                generated,
                parsed,
                consenter,
                always_confirm,
                &config.blocklist,
                None,
            )
        }
        Mode::Init | Mode::Check | Mode::Remove => shell_mode(parsed),
        Mode::Fix => {
            let history = History::with_overrides(&Overrides::from_env()?)?;
            fix_with_blocklist(
                engine,
                parsed,
                consenter,
                always_confirm,
                &history,
                &FixSettings {
                    blocklist: &config.blocklist,
                    params: gen_params(config),
                    runner,
                },
            )
        }
        Mode::Interactive => Err(Error::ModeNotWired {
            mode: parsed.mode.to_string(),
        }),
    }
}

/// Implements `--list-models`: the embedded registry as a table.
///
/// Needs no model, no network, and no consent, so it is usable on a fresh
/// install. The table is carried in `explanation`, because this mode produces no
/// command; `main` prints the explanation when the command is empty.
///
/// # Errors
///
/// [`Error::Registry`] if the embedded registry fails validation, which
/// `build.rs` is meant to make unreachable.
fn list_models() -> Result<Output> {
    let registry = crate::model::registry::registry()?;
    Ok(Output {
        command: String::new(),
        level: Risk::Safe,
        mode: Mode::ListModels.to_string(),
        reasons: Vec::new(),
        segments: Vec::new(),
        explanation: Some(registry.table()),
        executed: false,
        exit_code: None,
    })
}

/// The registry as the JSON array `--list-models --json` prints.
///
/// A list is an array, not the single object [`Output::to_json`] emits, so this
/// is a separate renderer rather than a field on `Output`. `main` selects it by
/// mode, which is where the `--json` branch already lives.
///
/// # Errors
///
/// [`Error::Registry`] if the embedded registry fails validation.
pub fn list_models_json() -> Result<String> {
    Ok(crate::model::registry::registry()?.to_json())
}

/// Implements `--download-model`: fetch a registry model and verify its digest.
///
/// Needs no engine and no consent. It is idempotent: a file already present and
/// correct is not touched, and a file that hashes to something else is deleted
/// before a retry, both of which live in [`crate::model::download::ensure`].
///
/// The transport is injected rather than constructed here so the whole path is
/// exercised by a fake in tests, and so a build without the `download` feature
/// reaches [`Error::DownloadUnavailable`] instead of a silent no-op.
///
/// # Errors
///
/// [`Error::DownloadRefused`] when `no_model` is set,
/// [`Error::DownloadUnavailable`] when no transport was supplied,
/// [`Error::UnknownModel`] when the name is not in the registry,
/// [`Error::Registry`] when the embedded registry fails validation, and
/// [`Error::Download`] when the transfer fails or the bytes do not match.
pub fn download_model<'a>(
    name: Option<&str>,
    config: &'a crate::config::EffectiveConfig,
    models_dir: &std::path::Path,
    transport: Option<&'a dyn crate::model::download::Transport>,
    on_progress: Option<&'a mut dyn FnMut(u64, u64)>,
) -> Result<Output> {
    use crate::model::download::{self, Outcome};

    if config.no_model {
        return Err(Error::DownloadRefused);
    }
    let transport = transport.ok_or(Error::DownloadUnavailable)?;

    let registry = crate::model::registry::registry()?;
    let name = match name {
        Some(name) => name.to_owned(),
        None => match &config.model_name {
            Some(name) => name.clone(),
            None => registry
                .default_entry()
                .map(|entry| entry.name.clone())
                .ok_or_else(|| Error::Registry {
                    message: "the registry has no default model".to_owned(),
                })?,
        },
    };
    let entry = registry
        .get(&name)
        .ok_or_else(|| Error::UnknownModel { name: name.clone() })?;

    let resolved = download::resolve(&download::ResolveInput {
        name: Some(&name),
        path: config.model_path.as_deref(),
        models_dir,
        search_dirs: &[],
    })?;

    let mut request = download::request(entry, resolved.clone(), transport);
    request.mirror = config.model_mirror.as_deref();
    request.on_progress = on_progress;
    let outcome = download::ensure(&mut request).map_err(|e| Error::Download {
        message: e.to_string(),
    })?;

    let where_to = resolved.path.display();
    let explanation = match outcome {
        Outcome::AlreadyPresent => format!("already present and verified: {where_to}"),
        Outcome::Downloaded => format!(
            "downloaded {name} ({}), verified, to {where_to}",
            entry.size_human()
        ),
        Outcome::Resumed => format!(
            "resumed and finished {name} ({}), verified, to {where_to}",
            entry.size_human()
        ),
    };

    Ok(Output {
        command: String::new(),
        level: Risk::Safe,
        mode: Mode::DownloadModel { name: Some(name) }.to_string(),
        reasons: Vec::new(),
        segments: Vec::new(),
        explanation: Some(explanation),
        executed: false,
        exit_code: None,
    })
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
    fix_with_blocklist(
        engine,
        parsed,
        consenter,
        always_confirm,
        history,
        &FixSettings {
            blocklist: &[],
            params: GenParams::default(),
            runner: None,
        },
    )
}

/// What [`fix_with_blocklist`] needs beyond the engine, the prompt, and the history.
pub struct FixSettings<'a> {
    /// The user's `safety.blocklist`.
    pub blocklist: &'a [String],
    /// How to sample.
    pub params: GenParams,
    /// Runs the corrected command once it is approved.
    pub runner: Option<Runner<'a>>,
}

/// [`fix`] with the user's `safety.blocklist` applied by the gate.
///
/// # Errors
///
/// The same errors [`fix`] returns.
pub fn fix_with_blocklist(
    engine: Option<Arc<dyn InferenceEngine>>,
    parsed: &Parsed,
    consenter: &mut dyn Consenter,
    always_confirm: Risk,
    history: &History,
    settings: &FixSettings<'_>,
) -> Result<Output> {
    let blocklist = settings.blocklist;
    let runner = settings.runner;
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
    let generated = crate::inference::generate_command(&engine, &prompt, &settings.params)
        .map_err(|e| Error::Inference {
            message: e.to_string(),
        })?;

    let original = entries[failed_at].cmd.clone();
    let mut out = gate(
        generated,
        parsed,
        consenter,
        always_confirm,
        blocklist,
        runner,
    )?;
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
        exit_code: None,
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
///
/// `SAFE`, so every command that can run is asked about first; `--yes` is the only
/// way past the prompt ([ADR 0024](docs/adr/0024-confirm-every-command-by-default.md)).
pub const DEFAULT_CONFIRM_AT: Risk = Risk::Safe;

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
    blocklist: &[String],
    runner: Option<Runner<'_>>,
) -> Result<Output> {
    let verdict = safety::classify_with(&command, blocklist);

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
    //
    // `--complete` prints text for the shell to place on the line and runs
    // nothing, so below `MEDIUM` it does not ask, whatever the threshold.
    let threshold = if matches!(parsed.mode, Mode::Complete { .. }) {
        always_confirm.max(Risk::Medium)
    } else {
        always_confirm
    };
    if needs_confirmation(verdict.level, threshold, parsed.yes) {
        match consenter.ask(&command, &verdict) {
            Decision::Denied => {
                return Err(Error::ConsentDenied {
                    level: verdict.level,
                });
            }
            Decision::Granted(edited) => {
                if edited != command {
                    let rechecked = safety::classify_with(&edited, blocklist);
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

    // Execution. Reached only with a `Runner`, and only after everything above:
    // CRITICAL refused, `--dry-run` returned, consent obtained or not needed. The
    // level is checked once more against the command that will actually run, so a
    // later edit that reorders the code above cannot reach the executor with a
    // refused command.
    let Some(runner) = runner else {
        return Ok(output(&command, &verdict, &parsed.mode.to_string(), false));
    };
    if !verdict.level.is_runnable() {
        return Err(Error::RiskBlocked {
            level: verdict.level,
            reasons: verdict.reason_messages(),
        });
    }

    let code = runner.executor.run(&command)?;
    if let Some(history) = runner.history {
        // A history that cannot be written must not turn a command that already
        // ran into an error; the command's own status is what the caller needs.
        let _ = history.append(&executed_entry(&command, code));
    }
    let mut out = output(&command, &verdict, &parsed.mode.to_string(), true);
    out.exit_code = Some(code);
    Ok(out)
}

/// The history record for a command that ran.
///
/// The output is empty: the command's stdout and stderr go straight to the
/// terminal and are not captured.
fn executed_entry(command: &str, code: i32) -> HistoryEntry {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    HistoryEntry {
        ts,
        cmd: command.to_owned(),
        exit: code,
        cwd,
        out: String::new(),
    }
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
        exit_code: None,
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
        exit_code: None,
    }
}

/// Implements `--explain`: classify, describe, never execute, never suggest.
///
/// This mode needs no model and no consent, because it runs nothing. That makes
/// it the one mode that is fully usable before the inference pipeline lands.
fn explain(command: &str, _parsed: &Parsed, blocklist: &[String]) -> Output {
    let verdict = safety::classify_with(command, blocklist);
    let mut out = output(command, &verdict, "explain", false);
    out.explanation = Some(verdict.explanation());
    // An explain of a CRITICAL command is not an error. The user asked what it
    // does; refusing to answer would make the tool useless for the command most
    // worth understanding. Only `run` decides what is an error.
    out
}

#[cfg(test)]
mod download_glue_tests {
    use super::*;
    use crate::model::download::Transport;
    use std::io::{self, Read};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Serves fixed bytes, optionally sliced at `offset`.
    struct Bytes(Vec<u8>);

    impl Transport for Bytes {
        fn get(&self, _url: &str, offset: u64) -> io::Result<Box<dyn Read + Send>> {
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(self.0.len());
            Ok(Box::new(io::Cursor::new(self.0[start..].to_vec())))
        }

        fn supports_range(&self) -> bool {
            true
        }
    }

    /// A hand-rolled temp directory, as in `download.rs`: no dependency for one
    /// helper, and unique per process and per call without randomness.
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("gcode-rt-{}-{tag}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn default_name() -> String {
        crate::model::registry::registry()
            .expect("registry")
            .default_entry()
            .expect("a default entry")
            .name
            .clone()
    }

    #[test]
    fn an_unknown_name_is_refused_before_any_transfer() {
        let dir = Dir::new("unknown");
        let transport = Bytes(Vec::new());
        let error = download_model(
            Some("no-such-model"),
            &crate::config::defaults(),
            &dir.0,
            Some(&transport),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(&error, Error::UnknownModel { name } if name == "no-such-model"),
            "{error}"
        );
    }

    #[test]
    fn no_transport_reports_a_build_without_a_downloader() {
        let dir = Dir::new("none");
        let error = download_model(
            Some(&default_name()),
            &crate::config::defaults(),
            &dir.0,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, Error::DownloadUnavailable), "{error}");
    }

    #[test]
    fn no_model_in_the_config_refuses_the_download() {
        let dir = Dir::new("refused");
        let mut config = crate::config::defaults();
        config.no_model = true;
        let transport = Bytes(Vec::new());
        let error = download_model(
            Some(&default_name()),
            &config,
            &dir.0,
            Some(&transport),
            None,
        )
        .unwrap_err();
        assert!(matches!(error, Error::DownloadRefused), "{error}");
    }

    #[test]
    fn the_default_model_is_chosen_and_progress_is_reported() {
        // The body is shorter than the registry declares, so `ensure` reaches the
        // transfer, reports progress, and then refuses the truncated file. A
        // `Download` error rather than `UnknownModel` proves the name resolved to
        // the default entry and the transfer ran.
        let dir = Dir::new("default");
        let transport = Bytes(b"not the model".to_vec());
        let mut seen: Vec<(u64, u64)> = Vec::new();
        let mut progress = |received: u64, total: u64| seen.push((received, total));
        let error = download_model(
            None,
            &crate::config::defaults(),
            &dir.0,
            Some(&transport),
            Some(&mut progress),
        )
        .unwrap_err();
        assert!(matches!(error, Error::Download { .. }), "{error}");
        assert!(
            !seen.is_empty(),
            "the progress hook must fire while bytes land"
        );

        // A refused transfer keeps the partial, because it is still a good prefix
        // and the next attempt resumes from it rather than paying twice.
        let entries: Vec<_> = std::fs::read_dir(&dir.0)
            .expect("readable")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(entries.len(), 1, "expected one partial file: {entries:?}");
        assert!(
            entries[0].to_string_lossy().ends_with(".part"),
            "{:?} is not a .part",
            entries[0]
        );
    }
}
