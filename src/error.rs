//! Error types for the gcode library.
//!
//! Two layers, two crates, on purpose (AGENTS.md section 4):
//!
//! - This module: `thiserror` types. A library hands back a typed error that a
//!   caller can match on. Adding a variant is a breaking change for that
//!   caller, which is why the enum is `#[non_exhaustive]`.
//! - The binary: `anyhow` context. A CLI adds "what I was trying to do" on the
//!   way out and never exposes the context type in an API.
//!
//! A variant exists only when some code path constructs it. The config layer
//! adds the three TOML variants; the model downloader and the history store
//! each add their own when they arrive in Phase 1.4 and 2.1. An error variant
//! with no constructor is a lie about the interface, and it forces every caller
//! to write a dead match arm.

/// Result alias for library operations.
///
/// The error is always [`Error`]. Callers that need context wrap this with
/// `anyhow::Context`, which they do in the binary, not here.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything the gcode library can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The user's home directory could not be determined.
    #[error(
        "could not determine the home directory; gcode needs it to find its \
         config, data, and model directories (set HOME, or use GCODE_CONFIG \
         and GCODE_HISTORY_FILE to point gcode at explicit locations)"
    )]
    HomeDirUnavailable,

    /// An environment variable held a value that is not usable as a path.
    #[error("{variable} is set to a non-absolute path, which gcode will not use")]
    InvalidEnvPath {
        /// The name of the offending variable.
        variable: &'static str,
    },

    /// The config file could not be read.
    #[error("could not read the config file at {}", path.display())]
    ConfigRead {
        /// Where the file is.
        path: std::path::PathBuf,
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },

    /// The config file is not valid, or holds an unknown key.
    ///
    /// The message names the offending key. A user who mistyped one field needs
    /// to be told which one, because "invalid config" sends them looking through
    /// the whole file.
    #[error("invalid config at {}: {message}", path.display())]
    ConfigParse {
        /// Where the file is.
        path: std::path::PathBuf,
        /// The parser's message, which names the key.
        message: String,
    },

    /// A resolved value is outside the range the tool can work with.
    #[error("config field {field} {requirement}")]
    InvalidConfig {
        /// The dotted path of the offending field, e.g. `model.temperature`.
        field: String,
        /// The accepted range, in words.
        requirement: String,
    },

    /// `always_confirm` was set to `CRITICAL`.
    ///
    /// Its own variant rather than a field range, because the reason is not
    /// that the value is out of range. It is that a threshold of CRITICAL can
    /// never be reached by a command that runs, so accepting it would tell the
    /// user they had configured something the tool cannot deliver.
    #[error(
        "safety.always_confirm cannot be CRITICAL: critical commands are never run, \
         so that threshold can never be met; use HIGH or below"
    )]
    InvalidAlwaysConfirm,

    /// The embedded model registry failed validation.
    ///
    /// Carries a rendered message rather than the `RegistryError` itself,
    /// because the accessor hands back a `&'static` result and cannot lend out a
    /// reference to a value it does not hold.
    /// Inference failed or produced nothing usable.
    ///
    /// Carries a rendered message for the same reason [`Error::Registry`] does:
    /// the accessor is behind a `OnceLock` and cannot lend out a reference to a
    /// value it does not hold.
    #[error("{message}")]
    Inference {
        /// The failure, already formatted for a terminal.
        message: String,
    },

    /// The embedded model registry failed validation.
    #[error("{message}")]
    Registry {
        /// The validation failure, already formatted for a terminal.
        message: String,
    },

    /// Neither `--model` nor the config named a model, and none is the default.
    ///
    /// A distinct error rather than a silent fallback to the registry default,
    /// because a user who asked for a specific model and did not get it must be
    /// told, not handed a different one.
    #[error(
        "no model was named. Pass --model, or set `model` in {}, or pass --model \
         with the full path to a local .gguf file",
        crate::utils::paths::config_file_display()
    )]
    ModelNameRequired,

    /// The command classified above `SAFE` and was not confirmed.
    ///
    /// Not [`Error::RiskBlocked`]: the command *can* run, it was not allowed to
    /// this time. A script that distinguishes the two can retry with `--yes`,
    /// and one that does not can still see the level in this message.
    #[error("{level}: {} was not confirmed, so nothing ran", .level)]
    ConsentDenied {
        /// The level that needed consent.
        level: crate::safety::Risk,
    },

    /// The command classified as `CRITICAL` and cannot run.
    ///
    /// Carries the reasons so the user learns *why* without running `--explain`.
    /// There is no override, no flag, and no config key that reaches this
    /// variant, which is the point (ADR 0008).
    #[error("{level}, and this cannot be overridden:\n{}", .reasons.join("\n"))]
    RiskBlocked {
        /// The level, always `CRITICAL`.
        level: crate::safety::Risk,
        /// One line per reason the classifier matched.
        reasons: Vec<String>,
    },

    /// A mode needs a model and no engine was supplied.
    ///
    /// A distinct error rather than a panic or an empty command: the honest
    /// answer before the inference pipeline lands is "I have no model", not a
    /// plausible command invented to look busy.
    #[error(
        "the generate, complete, and fix modes need a model, and none is loaded. \
         Pass --model, or set `model` in {}, or run with --explain, which needs none",
        crate::utils::paths::config_file_display()
    )]
    NoEngine,

    /// The history file could not be read, written, or rotated.
    ///
    /// One variant rather than several. A history failure is never a reason to
    /// refuse a command: the file is a convenience for the next invocation, and a
    /// user whose history directory is read-only should still get their command.
    /// So this is reported and the work continues, which is the caller's decision
    /// to make with the error in hand.
    #[error("history: {message}")]
    History {
        /// What failed, with the path involved.
        message: String,
    },

    /// A mode is parsed and validated but has no implementation yet.
    #[error("the {mode} mode is parsed but not implemented yet")]
    ModeNotWired {
        /// The mode name, as a user would type it.
        mode: String,
    },

    /// `$SHELL` is not a shell gcode has a hook for.
    ///
    /// Refused rather than guessed at. Writing a bash hook into a fish config
    /// would leave the user with a shell that silently records nothing and a
    /// success message.
    #[error(
        "cannot install the shell hook for {shell}: gcode has hooks for bash and zsh only. \
         Re-run with --shell bash or --shell zsh to choose one"
    )]
    UnsupportedShell {
        /// The basename of `$SHELL`, as found.
        shell: String,
    },

    /// An rc file exists but could not be read.
    ///
    /// Distinct from "the file does not exist", which is not an error. This one
    /// matters because truncating an unreadable rc file would delete a user's
    /// shell configuration and then report success.
    #[error(
        "cannot read the shell configuration at {}: {source}. gcode did not change it. \
         Fix the permissions, or run with --shell and an explicit path",
        path.display()
    )]
    ShellRcUnreadable {
        /// The rc file that could not be read.
        path: std::path::PathBuf,
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },

    /// The rc file could not be written.
    #[error("cannot write the shell configuration at {}: {source}", path.display())]
    ShellRcWrite {
        /// The rc file that could not be written.
        path: std::path::PathBuf,
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },

    /// gcode's copy of the hook could not be written.
    #[error("cannot write the shell hook to {}: {source}", path.display())]
    ShellHookWrite {
        /// Where the hook file should have gone.
        path: std::path::PathBuf,
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },

    /// An rc file has a begin marker with no end marker.
    ///
    /// Raised rather than repaired. Appending a second block would leave the file
    /// with two, and guessing where the first one was meant to stop would risk
    /// deleting the user's own lines on the next remove.
    #[error(
        "the gcode block in the shell configuration is not terminated: {} has no {} after it. \
         gcode did not change the file; delete the block by hand, or remove the stray marker line",
        crate::shell::BEGIN_MARKER,
        crate::shell::END_MARKER
    )]
    ShellBlockMalformed,

    /// `--download-model` named a model the embedded registry does not have.
    ///
    /// Refused rather than falling back to the default. A user who typed a name
    /// expects that model, and handing them a different one under a success
    /// message is the failure this project treats as a lie.
    #[error("no model named {name} in the registry. Run --list-models to see the names")]
    UnknownModel {
        /// The name the user typed.
        name: String,
    },

    /// A download was requested from a build that cannot reach the network.
    ///
    /// The `download` feature is off, so this binary carries no HTTP client at
    /// all ([ADR 0001](docs/adr/0001-local-first-offline-inference.md)). Saying
    /// so is the honest answer; silently failing to fetch would look like a
    /// network problem the user cannot fix.
    #[error(
        "this build has no downloader. Rebuild with `--features download`, or \
         place the .gguf file in the model directory by hand"
    )]
    DownloadUnavailable,

    /// A download was requested while `no_model` is set in the configuration.
    #[error("a model download was requested, but `no_model` is set in the configuration")]
    DownloadRefused,

    /// An approved command could not be started.
    ///
    /// Not the command failing: a command that runs and exits non-zero is a
    /// status, not an error. This is `sh` itself failing to start.
    #[error("could not run the command: {message}")]
    Exec {
        /// What the operating system reported.
        message: String,
    },

    /// A model download failed.
    ///
    /// Carries the rendered [`DownloadError`](crate::model::download::DownloadError),
    /// which already names the URL, both digests on a mismatch, and whether the
    /// partial file was kept.
    #[error("{message}")]
    Download {
        /// The failure, already formatted for a terminal.
        message: String,
    },
}
