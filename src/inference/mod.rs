//! The boundary between gcode and a model.
//!
//! Everything downstream of the prompt takes a `&dyn InferenceEngine`, so the
//! whole pipeline can be exercised in milliseconds against a fake and no test
//! in this repository ever loads a 400 MB file. That is not a convenience. It
//! is the difference between a test suite people run before committing and one
//! they skip.
//!
//! # The rules the boundary enforces
//!
//! Three things happen here that the rest of the program depends on, and all
//! three belong on this side of the line rather than in each caller:
//!
//! 1. **Post-processing.** A model asked for a shell command returns a command
//!    wrapped in a markdown fence about half the time, and occasionally a
//!    sentence of explanation first. Stripping that once, here, means
//!    [`crate::safety`] only ever sees what the model actually meant to run.
//! 2. **A timeout.** Thirty seconds of wall clock, then an error. Never a
//!    partial command: a truncated `rm -rf /home/user/pro` is a syntax error
//!    today and a catastrophe the day it is not.
//! 3. **Bounded output.** `max_tokens` is enforced, and so is a hard ceiling on
//!    the returned string, because a runaway generation is a memory problem as
//!    well as a correctness one.

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use crate::error::Result;

/// Wall-clock ceiling on one generation.
///
/// Not a preference. A user who typed a request is waiting for an answer, and
/// on a small machine a 3B model can take a minute to finish a sentence it is
/// still going to get wrong. Thirty seconds is long enough for a normal answer
/// on the slowest hardware this project targets and short enough that a hang is
/// visibly a hang.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Hard ceiling on the characters [`InferenceEngine::generate`] will return,
/// independent of `max_tokens`.
///
/// A token is not a character and no engine is obliged to agree with any other
/// about how many characters a token is. This bound is what stops a broken or
/// hostile generation from turning into unbounded allocation, so it is enforced
/// on the string rather than trusted from the params.
pub const MAX_OUTPUT_CHARS: usize = 8 * 1024;

/// How a generation is to be sampled.
///
/// Mirrors what a GGUF model exposes, minus anything gcode has no business
/// setting. The defaults are the ones the model card recommends for shell
/// output: low temperature, because a creative shell command is a wrong shell
/// command, and a fixed seed so the same question twice gives the same answer
/// and a bug report is reproducible.
#[derive(Debug, Clone, PartialEq)]
pub struct GenParams {
    /// Sampling temperature. `0.0` is greedy.
    pub temperature: f32,
    /// Nucleus sampling cutoff in `0.0..=1.0`.
    pub top_p: f32,
    /// Ceiling on generated tokens.
    pub max_tokens: u32,
    /// RNG seed. `None` seeds from entropy.
    pub seed: Option<u64>,
    /// Wall-clock ceiling.
    pub timeout: Duration,
}

impl Default for GenParams {
    /// Greedy, reproducible, and short.
    ///
    /// A fixed default seed rather than `None`: two runs of the same prompt
    /// producing different commands makes every bug report unreproducible, and
    /// the reproducibility is worth more than the variety for a tool whose
    /// output is about to be classified and possibly executed.
    fn default() -> Self {
        Self {
            temperature: 0.0,
            top_p: 1.0,
            max_tokens: 256,
            seed: Some(0),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl GenParams {
    /// Params with a different token ceiling.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Params with a different wall-clock ceiling.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Whether these params can produce a usable answer.
    ///
    /// # Errors
    ///
    /// Returns [`InferenceError::BadParams`] when `max_tokens` is zero or
    /// either probability is outside `0.0..=1.0`, or the timeout is zero. A zero
    /// timeout would fail on the first token, which reads as a broken
    /// installation rather than a bad flag.
    pub fn validate(&self) -> std::result::Result<(), InferenceError> {
        if self.max_tokens == 0 {
            return Err(InferenceError::BadParams(
                "max_tokens is 0, so no command could ever be produced".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.top_p) {
            return Err(InferenceError::BadParams(format!(
                "top_p is {}, which is outside 0.0..=1.0",
                self.top_p
            )));
        }
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err(InferenceError::BadParams(format!(
                "temperature is {}, which is not a usable value",
                self.temperature
            )));
        }
        if self.timeout.is_zero() {
            return Err(InferenceError::BadParams(
                "timeout is 0, which would fail before the first token".into(),
            ));
        }
        Ok(())
    }
}

/// What a loaded model is, for `--version` and the credits screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    /// Registry name the model was loaded from.
    pub model_name: String,
    /// Native context window in tokens.
    pub context_size: u32,
    /// Threads the model is using.
    pub threads: u32,
    /// Whether sampling is greedy, for the debug line.
    pub greedy: bool,
}

impl fmt::Display for EngineInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} ctx, {} threads{})",
            self.model_name,
            self.context_size,
            self.threads,
            if self.greedy { ", greedy" } else { "" }
        )
    }
}

/// A loaded model that can turn a prompt into text.
///
/// Takes `&dyn` rather than `&self` for the object-safety of the pipeline: the
/// engine is loaded once per process and shared, so callers hold a reference.
/// The trait itself is object safe so `&dyn InferenceEngine` is what the rest of
/// the program passes around.
pub trait InferenceEngine: Send + Sync {
    /// Generates a completion for `prompt`.
    ///
    /// Implementations return the raw generation. Cleaning it up is
    /// [`generate_command`]'s job, not theirs, so that a fake and a real engine
    /// are cleaned identically and a test of the cleaner is a test of what
    /// production actually runs.
    /// # Errors
    ///
    /// Returns [`crate::Error`] when the model cannot be sampled. An
    /// implementation should check `params.timeout` from inside its own
    /// sampling loop where it can, so that a long generation stops promptly
    /// rather than running to completion and being discarded.
    fn generate(&self, prompt: &str, params: &GenParams) -> Result<String>;

    /// What this engine is.
    fn info(&self) -> EngineInfo;
}

/// Anything that can go wrong turning a prompt into a command.
#[derive(Debug)]
pub enum InferenceError {
    /// The model could not be loaded.
    Load {
        /// What failed.
        cause: String,
    },
    /// Sampling parameters that cannot produce an answer.
    BadParams(String),
    /// Generation exceeded its wall clock.
    Timeout {
        /// The ceiling that was hit.
        limit: Duration,
    },
    /// Generation failed inside the engine.
    Generate {
        /// What the engine reported.
        cause: String,
    },
    /// The model produced nothing usable after cleaning.
    Empty,
}

impl fmt::Display for InferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load { cause } => write!(f, "could not load the model: {cause}"),
            Self::BadParams(m) => write!(f, "unusable sampling parameters: {m}"),
            Self::Timeout { limit } => write!(
                f,
                "the model did not finish within {}s, so no command was produced. \
                 A partial command is never run: half of `rm -rf /home/user/pro` \
                 is a different command from the one the model meant. Try a \
                 smaller model, or raise the timeout",
                limit.as_secs()
            ),
            Self::Generate { cause } => write!(f, "the model failed to generate: {cause}"),
            Self::Empty => write!(
                f,
                "the model did not produce a command. Try rephrasing the request, or \
                 name the command you want more precisely"
            ),
        }
    }
}

impl std::error::Error for InferenceError {}

impl From<crate::Error> for InferenceError {
    fn from(e: crate::Error) -> Self {
        match e {
            crate::Error::Inference { message } => Self::Generate { cause: message },
            other => Self::Generate {
                cause: other.to_string(),
            },
        }
    }
}

impl From<InferenceError> for crate::Error {
    fn from(e: InferenceError) -> Self {
        Self::Inference {
            message: e.to_string(),
        }
    }
}

/// Generates a shell command from `prompt`, cleaned and bounded.
///
/// The single entry point the program uses. It is a free function rather than a
/// method so that the timeout, the post-processing, and the emptiness check
/// cannot be bypassed by calling [`InferenceEngine::generate`] directly from
/// somewhere that forgot.
///
/// # Errors
///
/// Returns [`InferenceError::BadParams`] for unusable params,
/// [`InferenceError::Timeout`] when the ceiling is hit, and
/// [`InferenceError::Empty`] when nothing usable came back. A timeout never
/// yields a partial command.
pub fn generate_command(
    engine: &dyn InferenceEngine,
    prompt: &str,
    params: &GenParams,
) -> std::result::Result<String, InferenceError> {
    params.validate()?;

    // The wall clock is enforced here rather than trusted to the engine, because
    // only a process-wide timer can interrupt a call and a real engine that
    // forgets to check its own deadline would otherwise return a late answer
    // that looks fine. Running the generation on its own thread and abandoning it
    // on timeout is the only option that covers a `generate` that ignores the
    // limit: the generation cannot be cancelled, but it also cannot be *used*.
    let raw = run_within(engine, prompt, params)?;
    if raw.len() > MAX_OUTPUT_CHARS {
        return Err(InferenceError::Generate {
            cause: format!(
                "the model returned {} characters, over the {MAX_OUTPUT_CHARS} limit; \
                 refusing to hand a runaway generation to the classifier",
                raw.len()
            ),
        });
    }
    let cleaned = clean(&raw);
    if cleaned.is_empty() {
        return Err(InferenceError::Empty);
    }
    Ok(cleaned)
}

/// Runs `generate` under a wall clock, abandoning it if it overruns.
///
/// The generation goes to a detached thread and the result travels back over a
/// channel. If the answer arrives in time it is returned; if the timer wins, the
/// thread is left to finish on its own and its output is dropped on the floor.
///
/// That leak is deliberate, and it has a cost worth stating plainly: the call
/// does **not** return on time. `thread::scope` joins every thread it spawned
/// before it returns, so a slow generation is waited out even though its output
/// has already been discarded. The timeout guarantees a late answer is never
/// *used*; it does not make a slow model fast.
///
/// For gcode as a CLI this is acceptable, because the process exits immediately
/// afterwards. It would not be acceptable for a long-lived server, and a
/// `LlamaEngine` that can check a deadline from inside its own sampling loop
/// should do so — the real fix is cooperative cancellation, and the thread here
/// is the backstop for an engine that does not offer it.
fn run_within(
    engine: &dyn InferenceEngine,
    prompt: &str,
    params: &GenParams,
) -> std::result::Result<String, InferenceError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let timeout = params.timeout;
    let prompt = prompt.to_owned();

    // `&dyn InferenceEngine` is `Send` because the trait requires `Sync`, so a
    // scoped thread can borrow it without an `Arc` and without the engine being
    // cloneable. `scope` rather than `thread::spawn` because the borrow has to
    // outlive the closure.
    let received = std::thread::scope(|scope| {
        scope.spawn(|| {
            let _ = tx.send(
                engine
                    .generate(&prompt, params)
                    .map_err(InferenceError::from),
            );
        });
        rx.recv_timeout(timeout)
    });

    // Two layers of `Result`: the outer is "did an answer arrive in time", the
    // inner is "what did the engine say". They mean different things and must
    // not be flattened into one.
    match received {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(e),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(InferenceError::Timeout { limit: timeout })
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(InferenceError::Generate {
            cause: "the generation thread ended without producing anything".into(),
        }),
    }
}

/// Strips the packaging a model puts around a command.
///
/// Models asked to emit a shell command reliably wrap it in a markdown fence
/// despite being told not to, and sometimes prefix a sentence of explanation.
/// [`crate::safety`] must see the command and nothing else, so this runs before
/// classification, always, in one place.
///
/// The order is deliberate: fences are stripped first because the fence is the
/// outermost wrapper, then any language tag, then a leading `Command:`-style
/// label, then a leading prose line if the first line does not look like a
/// command at all.
#[must_use]
pub fn clean(raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() {
        return String::new();
    }

    let unfenced = strip_fence(text);
    let untagged = strip_language_tag(&unfenced);
    let unlabelled = strip_label(&untagged);
    strip_leading_prose(&unlabelled)
        .trim()
        .trim_matches('`')
        .trim()
        .to_owned()
}

/// Removes a markdown code fence, keeping what is inside.
///
/// Handles a fence with a language tag, without one, and an unterminated one,
/// which is what a generation cut off at `max_tokens` looks like. Only the
/// first fence is considered: a command that itself contains a fence is not a
/// thing gcode runs.
fn strip_fence(text: &str) -> String {
    let Some(rest) = text.strip_prefix("```") else {
        return text.to_owned();
    };
    // The rest starts with an optional language tag, then a newline.
    let after_tag = match rest.find('\n') {
        Some(nl) => &rest[nl + 1..],
        // A fence with nothing after it: the model produced only a fence.
        None => return String::new(),
    };
    match after_tag.find("```") {
        Some(end) => after_tag[..end].to_owned(),
        // Unterminated: the generation stopped mid-fence. Keeping what came
        // after the opening line is the only useful reading.
        None => after_tag.to_owned(),
    }
}

/// Removes a `bash`, `sh`, `shell`, `console` or `zsh` tag on the first line.
///
/// Only when the first line is nothing but the tag, so a command that happens
/// to start with the word `bash` is not truncated to nothing.
fn strip_language_tag(text: &str) -> String {
    const TAGS: [&str; 5] = ["bash", "sh", "shell", "console", "zsh"];
    let Some(first_nl) = text.find('\n') else {
        // A single line that is only a tag has no command in it.
        let only = text.trim();
        if TAGS.contains(&only) {
            return String::new();
        }
        return text.to_owned();
    };
    let first = text[..first_nl].trim();
    if TAGS.contains(&first) {
        return text[first_nl + 1..].to_owned();
    }
    text.to_owned()
}

/// Removes a leading `Command:`, `Command -`, `$ `, or `> ` marker.
fn strip_label(text: &str) -> String {
    const LABELS: [&str; 6] = ["Command:", "command:", "Command -", "Output:", "$ ", "> "];
    let trimmed = text.trim_start();
    for label in LABELS {
        if let Some(rest) = trimmed.strip_prefix(label) {
            return rest.trim_start().to_owned();
        }
    }
    // A bare `Command:` with no space, e.g. `Command:ls -la`.
    if let Some(rest) = trimmed.strip_prefix("Command:") {
        return rest.trim_start().to_owned();
    }
    text.to_owned()
}

/// Drops a leading sentence of prose, keeping the first line that looks like a
/// command.
///
/// "Sure! Here is the command you asked for: `ls -la`" is one line, so this
/// mostly handles the multi-line case. The test is deliberately loose — does
/// the line contain a token that could start a command? — because being too
/// clever here means eating a real command that begins with an unusual word.
fn strip_leading_prose(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.to_owned();
    }
    let Some((idx, _)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| looks_like_command(line))
    else {
        // Nothing looked like a command. Keeping the text lets the classifier
        // decide, which is safer than returning an empty string that looks like
        // a successful generation of nothing.
        return text.to_owned();
    };
    lines[idx..].join("\n")
}

/// Whether a line plausibly *is* a command rather than a sentence about one.
///
/// A line qualifies if its first token is a plausible command word: a known
/// shell builtin or utility, a path, or a word with a flag. This errs towards
/// `true`, because the cost of guessing wrong in the permissive direction is
/// that a prose line survives to be classified as a low-risk string, while the
/// cost of guessing wrong in the strict direction is a real command being
/// deleted.
fn looks_like_command(line: &str) -> bool {
    // A known command word.
    const COMMANDS: &[&str] = &[
        "ls",
        "cd",
        "cat",
        "grep",
        "rg",
        "find",
        "fd",
        "awk",
        "sed",
        "cut",
        "sort",
        "uniq",
        "wc",
        "head",
        "tail",
        "less",
        "more",
        "echo",
        "printf",
        "pwd",
        "cp",
        "mv",
        "rm",
        "mkdir",
        "rmdir",
        "touch",
        "chmod",
        "chown",
        "ln",
        "df",
        "du",
        "ps",
        "kill",
        "top",
        "htop",
        "git",
        "cargo",
        "rustc",
        "npm",
        "npx",
        "pnpm",
        "yarn",
        "node",
        "python",
        "python3",
        "pip",
        "pip3",
        "make",
        "cmake",
        "docker",
        "systemctl",
        "journalctl",
        "curl",
        "wget",
        "ssh",
        "scp",
        "tar",
        "zip",
        "unzip",
        "diff",
        "patch",
        "tree",
        "which",
        "whereis",
        "man",
    ];

    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    // Anything quoted, redirected, or piped is a command.
    if line.contains('|') || line.contains('>') || line.contains('<') || line.contains('`') {
        return true;
    }
    let first = line.split_whitespace().next().unwrap_or("");
    if first.is_empty() {
        return false;
    }
    // A path is a command.
    if first.starts_with('/') || first.starts_with("./") || first.starts_with('~') {
        return true;
    }
    // A leading flag means the token is an argument, not a command word, so
    // this line is prose.
    if first.starts_with('-') {
        return false;
    }
    // A comment introduces the command that follows it on the next line, so it
    // is not prose and must not start a drop.
    let first_word = first.split('#').next().unwrap_or("").trim();
    if first_word.is_empty() {
        return true;
    }
    if COMMANDS.contains(&first) {
        return true;
    }
    // A word immediately followed by a flag is a command even if the word is not
    // in the list above. `xmllint --format f.xml` must not be mistaken for prose
    // because xmllint was never enumerated: the list is a convenience, not the
    // rule, and a strict check deletes real commands.
    if line.split_whitespace().count() > 1
        && line.split_whitespace().skip(1).any(|t| t.starts_with('-'))
    {
        return true;
    }
    // An env assignment prefix still introduces a command.
    if first.contains('=') && !first.starts_with('-') {
        return true;
    }
    false
}

/// A loaded model, cached for the life of the process.
///
/// gcode is a one-shot command: it generates a command, shows it, and exits. A
/// model load costs seconds and gigabytes of mmap, so it happens exactly once
/// and every later caller shares it. A `OnceLock` rather than a `Mutex` because
/// nothing here is mutated after construction.
///
/// `get` rather than a `new` so a test can install its own engine before the
/// first real call, and cannot then be surprised by a cached real one.
static ENGINE: OnceLock<&'static (dyn InferenceEngine + Sync)> = OnceLock::new();

/// Installs `engine` as the process-wide engine.
///
/// # Errors
///
/// Returns a message if an engine is already installed. This is an error rather
/// than a silent no-op because a test that installs a fake into a process that
/// already loaded a real model would otherwise pass or fail depending on test
/// order, which is the worst possible failure mode in a safety tool.
pub fn install(engine: &'static (dyn InferenceEngine + Sync)) -> std::result::Result<(), String> {
    ENGINE
        .set(engine)
        .map_err(|_| "an inference engine is already loaded in this process".to_owned())
}

/// The installed engine, if there is one.
#[must_use]
pub fn installed() -> Option<&'static (dyn InferenceEngine + Sync)> {
    ENGINE.get().copied()
}

/// The installed engine, or an error explaining that none was loaded.
///
/// # Errors
///
/// Returns [`InferenceError::Load`] when no engine has been installed, so the
/// caller can tell "no model" apart from "the model is broken".
pub fn require() -> Result<&'static (dyn InferenceEngine + Sync)> {
    installed().ok_or_else(|| InferenceError::Load {
        cause: "no model is loaded".into(),
    })?;
    Ok(installed().unwrap_or_else(|| unreachable!("just checked")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// An engine that returns a fixed string and counts its calls.
    struct FakeEngine {
        reply: String,
        calls: AtomicUsize,
        params_seen: OnceLock<GenParams>,
        info: EngineInfo,
    }

    impl FakeEngine {
        fn new(reply: &str) -> Self {
            Self {
                reply: reply.to_owned(),
                calls: AtomicUsize::new(0),
                params_seen: OnceLock::new(),
                info: EngineInfo {
                    model_name: "fake".into(),
                    context_size: 4096,
                    threads: 1,
                    greedy: true,
                },
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl InferenceEngine for FakeEngine {
        fn generate(&self, _prompt: &str, params: &GenParams) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = self.params_seen.set(params.clone());
            Ok(self.reply.clone())
        }

        fn info(&self) -> EngineInfo {
            self.info.clone()
        }
    }

    /// An engine that sleeps past any timeout, then returns a valid-looking
    /// command. Used to prove the timeout produces an error and never a partial.
    struct SlowEngine;

    impl InferenceEngine for SlowEngine {
        fn generate(&self, _prompt: &str, _params: &GenParams) -> Result<String> {
            std::thread::sleep(Duration::from_millis(50));
            Ok("echo done".into())
        }

        fn info(&self) -> EngineInfo {
            EngineInfo {
                model_name: "slow".into(),
                context_size: 2048,
                threads: 1,
                greedy: true,
            }
        }
    }

    /// An engine that returns far more than the output ceiling.
    struct RunawayEngine;

    impl InferenceEngine for RunawayEngine {
        fn generate(&self, _prompt: &str, _params: &GenParams) -> Result<String> {
            Ok("x".repeat(MAX_OUTPUT_CHARS + 1))
        }

        fn info(&self) -> EngineInfo {
            EngineInfo {
                model_name: "runaway".into(),
                context_size: 1,
                threads: 1,
                greedy: true,
            }
        }
    }

    #[test]
    fn a_fake_engine_drives_the_pipeline() {
        let engine = FakeEngine::new("ls -la");
        let out = generate_command(&engine, "list files", &GenParams::default()).expect("generate");
        assert_eq!(out, "ls -la");
        assert_eq!(engine.calls(), 1);
    }

    #[test]
    fn the_params_reach_the_engine() {
        let engine = FakeEngine::new("ls");
        let params = GenParams::default().with_max_tokens(64);
        generate_command(&engine, "list", &params).expect("generate");
        assert_eq!(
            *engine.params_seen.get().expect("params were recorded"),
            params
        );
    }

    #[test]
    fn engine_info_renders_the_context_and_threads() {
        let engine = FakeEngine::new("ls");
        let text = engine.info().to_string();
        assert!(text.contains("fake"));
        assert!(text.contains("4096"));
        assert!(text.contains("greedy"), "greedy shown: {text}");
    }

    // ── post-processing ────────────────────────────────────────────────────

    #[test]
    fn a_markdown_fence_is_stripped() {
        assert_eq!(clean("```bash\nrm -rf /tmp/x\n```"), "rm -rf /tmp/x");
        assert_eq!(clean("```\nls -la\n```"), "ls -la");
    }

    #[test]
    fn an_unterminated_fence_still_yields_the_command() {
        // What a generation cut off at max_tokens looks like.
        assert_eq!(clean("```bash\ndf -h"), "df -h");
    }

    #[test]
    fn a_fence_with_nothing_in_it_yields_nothing() {
        assert_eq!(clean("```"), "");
    }

    #[test]
    fn a_language_tag_on_its_own_line_is_removed() {
        assert_eq!(clean("sh\nls"), "ls");
        assert_eq!(clean("console\npwd"), "pwd");
    }

    /// A command that legitimately begins with the word `bash` must survive.
    #[test]
    fn a_command_starting_with_bash_is_not_mistaken_for_a_tag() {
        assert_eq!(clean("bash script.sh"), "bash script.sh");
    }

    #[test]
    fn a_leading_command_label_is_removed() {
        assert_eq!(clean("Command: rm -rf /tmp/x"), "rm -rf /tmp/x");
        assert_eq!(clean("command: df -h"), "df -h");
        assert_eq!(clean("Output:\nwhoami"), "whoami");
    }

    #[test]
    fn a_dollar_prompt_is_removed() {
        assert_eq!(clean("$ ls -la"), "ls -la");
    }

    #[test]
    fn a_leading_prose_line_is_dropped() {
        let raw = "Sure! Here is the command you asked for.\ndf -h /";
        assert_eq!(clean(raw), "df -h /");
    }

    /// Erring towards keeping the line matters: a strict check eats real
    /// commands whose first word is not in any list.
    #[test]
    fn a_command_whose_first_word_is_unusual_survives() {
        let raw = "I can help with that.\nxmllint --format f.xml";
        let out = clean(raw);
        assert_eq!(out, "xmllint --format f.xml", "unusual command eaten");
    }

    #[test]
    fn a_path_command_survives_prose_stripping() {
        let raw = "That will list them.\n/usr/bin/find . -name '*.rs'";
        assert_eq!(clean(raw), "/usr/bin/find . -name '*.rs'");
    }

    #[test]
    fn an_env_assignment_prefix_survives() {
        assert_eq!(clean("EDITOR=nvim vi file.txt"), "EDITOR=nvim vi file.txt");
    }

    #[test]
    fn a_piped_command_survives_prose_stripping() {
        let raw = "Here you go:\nps aux | grep sshd";
        assert_eq!(clean(raw), "ps aux | grep sshd");
    }

    /// If no line looks like a command, the text is kept rather than emptied.
    /// Returning empty would look like a successful generation of nothing and
    /// hide the model's actual output from the classifier.
    #[test]
    fn prose_with_no_command_is_kept_for_the_classifier() {
        let raw = "I cannot do that.\nIt would be dangerous.";
        assert_eq!(clean(raw), raw);
    }

    #[test]
    fn whitespace_only_yields_nothing() {
        assert_eq!(clean("   \n\t "), "");
    }

    #[test]
    fn a_wrapped_answer_is_cleaned_end_to_end() {
        let raw = "```bash\n# list every file\nls -la\n```";
        assert_eq!(clean(raw), "# list every file\nls -la");
    }

    // ── bounds and failures ────────────────────────────────────────────────

    /// A timeout must be an error, never the string the slow engine would have
    /// produced. This is the assertion that matters: a truncated or late command
    /// is a different command from the one the model meant, and a destructive
    /// one that got cut in the wrong place is a catastrophe.
    ///
    /// The engine here ignores its deadline entirely, which is the point. The
    /// guarantee is enforced above the engine, so a real engine that forgets to
    /// check its own clock cannot return a late answer.
    #[test]
    fn a_timeout_is_an_error_and_not_a_partial_command() {
        let engine = SlowEngine;
        let params = GenParams::default().with_timeout(Duration::from_millis(1));
        let err = generate_command(&engine, "hello", &params).expect_err("must not succeed");
        assert!(
            matches!(err, InferenceError::Timeout { .. }),
            "expected a timeout, got {err:?}"
        );
        assert!(
            !err.to_string().contains("echo done"),
            "a timeout must not leak the command the engine would have produced"
        );
    }

    /// An engine that finishes inside the ceiling is not penalised by the thread
    /// the timeout is implemented with.
    #[test]
    fn a_fast_engine_is_not_slowed_by_the_deadline() {
        let engine = FakeEngine::new("ls -la");
        let params = GenParams::default().with_timeout(Duration::from_secs(30));
        let started = std::time::Instant::now();
        let out = generate_command(&engine, "list", &params).expect("generate");
        assert_eq!(out, "ls -la");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "took {:?}",
            started.elapsed()
        );
    }

    /// The timeout error message must not leak a command, and must tell the
    /// user what to do.
    #[test]
    fn the_timeout_message_explains_itself() {
        let e = InferenceError::Timeout {
            limit: Duration::from_secs(30),
        };
        let text = e.to_string();
        assert!(text.contains("30"), "names the limit: {text}");
        assert!(text.contains("partial"), "explains why: {text}");
    }

    #[test]
    fn zero_max_tokens_is_refused() {
        let engine = FakeEngine::new("ls");
        let params = GenParams::default().with_max_tokens(0);
        let err = generate_command(&engine, "list", &params).expect_err("refuse");
        assert!(matches!(err, InferenceError::BadParams(_)));
        assert_eq!(engine.calls(), 0, "must not call a bad engine");
    }

    #[test]
    fn a_zero_timeout_is_refused_before_generation() {
        let engine = FakeEngine::new("ls");
        let params = GenParams::default().with_timeout(Duration::ZERO);
        let err = generate_command(&engine, "list", &params).expect_err("refuse");
        assert!(matches!(err, InferenceError::BadParams(_)));
        assert_eq!(engine.calls(), 0);
    }

    #[test]
    fn an_out_of_range_top_p_is_refused() {
        let params = GenParams {
            top_p: 1.5,
            ..GenParams::default()
        };
        let engine = FakeEngine::new("ls");
        let err = generate_command(&engine, "list", &params).expect_err("refuse");
        assert!(matches!(err, InferenceError::BadParams(_)));
    }

    #[test]
    fn a_runaway_generation_is_refused() {
        let engine = RunawayEngine;
        let err = generate_command(&engine, "go", &GenParams::default()).expect_err("refuse");
        let text = err.to_string();
        assert!(text.contains("runaway") || text.contains("limit"), "{text}");
    }

    #[test]
    fn an_empty_reply_is_an_error_not_an_empty_command() {
        let engine = FakeEngine::new("   ");
        let err = generate_command(&engine, "go", &GenParams::default()).expect_err("refuse");
        assert!(matches!(err, InferenceError::Empty));
    }

    #[test]
    fn a_fence_with_nothing_inside_is_an_error() {
        let engine = FakeEngine::new("```bash\n```");
        let err = generate_command(&engine, "go", &GenParams::default()).expect_err("refuse");
        assert!(matches!(err, InferenceError::Empty));
    }

    // ── the process-wide cache ─────────────────────────────────────────────

    #[test]
    fn installing_an_engine_twice_is_an_error() {
        // Runs against the real static. The first install wins; the second must
        // be refused so a test cannot be poisoned by ordering.
        static E: FakeEngine = FakeEngine {
            reply: String::new(),
            calls: AtomicUsize::new(0),
            params_seen: OnceLock::new(),
            info: EngineInfo {
                model_name: String::new(),
                context_size: 1,
                threads: 1,
                greedy: true,
            },
        };
        let _ = install(&E);
        // A second install, whether the first succeeded or another test won the
        // race, must be refused.
        let second = install(&E);
        assert!(second.is_err(), "a second install must be refused");
    }

    #[test]
    fn require_reports_when_nothing_is_loaded() {
        // Depends on install order, so only assert the shape: either an engine
        // is there and require succeeds, or require explains the absence.
        match require() {
            Ok(_) => {}
            Err(e) => assert!(e.to_string().contains("no model"), "{e}"),
        }
    }
}
