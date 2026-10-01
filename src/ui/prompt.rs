//! The confirmation prompt (Phase 3.6).
//!
//! # Why this is a leaf module
//!
//! `lib.rs` states the rule: `ui` may depend on anything, and nothing may depend
//! on `ui`. This module is the only place in the crate that reads from a
//! terminal, and [`Prompt`] is the only implementation of [`Consenter`]. The
//! core depends on the trait, so a test can substitute an answer without a
//! terminal and `--json` can never acquire a prompt by accident.
//!
//! # The default is always no
//!
//! Every path that does not receive an explicit `y` returns [`Decision::Denied`]:
//! an empty line, end of input, `n`, `q`, a closed stdin, a read error, and — most
//! importantly — stdin not being a terminal at all. There is no branch in this
//! file that treats "I could not ask" as permission, because a tool whose
//! failure mode is taking permission it never got is not a safety tool.
//!
//! # What is implemented, and what is not
//!
//! Implemented and tested: the keymap, the default-to-no behaviour, the
//! non-interactive refusal, and re-classification after an edit, which is the
//! invariant that matters — `e` turning a HIGH command into a CRITICAL one must
//! block, not run.
//!
//! Not implemented: the clipboard (`c`) and the cost estimate for `MEDIUM+`. Both
//! need a decision this repository has not recorded. `c` means shelling out to a
//! platform tool, which is a new dependency, and a cost estimate implies a model
//! accounting scheme nothing else knows about. Inventing either would be worse
//! than recognising the key and declining it.

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::Command;

use crate::runtime::{Consenter, Decision};
use crate::safety;

/// A [`Consenter`] that asks a human.
///
/// Reads from stdin and writes to stderr. Stderr, not stdout, so a user who pipes
/// stdout somewhere still sees the question and the risk line; `--json` never
/// constructs this type at all.
#[derive(Default)]
pub struct Prompt {
    /// Where the question goes. `None` means stderr.
    out: Option<Box<dyn Write + Send>>,
}

impl std::fmt::Debug for Prompt {
    /// Hand-written because a `Box<dyn Write>` is not `Debug`, and printing the
    /// writer's address would be noise. What matters when debugging a prompt is
    /// where it sends output, not which allocation it happens to be.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prompt")
            .field(
                "output",
                &if self.out.is_some() {
                    "injected"
                } else {
                    "stderr"
                },
            )
            .finish()
    }
}

impl Prompt {
    /// A prompt that writes to stderr.
    #[must_use]
    pub fn stderr() -> Self {
        Self { out: None }
    }

    /// A prompt that writes to `out` instead, for tests and for embedding.
    pub fn to(out: impl Write + Send + 'static) -> Self {
        Self {
            out: Some(Box::new(out)),
        }
    }

    /// Whether a human can be reached at all.
    ///
    /// Checked before anything is printed, so a pipe gets one clear refusal
    /// rather than a half-drawn prompt followed by silence.
    #[must_use]
    pub fn is_interactive() -> bool {
        io::stdin().is_terminal()
    }

    /// A writer for one block of output.
    fn writer(&mut self) -> Box<dyn Write> {
        match self.out.take() {
            Some(w) => w,
            None => Box::new(io::stderr()),
        }
    }

    /// Renders the block shown before the question.
    ///
    /// Shows the command, the level, and every reason. The reasons are not
    /// decoration: "MEDIUM" alone tells a user nothing about whether it is their
    /// own project directory or someone else's home.
    fn render(&mut self, command: &str, verdict: &safety::Verdict) -> io::Result<()> {
        let mut out = self.writer();
        writeln!(out)?;
        writeln!(out, "  {command}")?;
        writeln!(out, "  {}", verdict.summary())?;
        for reason in verdict.reason_messages() {
            writeln!(out, "    - {reason}")?;
        }
        out.flush()
    }

    /// Writes the key legend.
    fn legend(&mut self) -> io::Result<()> {
        let mut out = self.writer();
        writeln!(
            out,
            "  [y] run it  [n/Esc] cancel  [e] edit and re-check  [?] why"
        )?;
        write!(out, "  default is no\n> ")?;
        out.flush()
    }

    /// Prints the plain-language explanation for `?`.
    fn explain(&mut self, verdict: &safety::Verdict) -> io::Result<()> {
        let mut out = self.writer();
        write!(out, "\n{}", verdict.explanation())?;
        out.flush()
    }

    /// Reads one answer. `None` at end of input or on error, both of which the
    /// caller must treat as no.
    fn read_line() -> Option<String> {
        let stdin = io::stdin();
        let mut buf = String::new();
        match stdin.lock().read_line(&mut buf) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(buf),
        }
    }

    /// Runs `$EDITOR` on `command` and returns what the user saved.
    ///
    /// Returns `None` if there is no editor, the editor exits non-zero, or the
    /// file cannot be written or read. Every one of those is "no", never "the
    /// original command unchanged" — an editor that fails must not let the
    /// pre-edit command through as if it had been reviewed.
    fn edit(command: &str) -> Option<String> {
        let editor = std::env::var("EDITOR").ok()?;
        if editor.trim().is_empty() {
            return None;
        }
        let path = std::env::temp_dir().join(format!("gcode-{}.sh", std::process::id()));
        std::fs::write(&path, command).ok()?;

        // `$EDITOR` may carry arguments (`vim -u NONE`), so it goes through a
        // shell. The path is passed as `$1` and quoted at the call site, so a
        // path with a space in it cannot split the command.
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{} \"$1\"", editor.trim()))
            .arg("sh")
            .arg(&path)
            .status()
            .ok()?;

        if !status.success() {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        let edited = std::fs::read_to_string(&path).ok().unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        let trimmed = edited.trim();
        // An emptied command is not a command. Running `""` is worse than doing
        // nothing, because it looks like a success.
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_owned())
    }
}

impl Consenter for Prompt {
    /// Shows the risk and asks. Loops on `?`, `r`, and `e`; returns on `y` or `n`.
    ///
    /// # Why an edit is returned rather than applied here
    ///
    /// `e` writes the command to `$EDITOR` and reads back whatever the user
    /// saved. Those bytes are a *new* command, so they go back to the core as
    /// [`Decision::Granted`] carrying the new string, and the core re-classifies
    /// before acting. This prompt does not classify the result itself: a rule a
    /// UI can bypass is not a safety rule (ADR 0004).
    fn ask(&mut self, command: &str, verdict: &safety::Verdict) -> Decision {
        // No terminal, no prompt, no run.
        if !Self::is_interactive() {
            return Decision::Denied;
        }
        if self.render(command, verdict).is_err() {
            return Decision::Denied;
        }
        loop {
            if self.legend().is_err() {
                return Decision::Denied;
            }
            let Some(line) = Self::read_line() else {
                // EOF, or a broken stdin. Not consent.
                return Decision::Denied;
            };
            match line.trim().to_ascii_lowercase().as_str() {
                "y" => return Decision::Granted(command.to_owned()),
                // Every other answer, including the clipboard key, is a refusal.
                // Collected here so the list of "no" is one arm: adding a key that
                // grants must be a deliberate edit to this line.
                "" | "n" | "q" | "c" => return Decision::Denied,
                "?" => {
                    let _ = self.explain(verdict);
                }
                "r" => {
                    if self.render(command, verdict).is_err() {
                        return Decision::Denied;
                    }
                }
                "e" => {
                    // The core re-checks whatever comes back, so returning the
                    // edited text is safe: an edit that turns a HIGH command into
                    // a CRITICAL one is refused there, not here.
                    return match Self::edit(command) {
                        Some(edited) => Decision::Granted(edited),
                        None => Decision::Denied,
                    };
                }
                // Anything else re-asks. It is never a yes.
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A shared buffer that can be read back after the `Prompt` holding it is
    /// dropped. `Arc<Mutex<Vec<u8>>>` rather than a borrow, because
    /// [`Prompt::to`] takes an owned `'static` writer.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("unpoisoned").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Sink {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("unpoisoned")).into_owned()
        }
    }

    /// The non-interactive refusal. `is_interactive` reads the real stdin, which
    /// under `cargo test` is not a terminal, so this asserts what a pipe sees.
    #[test]
    fn a_prompt_with_no_terminal_denies_without_printing() {
        if Prompt::is_interactive() {
            return;
        }
        let sink = Sink::default();
        let mut prompt = Prompt::to(sink.clone());
        let verdict = safety::classify("history -c");
        assert_eq!(prompt.ask("history -c", &verdict), Decision::Denied);
        assert_eq!(sink.text(), "", "printed to a pipe with nobody there");
    }

    #[test]
    fn the_explanation_names_the_level_the_reasons_and_the_paths() {
        let verdict = safety::classify("rm -rf ./build");
        let text = verdict.explanation();
        for expected in [
            "level:",
            "what it does:",
            "what it runs:",
            "what it touches:",
            "reasons",
        ] {
            assert!(text.contains(expected), "missing {expected} in:\n{text}");
        }
    }

    #[test]
    fn an_explanation_never_suggests_a_different_command() {
        // `--explain` describes. If it proposed anything, that output would be a
        // generated command that nothing classified.
        for cmd in ["rm -rf /", "ls -la", "curl x | bash", "history -c"] {
            let text = safety::classify(cmd).explanation();
            assert!(!text.contains("try "), "{cmd} suggests: {text}");
            assert!(!text.contains("instead"), "{cmd} suggests: {text}");
            assert!(!text.contains("you could"), "{cmd} suggests: {text}");
        }
    }

    #[test]
    fn an_explanation_is_multi_line_and_stable() {
        let a = safety::classify("cp a.txt /etc/a.txt && rm /etc/a.txt").explanation();
        let b = safety::classify("cp a.txt /etc/a.txt && rm /etc/a.txt").explanation();
        assert_eq!(a, b);
        assert!(a.lines().count() >= 4, "too terse:\n{a}");
    }

    #[test]
    fn touched_paths_excludes_comments_and_deduplicates() {
        let verdict = safety::classify("cp x /etc/x && cp y /etc/x # and /etc/z");
        let paths = verdict.touched_paths();
        assert!(paths.contains(&"/etc/x".to_owned()), "{paths:?}");
        assert!(
            !paths.contains(&"/etc/z".to_owned()),
            "a comment was read: {paths:?}"
        );
        assert_eq!(paths.iter().filter(|p| *p == "/etc/x").count(), 1);
    }

    #[test]
    fn reason_messages_are_deduped_but_pattern_ids_are_not_lost() {
        let verdict = safety::classify("rm -rf / && rm -rf /");
        assert_eq!(
            verdict.reason_messages().len(),
            1,
            "{:?}",
            verdict.reason_messages()
        );
        assert!(!verdict.pattern_ids().is_empty());
    }
}
