//! Command-line parsing.
//!
//! This module answers one question: what did the user ask for? It does not
//! read the config file, resolve a model, or touch the terminal. Everything
//! downstream takes a [`Cli`] and decides what to do with it.
//!
//! # Why the mode is validated here
//!
//! Exactly one mode must be selected. `gcode -c "list files" --fix` is
//! ambiguous: does the user want a fresh command, or a repair of the last
//! failure? Picking one silently would be the worst outcome, because the user
//! would get a plausible answer to a question they did not ask. So this module
//! refuses, and the error names every valid mode.
//!
//! # Why ranges are validated here
//!
//! `--context 0` and `--temperature 99` are not nonsense to clap — they are
//! well-formed values for a program that will not work. Catching them at the
//! edge means every module behind this one can treat its inputs as in-range,
//! and a bad value produces one specific error instead of a strange result
//! three layers down.

use std::fmt;
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

// Bools are inherent to a flag surface: `--yes` is either given or it is not,
// and a two-variant enum per flag would be ceremony around a command line.
// The lint stays enabled crate-wide, where it is useful for domain types.
#[allow(clippy::struct_excessive_bools)]
/// Natural-language shell command generator. Local-first and offline.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "gcode",
    version,
    about = "Turns a sentence into a shell command, classifies the risk, and asks before it runs.",
    long_about = None,
    // NOT `arg_required_else_help`. Bare `gcode` is the interactive mode, per
    // USAGE.md, so it must reach `resolve_mode` and become `Mode::Interactive`
    // rather than clap printing help and exiting. That flag would also have
    // made bare `gcode` exit 0 while doing nothing, which is the same "looks
    // like success but is not" failure the Phase 0 skeleton was written to
    // avoid.
    //
    // Unknown flags are still rejected outright by clap, so a typo is never
    // silently dropped.
)]
pub struct Cli {
    /// Natural language request. Required unless another mode is chosen.
    #[arg(short = 'c', long = "command", value_name = "TEXT")]
    pub command: Option<String>,

    /// Repair the last failed command from history.
    #[arg(long)]
    pub fix: bool,

    /// Continue a partial command.
    #[arg(long, value_name = "PARTIAL")]
    pub complete: Option<String>,

    /// Explain a command. Generates nothing and runs nothing.
    #[arg(long, value_name = "CMD")]
    pub explain: Option<String>,

    /// Print the model registry and exit. Needs no model and no network.
    #[arg(long)]
    pub list_models: bool,

    /// Download a model from the registry, verify it, and exit.
    ///
    /// `--download-model` with no value fetches the configured model, or the
    /// registry default. Naming one explicitly is how a user opts into a
    /// different model without editing the config first.
    #[arg(long, value_name = "NAME", num_args = 0..=1)]
    pub download_model: Option<Option<String>>,

    /// Install the shell hook that records command history.
    #[arg(long)]
    pub init: bool,

    /// Report whether the shell hook is installed, and which version.
    #[arg(long)]
    pub check: bool,

    /// Remove the shell hook block and its files.
    #[arg(long)]
    pub remove: bool,

    /// Shell to install for. Defaults to $SHELL.
    #[arg(long, value_name = "SHELL", value_enum)]
    pub shell: Option<ShellFlag>,

    /// Skip the confirmation prompt. Still classifies, still logs.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Generate and print, never execute. Alias of --dry-run.
    #[arg(short = 'n', long)]
    pub no: bool,

    /// Print the command and risk, then exit.
    #[arg(long)]
    pub dry_run: bool,

    /// Number of recent history entries to read as context.
    #[arg(long, value_name = "N")]
    pub context: Option<usize>,

    /// Ignore history entirely.
    #[arg(long)]
    pub no_history: bool,

    /// Leave git status out of the context.
    #[arg(long)]
    pub no_git: bool,

    /// Leave the environment out of the context.
    #[arg(long)]
    pub no_env: bool,

    /// Build context as if running in this directory.
    #[arg(long = "in", value_name = "DIR")]
    pub in_dir: Option<PathBuf>,

    /// Model name or path to a GGUF file.
    #[arg(long, value_name = "PATH_OR_NAME")]
    pub model: Option<String>,

    /// Inference thread count. 0 uses all cores.
    #[arg(long, value_name = "N")]
    pub n_threads: Option<usize>,

    /// Layers to offload to the GPU.
    #[arg(long, value_name = "N")]
    pub n_gpu_layers: Option<i32>,

    /// Prompt context window, in tokens.
    #[arg(long, value_name = "N")]
    pub context_size: Option<u32>,

    /// Sampling temperature. Keep it low for commands.
    // `allow_hyphen_values` because the valid range starts at 0.0 and a user
    // who types `--temperature -0.1` should be told the value is out of range,
    // not that clap found an unexpected argument called `-0`. Without this the
    // range check below is unreachable for the one input that most needs it.
    #[arg(long, value_name = "F", allow_hyphen_values = true)]
    pub temperature: Option<f32>,

    /// Machine-readable output. No prompts, no colour.
    #[arg(long)]
    pub json: bool,

    /// Disable ANSI colour.
    #[arg(long)]
    pub no_color: bool,
}

/// What the user asked gcode to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Generate a command from a natural language request.
    Generate {
        /// The request as the user typed it.
        request: String,
    },
    /// Repair the last failed command.
    Fix,
    /// Continue a half-typed command.
    Complete {
        /// The partial command so far.
        partial: String,
    },
    /// Explain an existing command.
    Explain {
        /// The command to explain.
        command: String,
    },
    /// Print the model registry.
    ListModels,
    /// Download and verify a model from the registry.
    DownloadModel {
        /// The registry name, or `None` for the configured or default model.
        name: Option<String>,
    },
    /// Install the shell hook.
    Init,
    /// Report the shell hook's installed state.
    Check,
    /// Remove the shell hook.
    Remove,
    /// Interactive prompt loop.
    Interactive,
}

impl Mode {
    /// Whether this mode is one of the three shell-integration modes.
    #[must_use]
    pub fn is_shell_hook(&self) -> bool {
        matches!(self, Self::Init | Self::Check | Self::Remove)
    }
}

impl fmt::Display for Mode {
    /// The mode name, as it appears in `--json` output and in error messages.
    ///
    /// Kept separate from `Debug` on purpose: `Debug` prints `Generate { request:
    /// "..." }`, which is for a programmer reading a panic, not for a user
    /// reading `--json` or an error. This is the name a person would type.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Generate { .. } => "generate",
            Self::Fix => "fix",
            Self::Complete { .. } => "complete",
            Self::Explain { .. } => "explain",
            Self::ListModels => "list-models",
            Self::DownloadModel { .. } => "download-model",
            Self::Init => "init",
            Self::Check => "check",
            Self::Remove => "remove",
            Self::Interactive => "interactive",
        };
        f.write_str(name)
    }
}

/// Inclusive upper bound on `--context`.
///
/// A thousand entries is already far more context than most models can hold.
/// Past that the request is either a mistake or an attempt to exhaust memory,
/// and either way it should be refused rather than truncated silently.
const MAX_CONTEXT_ENTRIES: usize = 1000;

/// Inclusive bounds on `--temperature`.
const MIN_TEMPERATURE: f32 = 0.0;
const MAX_TEMPERATURE: f32 = 2.0;

/// Inclusive bounds on `--context-size`.
const MIN_CONTEXT_SIZE: u32 = 512;
const MAX_CONTEXT_SIZE: u32 = 32768;

/// The resolved, validated request.
///
/// Constructing one of these is the only way to obtain a [`Mode`], so nothing
/// downstream has to re-check that a mode was chosen or that a value is in
/// range. The validation is not a convenience; it is the reason this type
/// exists.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct Parsed {
    /// What the user asked for.
    pub mode: Mode,
    /// Whether to skip the confirmation prompt.
    pub yes: bool,
    /// Whether to print without executing.
    pub dry_run: bool,
    /// Whether to ignore history.
    pub no_history: bool,
    /// Whether to leave git status out of the context.
    pub no_git: bool,
    /// Whether to leave the environment out of the context.
    pub no_env: bool,
    /// How many history entries to read as context, if the flag was given.
    ///
    /// `None` means the flag was absent, which is not the same as `Some(15)`:
    /// the config file and the environment may have set a different value. The
    /// default lives in `config::defaults`, in one place.
    pub context_entries: Option<usize>,
    /// Model name or path, if the user named one.
    pub model: Option<String>,
    /// Thread count, if the flag was given. 0 means all cores.
    pub n_threads: Option<usize>,
    /// Layers to offload to the GPU, if the flag was given.
    pub n_gpu_layers: Option<i32>,
    /// Prompt context window in tokens, if the flag was given.
    pub context_size: Option<u32>,
    /// Sampling temperature, if the flag was given.
    pub temperature: Option<f32>,
    /// Whether to emit JSON.
    pub json: bool,
    /// Whether to disable colour.
    pub no_color: bool,
    /// Directory to build context as if running in.
    pub in_dir: Option<PathBuf>,
    /// The shell the hook modes act on. `None` means read `$SHELL`.
    ///
    /// Kept as the resolved [`Kind`](crate::shell::Kind) rather than the flag
    /// type so nothing downstream has to know which spelling came from where.
    pub shell: Option<crate::shell::Kind>,
}

/// The valid modes, as they appear in the conflict error. Kept in one place so
/// that adding a mode and updating this string cannot drift apart.
const MODE_CHOICES: &str = "--command/-c, --fix, --complete, --explain, --list-models, \
     --download-model, --init, --check, --remove, or no arguments for interactive";

impl Cli {
    /// Validates the parsed flags and resolves them into a [`Parsed`].
    ///
    /// # Errors
    ///
    /// Returns an error when no mode is selected, more than one mode is
    /// selected, or a numeric value falls outside its documented range.
    pub fn parse(self) -> Result<Parsed, String> {
        let mode = self.resolve_mode()?;
        if let Some(context_entries) = self.context {
            if context_entries > MAX_CONTEXT_ENTRIES {
                return Err(format!(
                    "--context is {context_entries}, but the maximum is {MAX_CONTEXT_ENTRIES}; \
                     a smaller number is also faster, because every entry becomes tokens"
                ));
            }
        }

        if let Some(temperature) = self.temperature {
            if !(MIN_TEMPERATURE..=MAX_TEMPERATURE).contains(&temperature) {
                return Err(format!(
                    "--temperature is {temperature}, but it must be between {MIN_TEMPERATURE} and \
                     {MAX_TEMPERATURE}; values above 1 produce a command you did not ask for"
                ));
            }
        }

        if let Some(context_size) = self.context_size {
            if !(MIN_CONTEXT_SIZE..=MAX_CONTEXT_SIZE).contains(&context_size) {
                return Err(format!(
                    "--context-size is {context_size}, but it must be between {MIN_CONTEXT_SIZE} \
                     and {MAX_CONTEXT_SIZE} tokens; a window too small for the prompt is not an \
                     error the model can report"
                ));
            }
        }

        // `--shell` only means something to the hook modes. Accepting
        // `gcode --shell zsh` on its own would imply an install that never
        // happens, which is the "looks like it did something" failure this
        // project treats as a bug.
        if self.shell.is_some() && !mode.is_shell_hook() {
            return Err(format!(
                "--shell only applies to --init, --check, and --remove; \
                 choose one of {MODE_CHOICES}"
            ));
        }

        Ok(Parsed {
            mode,
            // `--no` is documented as an alias of `--dry-run`. Keeping them
            // distinct here would let a future caller treat them differently
            // and quietly break the promise in USAGE.md.
            yes: self.yes,
            dry_run: self.dry_run || self.no,
            no_history: self.no_history,
            no_git: self.no_git,
            no_env: self.no_env,
            context_entries: self.context,
            model: self.model,
            n_threads: self.n_threads,
            n_gpu_layers: self.n_gpu_layers,
            context_size: self.context_size,
            temperature: self.temperature,
            json: self.json,
            no_color: self.no_color,
            in_dir: self.in_dir,
            shell: self.shell.map(Into::into),
        })
    }

    /// The mode flags the user actually supplied, in the order given on the
    /// command line.
    ///
    /// Reported separately from [`Mode`] because a flag and a mode name are
    /// different things: the user typed `-c`, and `Mode::Generate` is what that
    /// means. An error that named only the mode would leave them searching the
    /// help text for "generate", which is not a flag.
    fn selected_mode_flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.command.is_some() {
            flags.push("--command/-c");
        }
        if self.fix {
            flags.push("--fix");
        }
        if self.complete.is_some() {
            flags.push("--complete");
        }
        if self.explain.is_some() {
            flags.push("--explain");
        }
        if self.list_models {
            flags.push("--list-models");
        }
        if self.download_model.is_some() {
            flags.push("--download-model");
        }
        if self.init {
            flags.push("--init");
        }
        if self.check {
            flags.push("--check");
        }
        if self.remove {
            flags.push("--remove");
        }
        flags
    }

    /// Resolves the mode flags into exactly one [`Mode`].
    fn resolve_mode(&self) -> Result<Mode, String> {
        let mut selected: Vec<Mode> = Vec::new();

        if let Some(request) = &self.command {
            selected.push(Mode::Generate {
                request: request.clone(),
            });
        }
        if self.fix {
            selected.push(Mode::Fix);
        }
        if let Some(partial) = &self.complete {
            selected.push(Mode::Complete {
                partial: partial.clone(),
            });
        }
        if let Some(command) = &self.explain {
            selected.push(Mode::Explain {
                command: command.clone(),
            });
        }
        if self.list_models {
            selected.push(Mode::ListModels);
        }
        if let Some(name) = &self.download_model {
            selected.push(Mode::DownloadModel { name: name.clone() });
        }
        if self.init {
            selected.push(Mode::Init);
        }
        if self.check {
            selected.push(Mode::Check);
        }
        if self.remove {
            selected.push(Mode::Remove);
        }

        match selected.len() {
            0 => Ok(Mode::Interactive),
            1 => Ok(selected.remove(0)),
            _ => {
                // `Mode::Display` is the short name. The flag is what the user
                // actually typed, so both are shown: a user who wrote `-c` and
                // `--fix` needs to see "generate" resolved from `-c`, and
                // seeing only "generate" would leave them guessing.
                let flags: Vec<&str> = self.selected_mode_flags();
                Err(format!(
                    "more than one mode selected: {}. Choose exactly one of {MODE_CHOICES}.",
                    flags.join(", ")
                ))
            }
        }
    }
}

/// What [`parse_from`] produced, distinguishing "already answered" from
/// "you typed it wrong".
///
/// This exists because clap routes `--help` and `--version` through its error
/// channel. A caller that treats every `Err` as a failure makes
/// `gcode --version` exit non-zero, which breaks the one invocation every
/// script and every package manager performs first. Modelling the distinction
/// here means `main` cannot get it wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    /// `--help` or `--version`. Print the text and exit 0.
    Handled(String),
    /// A genuine usage error. Report it and exit non-zero.
    Usage(String),
}

/// Parse and validate in one step. This is what `main` calls.
///
/// # Errors
///
/// Returns [`ParseOutcome::Usage`] for a bad flag combination or an
/// out-of-range value, and [`ParseOutcome::Handled`] for `--help` and
/// `--version`, which are successes that clap delivers as errors.
pub fn parse_from<I, T>(args: I) -> Result<Parsed, ParseOutcome>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => cli.parse().map_err(ParseOutcome::Usage),
        Err(error) => Err(match error.kind() {
            clap::error::ErrorKind::DisplayHelp
            | clap::error::ErrorKind::DisplayVersion
            | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                ParseOutcome::Handled(error.to_string())
            }
            _ => ParseOutcome::Usage(error.to_string()),
        }),
    }
}

/// The shells `--shell` accepts.
///
/// A local enum rather than `crate::shell::Kind` so this module owns its own
/// flag surface: the conversion lives in `resolve_shell` and the error it
/// produces names the value the user typed, not an internal type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ShellFlag {
    /// bash, hooked via `PROMPT_COMMAND`.
    Bash,
    /// zsh, hooked via `precmd_functions`.
    Zsh,
}

impl From<ShellFlag> for crate::shell::Kind {
    fn from(flag: ShellFlag) -> Self {
        match flag {
            ShellFlag::Bash => Self::Bash,
            ShellFlag::Zsh => Self::Zsh,
        }
    }
}

/// Log level, for the `--log-level` flag added in Phase 1.6.
///
/// Declared here so the flag surface has one home, but not wired to a `--log-level`
/// argument yet: adding a flag that does nothing is worse than not having it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    /// Only failures.
    Error,
    /// Failures and things that will become failures.
    Warn,
    /// Normal progress reporting.
    Info,
    /// Detail useful when diagnosing a problem.
    Debug,
    /// Everything, including per-token output.
    Trace,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        match parse_from(args) {
            Ok(parsed) => Ok(parsed),
            Err(ParseOutcome::Usage(message)) => Err(message),
            Err(ParseOutcome::Handled(_)) => {
                panic!("expected a usage error, got help or version output")
            }
        }
    }

    /// True when the arguments produced help or version output rather than a
    /// usage error. `--help` and `--version` are successes.
    fn handled(args: &[&str]) -> Option<String> {
        match parse_from(args) {
            Err(ParseOutcome::Handled(text)) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn command_becomes_generate_mode() {
        let parsed = parse(&["gcode", "-c", "list files"]).unwrap();
        assert_eq!(
            parsed.mode,
            Mode::Generate {
                request: "list files".to_owned()
            }
        );
    }

    #[test]
    fn the_long_form_of_command_works_too() {
        let parsed = parse(&["gcode", "--command", "list files"]).unwrap();
        assert_eq!(
            parsed.mode,
            Mode::Generate {
                request: "list files".to_owned()
            }
        );
    }

    #[test]
    fn no_mode_flags_means_interactive() {
        // USAGE.md documents bare `gcode` as the prompt loop.
        assert_eq!(parse(&["gcode"]).unwrap().mode, Mode::Interactive);
    }

    #[test]
    fn fix_is_its_own_mode() {
        assert_eq!(parse(&["gcode", "--fix"]).unwrap().mode, Mode::Fix);
    }

    #[test]
    fn complete_carries_its_partial() {
        assert_eq!(
            parse(&["gcode", "--complete", "find /var/log"])
                .unwrap()
                .mode,
            Mode::Complete {
                partial: "find /var/log".to_owned()
            }
        );
    }

    #[test]
    fn explain_carries_its_command() {
        assert_eq!(
            parse(&["gcode", "--explain", "rm -rf /"]).unwrap().mode,
            Mode::Explain {
                command: "rm -rf /".to_owned()
            }
        );
    }

    #[test]
    fn list_models_is_its_own_mode() {
        assert_eq!(
            parse(&["gcode", "--list-models"]).unwrap().mode,
            Mode::ListModels
        );
    }

    #[test]
    fn list_models_conflicts_with_a_command() {
        let error = parse(&["gcode", "--list-models", "-c", "x"]).unwrap_err();
        assert!(error.contains("--list-models"), "{error}");
    }

    #[test]
    fn download_model_takes_an_optional_name() {
        assert_eq!(
            parse(&["gcode", "--download-model"]).unwrap().mode,
            Mode::DownloadModel { name: None }
        );
        assert_eq!(
            parse(&["gcode", "--download-model", "qwen3-0.6b"])
                .unwrap()
                .mode,
            Mode::DownloadModel {
                name: Some("qwen3-0.6b".to_owned())
            }
        );
    }

    #[test]
    fn download_model_does_not_swallow_a_following_flag() {
        // The value is optional, so `--download-model --json` must mean
        // "download the default, as JSON", not "download a model named --json".
        let parsed = parse(&["gcode", "--download-model", "--json"]).unwrap();
        assert_eq!(parsed.mode, Mode::DownloadModel { name: None });
        assert!(parsed.json);
    }

    #[test]
    fn download_model_conflicts_with_a_command() {
        let error = parse(&["gcode", "--download-model", "-c", "x"]).unwrap_err();
        assert!(error.contains("--download-model"), "{error}");
    }

    #[test]
    fn every_pair_of_modes_is_refused() {
        // Guards against a future mode being added to `resolve_mode` but not
        // to this list, which would let the pair through unnoticed.
        let flags: Vec<Vec<&str>> = vec![
            vec!["-c", "x"],
            vec!["--fix"],
            vec!["--complete", "x"],
            vec!["--explain", "x"],
            vec!["--list-models"],
            vec!["--download-model"],
            vec!["--init"],
            vec!["--check"],
            vec!["--remove"],
        ];
        for a in &flags {
            for b in &flags {
                if a == b {
                    continue;
                }
                let mut args = vec!["gcode"];
                args.extend(a.iter().copied());
                args.extend(b.iter().copied());
                assert!(
                    parse(&args).is_err(),
                    "two modes accepted: {a:?} with {b:?}"
                );
            }
        }
    }

    #[test]
    fn the_conflict_error_lists_the_valid_modes() {
        let error = parse(&["gcode", "--fix", "--explain", "ls"]).unwrap_err();
        for expected in ["--command", "--fix", "--complete", "--explain"] {
            assert!(error.contains(expected), "missing {expected}: {error}");
        }
    }

    #[test]
    fn the_three_hook_modes_are_parsed() {
        assert_eq!(parse(&["gcode", "--init"]).unwrap().mode, Mode::Init);
        assert_eq!(parse(&["gcode", "--check"]).unwrap().mode, Mode::Check);
        assert_eq!(parse(&["gcode", "--remove"]).unwrap().mode, Mode::Remove);
    }

    #[test]
    fn hook_modes_are_modes_not_side_effects() {
        // They must not fall through to Interactive, or `gcode --init` would open
        // a prompt loop in the middle of installing a hook.
        for mode in [Mode::Init, Mode::Check, Mode::Remove] {
            assert!(
                mode.is_shell_hook(),
                "{mode} is not recognised as a hook mode"
            );
        }
        assert!(!Mode::Interactive.is_shell_hook());
        assert!(!Mode::Fix.is_shell_hook());
    }

    #[test]
    fn a_hook_mode_reaches_the_conflict_error() {
        // `--init --remove` is the one combination a user might reasonably type
        // after a botched install. It is refused, and both flags are named.
        let error = parse(&["gcode", "--init", "--remove"]).unwrap_err();
        assert!(error.contains("--init"), "does not name the first: {error}");
        assert!(
            error.contains("--remove"),
            "does not name the second: {error}"
        );
    }

    #[test]
    fn the_shell_flag_is_accepted_for_the_hook_modes() {
        use crate::shell::Kind;
        let cases: [(&[&str], Kind); 4] = [
            (&["gcode", "--init", "--shell", "bash"], Kind::Bash),
            (&["gcode", "--init", "--shell", "zsh"], Kind::Zsh),
            (&["gcode", "--check", "--shell", "zsh"], Kind::Zsh),
            (&["gcode", "--remove", "--shell", "bash"], Kind::Bash),
        ];
        for (args, expected) in cases {
            assert_eq!(parse(args).unwrap().shell, Some(expected), "{args:?}");
        }
    }

    #[test]
    fn the_shell_flag_alone_is_refused() {
        // Otherwise `gcode --shell zsh` implies an install that never happens.
        for args in [
            vec!["gcode", "--shell", "zsh"],
            vec!["gcode", "--shell", "bash", "-c", "list files"],
        ] {
            let error = parse(&args).unwrap_err();
            assert!(error.contains("--shell"), "unclear: {error}");
        }
    }

    #[test]
    fn an_unknown_shell_is_rejected_by_clap() {
        // Not a fallback to a default. Writing a bash hook into a fish config
        // would leave a shell that silently records nothing.
        assert!(parse(&["gcode", "--init", "--shell", "fish"]).is_err());
    }

    #[test]
    fn no_shell_flag_means_detect_from_the_environment() {
        assert_eq!(parse(&["gcode", "--init"]).unwrap().shell, None);
    }

    #[test]
    fn mode_display_names_the_list_models_mode() {
        assert_eq!(Mode::ListModels.to_string(), "list-models");
    }

    #[test]
    fn mode_display_names_the_download_model_mode() {
        assert_eq!(
            Mode::DownloadModel { name: None }.to_string(),
            "download-model"
        );
    }

    #[test]
    fn mode_display_names_the_hook_modes() {
        assert_eq!(Mode::Init.to_string(), "init");
        assert_eq!(Mode::Check.to_string(), "check");
        assert_eq!(Mode::Remove.to_string(), "remove");
    }

    #[test]
    fn an_unknown_flag_is_rejected_by_clap() {
        // Not turned into a Cli field, so clap must refuse it outright. A
        // silently ignored typo is the failure mode this guards.
        assert!(parse(&["gcode", "--dryrun"]).is_err());
        assert!(parse(&["gcode", "-c"]).is_err(), "missing value accepted");
    }

    #[test]
    fn an_absent_numeric_flag_stays_absent() {
        // Absent, not 15. The default lives in `config::defaults` so that the
        // config file and the environment are not silently overruled by a
        // default the user cannot see.
        assert_eq!(parse(&["gcode", "-c", "x"]).unwrap().context_entries, None);
    }

    #[test]
    fn two_modes_are_refused_and_both_are_named() {
        // The error must name the flags, not the internal mode names. A user
        // who typed `-c` and `--fix` has never heard of "generate".
        let error = parse(&["gcode", "-c", "list files", "--fix"]).unwrap_err();
        assert!(
            error.contains("--command/-c"),
            "does not name the first: {error}"
        );
        assert!(error.contains("--fix"), "does not name the second: {error}");
    }

    #[test]
    fn help_documents_the_hook_flags() {
        // A flag missing from --help is a flag nobody can find.
        let output = handled(&["gcode", "--help"]).expect("--help must be handled");
        for flag in ["--init", "--check", "--remove", "--shell"] {
            assert!(output.contains(flag), "help omits {flag}");
        }
    }

    #[test]
    fn the_conflict_error_does_not_repeat_itself() {
        // Guards the exact wording that a draft produced: "Choose exactly one
        // of one of: ...". Silly, and only visible by running it.
        let error = parse(&["gcode", "--fix", "--explain", "ls"]).unwrap_err();
        assert!(
            !error.contains("one of one of"),
            "duplicated phrase in: {error}"
        );
    }

    #[test]
    fn the_conflict_error_lists_every_flag_they_typed() {
        let error = parse(&["gcode", "--complete", "x", "--explain", "y", "--fix"]).unwrap_err();
        for flag in ["--complete", "--explain", "--fix"] {
            assert!(error.contains(flag), "missing {flag} in: {error}");
        }
    }

    #[test]
    fn context_at_the_maximum_is_accepted() {
        let parsed = parse(&["gcode", "-c", "x", "--context", "1000"]).unwrap();
        assert_eq!(parsed.context_entries, Some(1000));
    }

    #[test]
    fn temperature_above_the_range_is_refused() {
        for bad in ["2.1", "100"] {
            let error = parse(&["gcode", "-c", "x", "--temperature", bad]).unwrap_err();
            assert!(error.contains("temperature"), "unclear: {error}");
        }
    }

    #[test]
    fn a_negative_temperature_reaches_the_range_check() {
        // `allow_hyphen_values` on the flag is what makes this work. Without
        // it clap reads the leading `-` as another flag and reports
        // "unexpected argument '-0'", which tells the user nothing about the
        // actual problem.
        let forms: &[&[&str]] = &[&["--temperature", "-0.1"], &["--temperature=-0.1"]];
        for form in forms {
            let mut args = vec!["gcode", "-c", "x"];
            args.extend_from_slice(form);
            let error = parse(&args).unwrap_err();
            assert!(
                error.contains("must be between"),
                "range check not reached for {form:?}: {error}"
            );
        }
    }

    #[test]
    fn temperature_at_the_bounds_is_accepted() {
        for good in ["0.0", "0.2", "2.0"] {
            assert!(
                parse(&["gcode", "-c", "x", "--temperature", good]).is_ok(),
                "rejected a valid temperature: {good}"
            );
        }
    }

    #[test]
    fn context_size_outside_the_range_is_refused() {
        for bad in ["256", "65536"] {
            let error = parse(&["gcode", "-c", "x", "--context-size", bad]).unwrap_err();
            assert!(error.contains("context-size"), "unclear: {error}");
        }
    }

    #[test]
    fn context_size_at_the_bounds_is_accepted() {
        for good in ["512", "4096", "32768"] {
            assert!(
                parse(&["gcode", "-c", "x", "--context-size", good]).is_ok(),
                "rejected a valid context size: {good}"
            );
        }
    }

    #[test]
    fn no_is_an_alias_for_dry_run() {
        // USAGE.md promises they are the same thing.
        assert!(parse(&["gcode", "-c", "x", "-n"]).unwrap().dry_run);
        assert!(parse(&["gcode", "-c", "x", "--no"]).unwrap().dry_run);
        assert!(parse(&["gcode", "-c", "x", "--dry-run"]).unwrap().dry_run);
    }

    #[test]
    fn neither_no_nor_dry_run_means_do_not_dry_run() {
        assert!(!parse(&["gcode", "-c", "x"]).unwrap().dry_run);
    }

    #[test]
    fn yes_is_carried_through_untouched() {
        // --yes suppresses the prompt only. It must never be folded into any
        // other field here, or a later change could widen what it skips.
        let parsed = parse(&["gcode", "-c", "x", "-y"]).unwrap();
        assert!(parsed.yes);
        assert!(!parsed.dry_run);
        assert!(!parsed.json);
    }

    #[test]
    fn model_and_tuning_values_pass_through() {
        let parsed = parse(&[
            "gcode",
            "-c",
            "x",
            "--model",
            "kitty-bash-llm",
            "--n-threads",
            "4",
            "--n-gpu-layers",
            "12",
            "--context-size",
            "8192",
            "--temperature",
            "0.1",
        ])
        .unwrap();
        assert_eq!(parsed.model.as_deref(), Some("kitty-bash-llm"));
        assert_eq!(parsed.n_threads, Some(4));
        assert_eq!(parsed.n_gpu_layers, Some(12));
        assert_eq!(parsed.context_size, Some(8192));
        let temperature = parsed.temperature.expect("flag was given");
        assert!((temperature - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn in_dir_is_kept_as_a_path() {
        let parsed = parse(&["gcode", "-c", "x", "--in", "/srv/app"]).unwrap();
        assert_eq!(parsed.in_dir, Some(PathBuf::from("/srv/app")));
    }

    #[test]
    fn json_and_no_color_are_independent() {
        let parsed = parse(&["gcode", "-c", "x", "--json"]).unwrap();
        assert!(parsed.json);
        assert!(!parsed.no_color);
    }

    #[test]
    fn mode_display_is_the_name_a_user_would_type() {
        assert_eq!(Mode::Fix.to_string(), "fix");
        assert_eq!(Mode::Interactive.to_string(), "interactive");
        assert_eq!(
            Mode::Generate {
                request: "x".to_owned()
            }
            .to_string(),
            "generate"
        );
        assert_eq!(
            Mode::Complete {
                partial: "x".to_owned()
            }
            .to_string(),
            "complete"
        );
        assert_eq!(
            Mode::Explain {
                command: "x".to_owned()
            }
            .to_string(),
            "explain"
        );
    }

    #[test]
    fn mode_display_does_not_leak_the_request_into_an_error() {
        // A request can be anything the user typed, including a secret they
        // pasted. Error messages carry the mode name, never the payload.
        let mode = Mode::Generate {
            request: "export AWS_SECRET_ACCESS_KEY=hunter2".to_owned(),
        };
        assert_eq!(mode.to_string(), "generate");
    }

    #[test]
    fn help_is_a_success_not_a_usage_error() {
        // `gcode --help` has no mode and no context, so if this travelled as a
        // usage error it would exit 2 and every script checking the binary
        // would conclude it is broken.
        let output = handled(&["gcode", "--help"]).expect("--help must be handled");
        assert!(output.contains("Usage:"), "not help output: {output}");
        assert!(output.contains("--command"), "help omits the mode flag");
    }

    #[test]
    fn version_is_a_success_not_a_usage_error() {
        let output = handled(&["gcode", "--version"]).expect("--version must be handled");
        assert!(
            output.contains(env!("CARGO_PKG_VERSION")),
            "no version in: {output}"
        );
    }

    #[test]
    fn short_help_and_version_are_also_handled() {
        assert!(handled(&["gcode", "-h"]).is_some());
        assert!(handled(&["gcode", "-V"]).is_some());
    }

    #[test]
    fn a_real_usage_error_is_not_reported_as_handled() {
        // The distinction only matters if it is actually made. This is the case
        // that regresses if `parse_from` ever collapses both variants into one.
        assert!(handled(&["gcode", "--nope"]).is_none());
        assert!(handled(&["gcode", "-c", "x", "--fix"]).is_none());
        assert!(handled(&["gcode", "-c", "x", "--context", "99999"]).is_none());
    }

    #[test]
    fn log_level_variants_are_spelled_as_documented() {
        // No flag yet, but the names are a contract with USAGE.md.
        for (level, expected) in [
            (LogLevel::Error, "error"),
            (LogLevel::Warn, "warn"),
            (LogLevel::Info, "info"),
            (LogLevel::Debug, "debug"),
            (LogLevel::Trace, "trace"),
        ] {
            assert_eq!(
                clap::ValueEnum::to_possible_value(&level)
                    .unwrap()
                    .get_name(),
                expected
            );
        }
    }
}
