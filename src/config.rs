//! Configuration: defaults, file, environment, flags.
//!
//! Four sources, in increasing priority:
//!
//! 1. built-in defaults
//! 2. `~/.config/gcode/config.toml`
//! 3. environment variables
//! 4. command-line flags
//!
//! # Why two types
//!
//! [`Config`] is what the file and the environment produce. Every field is
//! [`Option`], and `None` means "not specified", which is different from
//! "specified as the default value". Collapsing the two would make it
//! impossible to honour a lower-priority source: if the file says
//! `temperature = 0.2` and that happens to equal the default, there is no way
//! to tell whether the user chose it.
//!
//! [`EffectiveConfig`] is the result of resolving all four. Every field is
//! concrete. Nothing downstream needs a default, because there is nothing left
//! to default.
//!
//! # Why resolution is one function
//!
//! Precedence bugs are silent. The worst outcome is a user who sets
//! `GCODE_MODEL` in their shell, does not notice it is being ignored, and gets
//! a different model than the one they configured. Doing the merge in exactly
//! one function, with a test per source, is the only way that stays true as
//! fields are added.
//!
//! # Missing is not broken
//!
//! A missing config file is the normal case, not an error. gcode has to work
//! on a fresh machine with no setup. A *malformed* file is an error that names
//! the path and the field, because silently ignoring a file the user wrote is
//! how you get a config that appears to do nothing.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::utils::paths;

/// Environment variable that overrides the model name or path.
pub const ENV_MODEL: &str = "GCODE_MODEL";
/// Environment variable that overrides the model download mirror.
pub const ENV_MODEL_MIRROR: &str = "GCODE_MODEL_MIRROR";
/// Environment variable that disables history when set to a truthy value.
pub const ENV_NO_HISTORY: &str = "GCODE_NO_HISTORY";
/// Environment variable that skips shell-hook installation.
pub const ENV_NO_INIT: &str = "GCODE_NO_INIT";
/// Environment variable that skips the git part of the environment context.
pub const ENV_NO_GIT: &str = "GCODE_NO_GIT";
/// Environment variable that skips the whole environment context.
pub const ENV_NO_ENV: &str = "GCODE_NO_ENV";
/// Environment variable that forbids any model download.
pub const ENV_NO_MODEL: &str = "GCODE_NO_MODEL";
/// Environment variable holding the log level.
pub const ENV_LOG_LEVEL: &str = "GCODE_LOG_LEVEL";

/// The five risk levels, ordered least to most dangerous.
///
/// The ordering is the point: `always_confirm` compares against this with
/// `>=`, so the enum derives [`PartialOrd`]. A new variant inserted in the
/// wrong place would silently change which commands prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum RiskLevel {
    /// Reads only. No prompt.
    Safe,
    /// Reads outside the working tree. No prompt by default.
    Low,
    /// Writes, installs, or walks the filesystem broadly.
    #[default]
    Medium,
    /// Destructive, privileged, or network-facing.
    High,
    /// Unrunnable. Never prompted, never executed.
    Critical,
}

impl RiskLevel {
    /// The name as it appears in the config file and in `--json`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The minimum risk that always prompts, whatever `--yes` says.
///
/// `CRITICAL` is absent on purpose and not by accident: a critical command is
/// never run at all, so a threshold that high would be unreachable rather than
/// strict. Raising this to CRITICAL must not become a way to run one.
const MIN_ALWAYS_CONFIRM: RiskLevel = RiskLevel::Safe;
/// The highest threshold a user may set. `CRITICAL` is excluded, see above.
const MAX_ALWAYS_CONFIRM: RiskLevel = RiskLevel::High;

/// Rejects an `always_confirm` that cannot be honoured.
///
/// Separate from `resolve` so the check sits next to the constants that
/// document why `CRITICAL` is excluded, instead of as an inline `if` the
/// reader has to interpret.
fn check_always_confirm(level: RiskLevel) -> Result<()> {
    // Written as a range check rather than `level == Critical` so that the two
    // constants above are the single statement of what is allowed. Adding a
    // variant above High would then need a decision here, not a silent pass.
    if level < MIN_ALWAYS_CONFIRM || level > MAX_ALWAYS_CONFIRM {
        return Err(Error::InvalidAlwaysConfirm);
    }
    Ok(())
}

/// `[model]` as written in the file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    /// Registry name of the model.
    pub name: Option<String>,
    /// Path to a GGUF file. Overrides `name` when both are given.
    pub path: Option<PathBuf>,
    /// Inference threads. 0 means all cores.
    pub n_threads: Option<usize>,
    /// Layers to offload to the GPU.
    pub n_gpu_layers: Option<i32>,
    /// Prompt context window, in tokens.
    pub context_size: Option<u32>,
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// Nucleus sampling cutoff.
    pub top_p: Option<f32>,
    /// Hard cap on generated tokens.
    pub max_tokens: Option<u32>,
}

/// `[context]` as written in the file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    /// How many recent history entries to read.
    pub history_entries: Option<usize>,
    /// Bytes of command output kept per history entry.
    pub output_tail_bytes: Option<usize>,
    /// Whether to include git status in the prompt.
    pub include_git: Option<bool>,
    /// Whether to include the environment in the prompt.
    pub include_env: Option<bool>,
    /// Whether to include the working directory in the prompt.
    pub include_cwd: Option<bool>,
}

/// `[safety]` as written in the file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyConfig {
    /// The lowest risk that prompts before running.
    ///
    /// `--yes` suppresses the prompt at every level, so this decides *which*
    /// levels prompt in the first place rather than imposing a floor no flag can
    /// lower. It previously read "regardless of `--yes`", which contradicted the
    /// roadmap's `--yes` test and made the flag unusable at the default `MEDIUM`.
    pub always_confirm: Option<RiskLevel>,
    /// Commands refused unconditionally. Not overridable by any flag.
    pub blocklist: Option<Vec<String>>,
    /// Whether to show the explanation line by default.
    pub explain: Option<bool>,
}

/// `[shell]` as written in the file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellConfig {
    /// Whether to install the history hook.
    pub hook: Option<bool>,
    /// Whether to capture command output for history.
    pub capture_output: Option<bool>,
    /// Extra environment variable patterns to redact. Extends, never replaces.
    pub redact_env: Option<Vec<String>>,
}

/// `[ui]` as written in the file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiConfig {
    /// Whether to use ANSI colour.
    pub color: Option<bool>,
    /// Whether to use emoji in output.
    pub emoji: Option<bool>,
    /// Whether to animate spinners.
    pub animation: Option<bool>,
}

/// The configuration file, as written.
///
/// Every field is optional. `None` means the user did not say, which is what
/// makes the four-source merge in [`Config::resolve`] correct.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Model settings.
    #[serde(default)]
    pub model: ModelConfig,
    /// Context settings.
    #[serde(default)]
    pub context: ContextConfig,
    /// Safety settings.
    #[serde(default)]
    pub safety: SafetyConfig,
    /// Shell integration settings.
    #[serde(default)]
    pub shell: ShellConfig,
    /// Interface settings.
    #[serde(default)]
    pub ui: UiConfig,
}

/// The environment, read once so resolution stays pure and testable.
///
/// Reading `std::env` inside `resolve` would make it untestable without
/// mutating the process environment, and tests that mutate the environment
/// race each other. This struct is the seam.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Env {
    /// `GCODE_MODEL`.
    pub model: Option<String>,
    /// `GCODE_MODEL_MIRROR`.
    pub model_mirror: Option<String>,
    /// `GCODE_NO_HISTORY`.
    pub no_history: Option<bool>,
    /// `GCODE_NO_INIT`.
    pub no_init: Option<bool>,
    /// `GCODE_NO_GIT`. Phase 2.2.
    pub no_git: Option<bool>,
    /// `GCODE_NO_ENV`. Phase 2.2. Stronger than `no_git`: it suppresses every
    /// environment fact, not just the repository one.
    pub no_env: Option<bool>,
    /// `GCODE_NO_MODEL`.
    pub no_model: Option<bool>,
    /// `GCODE_LOG_LEVEL`.
    pub log_level: Option<String>,
    /// `$SHELL`, the login shell. Not a `GCODE_` variable, but read here for the
    /// same reason: the environment context needs it, and reading `std::env` at the
    /// point of use is what makes that untestable.
    pub shell: Option<String>,
    /// `GCODE_CONFIG`, kept for diagnostics. The path itself is resolved by
    /// [`paths::config_file`], not here.
    pub config: Option<PathBuf>,
    /// `GCODE_HISTORY_FILE`.
    pub history_file: Option<PathBuf>,
}

impl Env {
    /// Reads the real environment.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            model: var(ENV_MODEL),
            model_mirror: var(ENV_MODEL_MIRROR),
            no_history: flag(ENV_NO_HISTORY),
            no_init: flag(ENV_NO_INIT),
            no_git: flag(ENV_NO_GIT),
            no_env: flag(ENV_NO_ENV),
            no_model: flag(ENV_NO_MODEL),
            log_level: var(ENV_LOG_LEVEL),
            shell: var("SHELL"),
            config: var_path("GCODE_CONFIG"),
            history_file: var_path("GCODE_HISTORY_FILE"),
        }
    }
}

/// A non-empty environment variable.
fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// A boolean environment variable.
///
/// Unset is `None`, not `false`: `GCODE_NO_HISTORY=` must not silently disable
/// history, and neither must a variable that is set to something unparseable.
/// Only an explicit truthy value turns it on, and only an explicit falsy one
/// turns it off.
fn flag(name: &str) -> Option<bool> {
    match var(name)?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        // Unparseable is treated as unset, not as false. A typo in
        // GCODE_NO_HISTORY should not quietly mean "history is on".
        _ => None,
    }
}

/// An environment variable holding an absolute path.
fn var_path(name: &str) -> Option<PathBuf> {
    var(name).map(PathBuf::from)
}

/// The fully-resolved configuration. No field is optional.
///
/// Constructing one of these requires calling [`Config::resolve`], so there is
/// no way to obtain a half-configured gcode.
///
/// The fields are flat rather than grouped into sub-structs on purpose. The
/// point of this type is that a consumer reads one name, `self.temperature`,
/// and is certain it came from the right source in the right order. Nesting
/// would push that check back onto every reader.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveConfig {
    /// Model registry name, if configured.
    pub model_name: Option<String>,
    /// Explicit model file path, if configured.
    pub model_path: Option<PathBuf>,
    /// Model download mirror, if configured.
    pub model_mirror: Option<String>,
    /// Whether a model download is forbidden.
    pub no_model: bool,
    /// Inference threads. 0 means all cores.
    pub n_threads: usize,
    /// Layers to offload to the GPU.
    pub n_gpu_layers: i32,
    /// Prompt context window, in tokens.
    pub context_size: u32,
    /// Sampling temperature.
    pub temperature: f32,
    /// Nucleus sampling cutoff.
    pub top_p: f32,
    /// Hard cap on generated tokens.
    pub max_tokens: u32,
    /// How many recent history entries to read.
    pub history_entries: usize,
    /// Bytes of command output kept per history entry.
    pub output_tail_bytes: usize,
    /// Whether to include git status in the prompt.
    pub include_git: bool,
    /// Whether to include the environment in the prompt.
    pub include_env: bool,
    /// Whether to include the working directory in the prompt.
    pub include_cwd: bool,
    /// The lowest risk that always prompts.
    pub always_confirm: RiskLevel,
    /// Commands refused unconditionally.
    pub blocklist: Vec<String>,
    /// Whether to show the explanation line.
    pub explain: bool,
    /// Whether to install the history hook.
    pub hook: bool,
    /// Whether to capture command output.
    pub capture_output: bool,
    /// Extra environment patterns to redact.
    pub redact_env: Vec<String>,
    /// Whether to use ANSI colour.
    pub color: bool,
    /// Whether to use emoji.
    pub emoji: bool,
    /// Whether to animate spinners.
    pub animation: bool,
    /// Whether history is disabled.
    pub no_history: bool,
    /// Whether shell-hook installation is suppressed.
    pub no_init: bool,
    /// The log level name, if configured.
    pub log_level: Option<String>,
}

/// Values from the command line, already parsed by `clap`.
///
/// Only the fields the CLI can set appear here. Anything the CLI has no flag
/// for is resolved from the file and environment alone, and that is deliberate:
/// a flag is a promise to support the setting, and adding one before the
/// behaviour behind it exists would be a lie in `--help`.
///
/// The switches are `bool` and the values are `Option<T>`. A switch has no
/// value to spell, so `false` can only mean "not typed", which is the same thing
/// as absent. A value flag has a real range of meanings for `Some`, so `None` is
/// load-bearing. Mixing the two styles would be the bug.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    /// `--model`
    pub model: Option<String>,
    /// `--n-threads`
    pub n_threads: Option<usize>,
    /// `--n-gpu-layers`
    pub n_gpu_layers: Option<i32>,
    /// `--context-size`
    pub context_size: Option<u32>,
    /// `--temperature`
    pub temperature: Option<f32>,
    /// `--context`
    pub context_entries: Option<usize>,
    /// `--no-history`
    pub no_history: bool,
    /// `--no-git`
    pub no_git: bool,
    /// `--no-env`
    pub no_env: bool,
    /// `--no-color`
    pub no_color: bool,
}

impl From<&crate::cli::Parsed> for Overrides {
    /// Reads the flags a user can actually set.
    ///
    /// Only the flags that exist in [`crate::cli::Cli`] appear here. A setting
    /// with no flag is reachable from the file and the environment only, which
    /// is the honest position: adding a flag before the behaviour behind it
    /// exists would put a promise in `--help` that nothing keeps.
    fn from(parsed: &crate::cli::Parsed) -> Self {
        Self {
            model: parsed.model.clone(),
            n_threads: parsed.n_threads,
            n_gpu_layers: parsed.n_gpu_layers,
            context_size: parsed.context_size,
            temperature: parsed.temperature,
            context_entries: parsed.context_entries,
            no_history: parsed.no_history,
            no_git: parsed.no_git,
            no_env: parsed.no_env,
            no_color: parsed.no_color,
        }
    }
}

/// Defaults for every field, in one place.
///
/// Kept as a function rather than a `const` so that the vectors are allocated
/// fresh per call. A `static` blocklist would be shared mutable-looking state
/// that a careless caller could reach.
#[must_use]
pub fn defaults() -> EffectiveConfig {
    EffectiveConfig {
        model_name: None,
        model_path: None,
        model_mirror: None,
        no_model: false,
        n_threads: 0,
        n_gpu_layers: 0,
        context_size: 4096,
        temperature: 0.2,
        top_p: 0.9,
        max_tokens: 256,
        history_entries: 15,
        output_tail_bytes: 2048,
        include_git: true,
        include_env: true,
        include_cwd: true,
        always_confirm: RiskLevel::Medium,
        // Not a safety control. The blocklist is a user convenience; the
        // classifier is what refuses a dangerous command, and it does not read
        // this list. A user who clears it gets no more dangerous tool.
        blocklist: vec![
            "rm -rf /".to_owned(),
            "mkfs".to_owned(),
            "dd of=/dev/".to_owned(),
            ":(){ :|:& };:".to_owned(),
        ],
        explain: true,
        hook: true,
        capture_output: true,
        redact_env: vec![
            "*_TOKEN".to_owned(),
            "*_SECRET".to_owned(),
            "*_KEY".to_owned(),
            "AWS_*".to_owned(),
            "GITHUB_TOKEN".to_owned(),
        ],
        color: true,
        emoji: true,
        animation: false,
        no_history: false,
        no_init: false,
        log_level: None,
    }
}

/// Parses config text, reporting the offending field by name.
///
/// The parser is good at saying *what* is wrong and bad at saying *where*: for
/// a type error it produces `invalid type: string "hot", expected f32`, with
/// no key. The user is left scanning a file of thirty keys for the one that is
/// a string. The byte span fixes that, and it is already in the error, so the
/// key is recovered from the source rather than from a second parse.
///
/// # Errors
///
/// Returns [`Error::ConfigParse`] with a message naming the path, the line, and
/// the dotted key, whenever one can be recovered.
fn parse_text(text: &str, path: &Path) -> Result<Config> {
    toml::from_str(text).map_err(|source| {
        let mut message = String::new();
        if let Some(location) = locate(text, &source) {
            message.push_str(&location);
            message.push_str(": ");
        }
        message.push_str(source.message());
        Error::ConfigParse {
            path: path.to_path_buf(),
            message,
        }
    })
}

/// Where in `text` the parser pointed, as a human-readable location.
///
/// Returns something like `line 2, model.context_size`. Returns `None` when the
/// parser has no span, or when the span does not land on a `key = value` line,
/// which is the case for an unknown key: there the parser points at the
/// enclosing table header because the offending name is what it cannot find, and
/// it already says so in its own message.
///
/// Written to degrade rather than to fail. A slightly vaguer message is a much
/// smaller problem than a config that refuses to load because the error
/// formatter had an off-by-one.
fn locate(text: &str, error: &toml::de::Error) -> Option<String> {
    let span = error.span()?;
    // A span that starts past the end means the parser is describing a
    // position it inferred, not one it read. Offsets are clamped so a malformed
    // file cannot index out of bounds here.
    if span.start > text.len() {
        return None;
    }
    let before = &text[..span.start];
    let line_number = before.lines().count().max(1);
    let line = text.lines().nth(line_number - 1)?;

    let section = current_section(&text[..before.len()]);
    let column = span.start - before.rfind('\n').map_or(0, |index| index + 1);
    let key = key_before_caret(line, column)?;

    Some(match section {
        Some(section) => format!("line {line_number}, {section}.{key}"),
        None => format!("line {line_number}, {key}"),
    })
}

/// The innermost `[section]` in effect at `offset`.
///
/// Steps back over the current line first: a `[model]` header and the field
/// beneath it are in the same section, and the header's own byte offset is
/// before the field.
fn current_section(text: &str) -> Option<String> {
    let head = text.rfind('\n').map_or(text, |newline| &text[..newline]);
    head.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
        (!inner.contains(']')).then(|| inner.to_owned())
    })
}

/// The key on `line` that ends at or before `column`.
fn key_before_caret(line: &str, column: usize) -> Option<String> {
    let head: String = line.chars().take(column).collect();
    let key = head.split('=').next()?.trim();
    if key.is_empty() {
        return None;
    }
    let is_identifier = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    is_identifier.then(|| key.to_owned())
}

impl Config {
    /// Reads the config file at `path`.
    ///
    /// A missing file yields `None`, because gcode must work on a machine where
    /// nothing has been configured yet. A malformed file is an error naming
    /// the path and the offending key, because a file the user wrote and gcode
    /// silently ignored is worse than no file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigRead`] if the file cannot be read, and
    /// [`Error::ConfigParse`] if it is not valid TOML or has a field of the
    /// wrong type or an unknown key.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path).map_err(|source| Error::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        parse_text(&text, path).map(Some)
    }

    /// Loads the config from the standard location, honouring `GCODE_CONFIG`.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`Config::load`], or from resolving the
    /// config path when the home directory is unavailable.
    pub fn load_default() -> Result<Option<Self>> {
        Self::load(&paths::config_file()?)
    }

    /// Merges defaults, this file, the environment, and CLI flags.
    ///
    /// Later sources win. The order is the one in USAGE.md: flag beats
    /// environment beats file beats default.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] when a merged value falls outside its
    /// documented range, and [`Error::InvalidAlwaysConfirm`] when
    /// `always_confirm` is set to `CRITICAL`.
    pub fn resolve(self, env: &Env, overrides: &Overrides) -> Result<EffectiveConfig> {
        let mut out = defaults();
        let model = self.model;
        let context = self.context;
        let safety = self.safety;
        let shell = self.shell;
        let ui = self.ui;

        // ── model ───────────────────────────────────────────────────────────
        out.model_name = model.name;
        out.model_path = model.path;
        if let Some(name) = &env.model {
            // GCODE_MODEL is documented as "GGUF path or registry name". It
            // displaces the file's value, so it must clear whichever field the
            // file used, or a name from the environment would be silently
            // overridden by a path from the file.
            out.model_name = Some(name.clone());
            out.model_path = None;
        }
        if let Some(overridden) = &overrides.model {
            out.model_name = Some(overridden.clone());
            out.model_path = None;
        }
        out.model_mirror.clone_from(&env.model_mirror);
        out.no_model = env.no_model.unwrap_or(false);
        out.n_threads = overrides
            .n_threads
            .or(model.n_threads)
            .unwrap_or(out.n_threads);
        out.n_gpu_layers = overrides
            .n_gpu_layers
            .or(model.n_gpu_layers)
            .unwrap_or(out.n_gpu_layers);
        out.context_size = overrides
            .context_size
            .or(model.context_size)
            .unwrap_or(out.context_size);
        out.temperature = overrides
            .temperature
            .or(model.temperature)
            .unwrap_or(out.temperature);
        out.top_p = model.top_p.unwrap_or(out.top_p);
        out.max_tokens = model.max_tokens.unwrap_or(out.max_tokens);

        // ── context ─────────────────────────────────────────────────────────
        out.history_entries = overrides
            .context_entries
            .or(context.history_entries)
            .unwrap_or(out.history_entries);
        out.output_tail_bytes = context.output_tail_bytes.unwrap_or(out.output_tail_bytes);
        out.include_git = context.include_git.unwrap_or(out.include_git);
        out.include_env = context.include_env.unwrap_or(out.include_env);
        out.include_cwd = context.include_cwd.unwrap_or(out.include_cwd);
        // `--no-git` and `--no-env` are switches, not values: there is no way to
        // type `--no-git=false`, so a flag that is present always wins and its
        // absence always falls through to the file and then the default.
        if overrides.no_git {
            out.include_git = false;
        }
        if overrides.no_env {
            out.include_env = false;
        }
        out.no_history = overrides.no_history || env.no_history.unwrap_or(false);

        // ── safety ──────────────────────────────────────────────────────────
        if let Some(level) = safety.always_confirm {
            check_always_confirm(level)?;
            out.always_confirm = level;
        }
        if let Some(blocklist) = safety.blocklist {
            out.blocklist = blocklist;
        }
        out.explain = safety.explain.unwrap_or(out.explain);

        // ── shell ───────────────────────────────────────────────────────────
        out.hook = shell.hook.unwrap_or(out.hook);
        if env.no_init == Some(true) {
            out.hook = false;
        }
        out.capture_output = shell.capture_output.unwrap_or(out.capture_output);
        if let Some(extra) = shell.redact_env {
            // Extends the built-ins. Replacing them would let a config file
            // switch off redaction for GITHUB_TOKEN, which is the one thing
            // ADR 0006 says a user may not do.
            for pattern in extra {
                if !out.redact_env.contains(&pattern) {
                    out.redact_env.push(pattern);
                }
            }
        }

        // ── ui ──────────────────────────────────────────────────────────────
        out.color = ui.color.unwrap_or(out.color);
        if overrides.no_color {
            out.color = false;
        }
        out.emoji = ui.emoji.unwrap_or(out.emoji);
        out.animation = ui.animation.unwrap_or(out.animation);

        out.log_level.clone_from(&env.log_level);

        validate(&out)?;
        Ok(out)
    }
}

/// Rejects a resolved config whose values cannot work.
///
/// Separate from `resolve` so that every bound is checked in one place after
/// the merge, rather than each source re-checking its own field and missing
/// the cross-source cases.
///
/// # Errors
///
/// Returns [`Error::InvalidConfig`] naming the field and the accepted range.
fn validate(config: &EffectiveConfig) -> Result<()> {
    // The bounds mirror `cli.rs`. They are repeated rather than shared because
    // the CLI checks what the user typed and this checks what survived the
    // merge; a value can arrive from the file without ever passing through
    // clap, and a file is not less in need of a bound.
    let checks: [(&str, bool, String); 6] = [
        (
            "context.history_entries",
            config.history_entries > 1000,
            "must be at most 1000".to_owned(),
        ),
        (
            "model.temperature",
            !(0.0..=2.0).contains(&config.temperature),
            "must be between 0.0 and 2.0".to_owned(),
        ),
        (
            "model.top_p",
            !(0.0..=1.0).contains(&config.top_p),
            "must be between 0.0 and 1.0".to_owned(),
        ),
        (
            "model.context_size",
            !(512..=32768).contains(&config.context_size),
            "must be between 512 and 32768".to_owned(),
        ),
        (
            "model.max_tokens",
            config.max_tokens == 0 || config.max_tokens > 8192,
            "must be between 1 and 8192".to_owned(),
        ),
        (
            "context.output_tail_bytes",
            config.output_tail_bytes == 0 || config.output_tail_bytes > 1_048_576,
            "must be between 1 and 1048576".to_owned(),
        ),
    ];

    for (field, invalid, requirement) in checks {
        if invalid {
            return Err(Error::InvalidConfig {
                field: field.to_owned(),
                requirement,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        check_always_confirm, defaults, parse_text, Config, EffectiveConfig, Env, Overrides,
        RiskLevel, MAX_ALWAYS_CONFIRM, MIN_ALWAYS_CONFIRM,
    };
    use crate::error::Error;
    use std::path::Path;

    /// Parses through the production path, so the message assertions below
    /// exercise the real formatter rather than a test-only one.
    fn parse(toml_text: &str) -> Result<Config, Error> {
        parse_text(toml_text, Path::new("config.toml"))
    }

    fn resolve(toml_text: &str) -> EffectiveConfig {
        parse(toml_text)
            .expect("fixture must parse")
            .resolve(&Env::default(), &Overrides::default())
            .expect("fixture must resolve")
    }

    fn resolve_with(
        toml_text: &str,
        env: &Env,
        overrides: &Overrides,
    ) -> Result<EffectiveConfig, Error> {
        parse(toml_text)?.resolve(env, overrides)
    }

    // ── defaults ────────────────────────────────────────────────────────────

    #[test]
    fn an_empty_file_yields_exactly_the_defaults() {
        assert_eq!(resolve(""), defaults());
    }

    #[test]
    fn defaults_match_the_documented_values() {
        let d = defaults();
        assert_eq!(d.context_size, 4096);
        assert!((d.temperature - 0.2).abs() < f32::EPSILON);
        assert!((d.top_p - 0.9).abs() < f32::EPSILON);
        assert_eq!(d.max_tokens, 256);
        assert_eq!(d.history_entries, 15);
        assert_eq!(d.output_tail_bytes, 2048);
        assert_eq!(d.always_confirm, RiskLevel::Medium);
        assert!(d.blocklist.contains(&"rm -rf /".to_owned()));
        assert!(d.redact_env.contains(&"GITHUB_TOKEN".to_owned()));
    }

    // ── parsing ─────────────────────────────────────────────────────────────

    #[test]
    fn every_documented_section_parses() {
        let config = resolve(
            r#"
[model]
name = "kitty-bash-llm"
path = "~/.local/share/gcode/models/kitty-bash-llm-q4_k_m.gguf"
n_threads = 0
n_gpu_layers = 0
context_size = 4096
temperature = 0.2
top_p = 0.9
max_tokens = 256

[context]
history_entries = 15
output_tail_bytes = 2048
include_git = true
include_env = true
include_cwd = true

[safety]
always_confirm = "MEDIUM"
blocklist = ["rm -rf /"]
explain = true

[shell]
hook = true
capture_output = true
redact_env = ["*_TOKEN"]

[ui]
color = true
emoji = true
animation = false
"#,
        );
        assert_eq!(config.model_name.as_deref(), Some("kitty-bash-llm"));
        assert!(config.model_path.is_some());
        assert_eq!(config.always_confirm, RiskLevel::Medium);
        assert!(config.emoji);
        assert!(!config.animation);
    }

    #[test]
    fn a_single_bad_field_is_reported_by_name() {
        // The roadmap requires this specifically. A user with a typo in one
        // field needs to be told which one, not that "the config is invalid".
        let error = parse("[model]\ncontext_size = \"four thousand\"\n").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("context_size"), "unnamed: {message}");
    }

    #[test]
    fn a_bad_value_in_any_section_is_reported_by_name() {
        for (fixture, field) in [
            ("[model]\ntemperature = \"hot\"\n", "temperature"),
            ("[context]\nhistory_entries = \"many\"\n", "history_entries"),
            ("[safety]\nexplain = 1\n", "explain"),
            ("[ui]\ncolor = \"yes\"\n", "color"),
        ] {
            let error = parse(fixture).unwrap_err().to_string();
            assert!(error.contains(field), "{field} not named in: {error}");
        }
    }

    #[test]
    fn an_unknown_key_is_an_error_rather_than_silently_ignored() {
        // A misspelled key that is ignored is a config that appears to do
        // nothing, which is the failure mode this module exists to prevent.
        let error = parse("[model]\ncontext_sizes = 4096\n").unwrap_err();
        assert!(error.to_string().contains("context_sizes"));
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        assert!(parse("[modle]\nname = \"x\"\n").is_err());
    }

    #[test]
    fn a_field_is_still_named_after_a_comment_and_indentation() {
        // The location is recovered from the source text, so it has to survive
        // the formatting a real config file has.
        let error = parse("[model]\n  # how many tokens\n  temperature = \"hot\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("model.temperature"), "unnamed: {error}");
        assert!(error.contains("line 3"), "wrong line: {error}");
    }

    #[test]
    fn a_structural_error_names_the_key_without_inventing_a_field() {
        // A duplicate table header is not a field problem, and claiming one
        // would send the user looking in the wrong place. The parser's own
        // message already names `model`, so that is what should appear.
        let error = parse("[model]\n[model]\n").unwrap_err().to_string();
        assert!(error.contains("duplicate key"), "no cause: {error}");
        assert!(error.contains("model"), "no key: {error}");
    }

    #[test]
    fn a_top_level_key_is_reported_without_a_section() {
        // A key outside any table is a mistake, and the message must not invent
        // a section for it.
        let error = parse("context_size = 1\n").unwrap_err().to_string();
        assert!(error.contains("context_size"), "no key: {error}");
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let path = std::path::Path::new("/nonexistent/gcode/config.toml");
        assert_eq!(Config::load(path).unwrap(), None);
    }

    #[test]
    fn a_malformed_file_names_the_path() {
        let dir = std::env::temp_dir().join("gcode-config-parse-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[model\nbroken").unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();

        assert!(error.contains("config.toml"), "path not named: {error}");
    }

    // ── precedence ──────────────────────────────────────────────────────────

    #[test]
    fn the_file_beats_the_default() {
        let config = resolve("[model]\ntemperature = 0.9\n");
        assert!((config.temperature - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn the_environment_beats_the_file() {
        // Required by the roadmap.
        let env = Env {
            model: Some("from-env".to_owned()),
            ..Env::default()
        };
        let config = resolve_with(
            "[model]\nname = \"from-file\"\n",
            &env,
            &Overrides::default(),
        )
        .unwrap();
        assert_eq!(config.model_name.as_deref(), Some("from-env"));
    }

    #[test]
    fn a_flag_beats_the_environment_and_the_file() {
        // Required by the roadmap.
        let env = Env {
            model: Some("from-env".to_owned()),
            ..Env::default()
        };
        let overrides = Overrides {
            model: Some("from-flag".to_owned()),
            ..Overrides::default()
        };
        let config = resolve_with("[model]\nname = \"from-file\"\n", &env, &overrides).unwrap();
        assert_eq!(config.model_name.as_deref(), Some("from-flag"));
    }

    #[test]
    fn a_model_from_the_environment_clears_a_path_from_the_file() {
        // Without this, `GCODE_MODEL=some-name` would be silently overridden by
        // a `path` in the file, because resolve sets path after name.
        let env = Env {
            model: Some("kitty-bash-llm".to_owned()),
            ..Env::default()
        };
        let config = resolve_with(
            "[model]\npath = \"/models/custom.gguf\"\n",
            &env,
            &Overrides::default(),
        )
        .unwrap();
        assert_eq!(config.model_name.as_deref(), Some("kitty-bash-llm"));
        assert_eq!(config.model_path, None, "file path leaked through");
    }

    #[test]
    fn the_environment_switch_beats_the_default() {
        let env = Env {
            no_history: Some(true),
            ..Env::default()
        };
        let config = resolve_with("", &env, &Overrides::default()).unwrap();
        assert!(config.no_history);
    }

    #[test]
    fn the_flag_switch_beats_the_environment_switch() {
        // `--no-history` is a switch, so it can only ever mean "on". The
        // environment turning it off and the flag turning it on is the whole
        // case: the flag is the more specific instruction.
        let env = Env {
            no_history: Some(false),
            ..Env::default()
        };
        let overrides = Overrides {
            no_history: true,
            ..Overrides::default()
        };
        let config = resolve_with("", &env, &overrides).unwrap();
        assert!(config.no_history, "flag must beat the environment");
    }

    #[test]
    fn an_absent_switch_falls_through_to_the_file() {
        let config = resolve_with(
            "[context]\ninclude_git = true\n",
            &Env::default(),
            &Overrides {
                no_git: false,
                ..Overrides::default()
            },
        )
        .unwrap();
        assert!(config.include_git);
    }

    #[test]
    fn a_switch_flag_disables_what_the_file_enabled() {
        for (flag, field) in [
            (
                Overrides {
                    no_git: true,
                    ..Overrides::default()
                },
                "include_git",
            ),
            (
                Overrides {
                    no_env: true,
                    ..Overrides::default()
                },
                "include_env",
            ),
        ] {
            let fixture = format!("[context]\n{field} = true\n");
            let config = resolve_with(&fixture, &Env::default(), &flag).unwrap();
            let actual = match field {
                "include_git" => config.include_git,
                _ => config.include_env,
            };
            assert!(!actual, "{field} survived the switch flag");
        }
    }

    // ── safety settings ─────────────────────────────────────────────────────

    #[test]
    fn always_confirm_cannot_be_set_to_critical() {
        // CRITICAL is unrunnable, so a threshold of CRITICAL is not "extra
        // strict", it is a setting that can never be satisfied. Refusing it
        // stops a user from believing they have configured something stronger
        // than the tool can deliver.
        let error = resolve_with(
            "[safety]\nalways_confirm = \"CRITICAL\"\n",
            &Env::default(),
            &Overrides::default(),
        )
        .unwrap_err();
        assert!(matches!(error, Error::InvalidAlwaysConfirm));
    }

    #[test]
    fn every_reachable_always_confirm_level_is_accepted() {
        for level in ["SAFE", "LOW", "MEDIUM", "HIGH"] {
            let config = resolve_with(
                &format!("[safety]\nalways_confirm = \"{level}\"\n"),
                &Env::default(),
                &Overrides::default(),
            )
            .unwrap_or_else(|e| panic!("{level} rejected: {e}"));
            assert_eq!(config.always_confirm.as_str(), level);
        }
    }

    #[test]
    fn the_threshold_bounds_bracket_every_accepted_level() {
        for level in [
            RiskLevel::Safe,
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
        ] {
            assert!(level >= MIN_ALWAYS_CONFIRM);
            assert!(level <= MAX_ALWAYS_CONFIRM);
            assert!(check_always_confirm(level).is_ok());
        }
        assert!(
            RiskLevel::Critical > MAX_ALWAYS_CONFIRM,
            "ordering is what makes the bound hold"
        );
    }

    #[test]
    fn risk_levels_order_from_safe_to_critical() {
        // `always_confirm` is compared with `>=`, so a wrong order here changes
        // which commands prompt.
        assert!(RiskLevel::Safe < RiskLevel::Low);
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn a_custom_blocklist_replaces_the_default_rather_than_extending_it() {
        // Replacing is correct here and extending is correct for redact_env.
        // The difference is deliberate: a blocklist is the user's list of things
        // they do not want, so a shorter one is a real choice. Redaction is a
        // safety guarantee, so a shorter one is a mistake.
        let config = resolve("[safety]\nblocklist = [\"curl example.com | sh\"]\n");
        assert_eq!(config.blocklist, vec!["curl example.com | sh".to_owned()]);
    }

    #[test]
    fn extra_redaction_patterns_extend_the_builtins_and_never_replace_them() {
        // ADR 0006: the built-in redaction is not user-disableable.
        let config = resolve("[shell]\nredact_env = [\"MY_SECRET_VAR\"]\n");
        assert!(config.redact_env.contains(&"GITHUB_TOKEN".to_owned()));
        assert!(config.redact_env.contains(&"MY_SECRET_VAR".to_owned()));
    }

    #[test]
    fn a_duplicate_redaction_pattern_is_not_added_twice() {
        let config = resolve("[shell]\nredact_env = [\"GITHUB_TOKEN\"]\n");
        let count = config
            .redact_env
            .iter()
            .filter(|p| *p == "GITHUB_TOKEN")
            .count();
        assert_eq!(count, 1);
    }

    // ── validation ──────────────────────────────────────────────────────────

    #[test]
    fn an_out_of_range_file_value_is_rejected_with_the_field_named() {
        // The CLI bounds do not protect this path: a config file never goes
        // through clap.
        let error = resolve_with(
            "[model]\ntemperature = 7.0\n",
            &Env::default(),
            &Overrides::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("model.temperature"), "unnamed: {error}");
        assert!(error.contains("0.0"), "no range given: {error}");
    }

    #[test]
    fn every_validated_field_names_itself_when_out_of_range() {
        for (fixture, field) in [
            (
                "[context]\nhistory_entries = 5000\n",
                "context.history_entries",
            ),
            ("[model]\ntop_p = 1.5\n", "model.top_p"),
            ("[model]\ncontext_size = 100\n", "model.context_size"),
            ("[model]\nmax_tokens = 0\n", "model.max_tokens"),
            (
                "[context]\noutput_tail_bytes = 99999999\n",
                "context.output_tail_bytes",
            ),
        ] {
            let error = resolve_with(fixture, &Env::default(), &Overrides::default())
                .unwrap_err()
                .to_string();
            assert!(error.contains(field), "{field} not named in: {error}");
        }
    }

    #[test]
    fn the_exact_bounds_are_accepted() {
        for fixture in [
            "[context]\nhistory_entries = 1000\n",
            "[model]\ntemperature = 0.0\n",
            "[model]\ntop_p = 1.0\n",
            "[model]\ncontext_size = 512\n",
            "[model]\nmax_tokens = 1\n",
        ] {
            assert!(
                resolve_with(fixture, &Env::default(), &Overrides::default()).is_ok(),
                "rejected a valid boundary: {fixture}"
            );
        }
    }

    #[test]
    fn no_init_from_the_environment_disables_the_hook() {
        let env = Env {
            no_init: Some(true),
            ..Env::default()
        };
        let config = resolve_with("[shell]\nhook = true\n", &env, &Overrides::default()).unwrap();
        assert!(!config.hook);
    }

    #[test]
    fn no_color_from_the_flag_disables_colour() {
        let overrides = Overrides {
            no_color: true,
            ..Overrides::default()
        };
        let config = resolve_with("[ui]\ncolor = true\n", &Env::default(), &overrides).unwrap();
        assert!(!config.color);
    }

    #[test]
    fn resolve_is_deterministic() {
        // Same inputs, same output. A merge that depended on iteration order
        // would make the safety settings unreproducible.
        let text = "[model]\nname = \"m\"\n[safety]\nalways_confirm = \"HIGH\"\n";
        let first = resolve(text);
        let second = resolve(text);
        assert_eq!(first, second);
    }
}
