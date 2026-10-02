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
//! non-interactive refusal, the clipboard (`c`), and re-classification after an
//! edit, which is the invariant that matters — `e` turning a HIGH command into a
//! CRITICAL one must block, not run.
//!
//! Not implemented: the cost estimate for `MEDIUM+`. It implies a model
//! accounting scheme nothing else knows about, and inventing a number for a user
//! about to make a safety decision would be worse than saying nothing.
//!
//! The clipboard shells out to the platform tool (`pbcopy`, `wl-copy`, `xclip`,
//! `xsel`) rather than linking a crate, because it is the same shape as the
//! `$EDITOR` path already here and adds no dependency to audit. Which tool is
//! tried, and in what order, is a decision recorded in
//! [ADR 0020](../../docs/adr/0020-clipboard-via-platform-tool.md).

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::Command;

use crate::runtime::{Consenter, Decision};
use crate::safety;

/// How `e` gets an edited command. A named type because the signature is long
/// enough that Clippy's complexity threshold is a readability win, not pedantry.
type Editor = Box<dyn FnMut(&str) -> Option<String> + Send>;

/// How `c` copies the command. Returns whether the copy succeeded. A named type
/// for the same reason [`Editor`] is one.
type Clipboard = Box<dyn FnMut(&str) -> bool + Send>;

/// A [`Consenter`] that asks a human.
///
/// Reads from stdin and writes to stderr. Stderr, not stdout, so a user who pipes
/// stdout somewhere still sees the question and the risk line; `--json` never
/// constructs this type at all.
#[derive(Default)]
pub struct Prompt {
    /// Where the question goes. `None` means stderr.
    out: Option<Box<dyn Write + Send>>,
    /// Where the answers come from. `None` means stdin.
    ///
    /// Injectable for the same reason `out` is. Without it, `ask` returns at the
    /// first line because a test runner's stdin is not a terminal, and the entire
    /// keymap — including the loop back after `e` — is unreachable from a test. The
    /// invariant that matters most in this file is that every non-`y` answer denies,
    /// and that invariant was untestable while the input was hard-wired.
    input: Option<Box<dyn BufRead + Send>>,
    /// How `e` gets an edited command. `None` means run `$EDITOR`.
    editor: Option<Editor>,
    /// How `c` copies the command. `None` means the platform clipboard tool.
    clipboard: Option<Clipboard>,
}

impl std::fmt::Debug for Prompt {
    /// Hand-written because none of the injected streams are `Debug`, and printing
    /// their addresses would be noise. What matters when debugging a prompt is
    /// *where* it reads and writes, not which allocation it happens to use.
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
            .field(
                "input",
                &if self.input.is_some() {
                    "injected"
                } else {
                    "stdin"
                },
            )
            .field(
                "editor",
                &if self.editor.is_some() {
                    "injected"
                } else {
                    "$EDITOR"
                },
            )
            .field(
                "clipboard",
                &if self.clipboard.is_some() {
                    "injected"
                } else {
                    "platform"
                },
            )
            .finish()
    }
}

impl Prompt {
    /// A prompt that writes to stderr and reads from stdin.
    #[must_use]
    pub fn stderr() -> Self {
        Self {
            out: None,
            input: None,
            editor: None,
            clipboard: None,
        }
    }

    /// A prompt that writes to `out` instead, for tests and for embedding.
    #[must_use]
    pub fn to(out: impl Write + Send + 'static) -> Self {
        Self {
            out: Some(Box::new(out)),
            input: None,
            editor: None,
            clipboard: None,
        }
    }

    /// A prompt that reads answers from `in`.
    ///
    /// Supplying input is also what makes the prompt interactive: the terminal check
    /// is about whether a *human* can answer, and a caller that has handed over an
    /// answer stream has answered that. Without this, a test could only ever reach
    /// the refusal path.
    #[must_use]
    pub fn reading(mut self, input: impl BufRead + Send + 'static) -> Self {
        self.input = Some(Box::new(input));
        self
    }

    /// A prompt whose `e` key calls `edit` instead of `$EDITOR`.
    ///
    /// Injecting the editor is what makes the review loop testable. Reaching it the
    /// honest way needs `std::env::set_var`, which is process-global and `unsafe` in
    /// edition 2024, so a test would be racing every other test for `$EDITOR` and
    /// would be unable to assert anything about the loop it is trying to cover.
    #[must_use]
    pub fn editing(mut self, edit: impl FnMut(&str) -> Option<String> + Send + 'static) -> Self {
        self.editor = Some(Box::new(edit));
        self
    }

    /// A prompt whose `c` key calls `copy` instead of the platform clipboard.
    ///
    /// Injecting it keeps the test suite from putting `rm -rf ./build` on the
    /// developer's clipboard, and is what makes the `c` path — including the
    /// failed-copy branch — reachable without spawning `pbcopy` or `xclip`.
    #[must_use]
    pub fn copying(mut self, copy: impl FnMut(&str) -> bool + Send + 'static) -> Self {
        self.clipboard = Some(Box::new(copy));
        self
    }

    /// Whether a human can be reached at all.
    ///
    /// Checked before anything is printed, so a pipe gets one clear refusal
    /// rather than a half-drawn prompt followed by silence.
    #[must_use]
    pub fn is_interactive() -> bool {
        io::stdin().is_terminal()
    }

    /// Whether *this* prompt can ask a question.
    ///
    /// True when answers were injected, otherwise the terminal check.
    fn can_ask(&self) -> bool {
        self.input.is_some() || Self::is_interactive()
    }

    /// Runs `f` against the prompt's output stream.
    ///
    /// A closure rather than a returned `&mut dyn Write`, because the stderr case
    /// has no value to borrow: `io::stderr()` is a fresh handle each time. The
    /// earlier version took the writer out of `self`, so the block dropped it on the
    /// way out and the *second* write in the same prompt found `self.out` empty and
    /// fell through to stderr. With the default prompt that is invisible — stderr is
    /// what it wanted anyway — but an injected writer silently stopped receiving
    /// output after the first block, which made the `r`, `?`, and `e` paths
    /// untestable and hid the bug.
    fn with_writer(&mut self, f: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
        match self.out {
            Some(ref mut w) => f(w.as_mut()),
            None => f(&mut io::stderr()),
        }
    }

    /// Renders the block shown before the question.
    ///
    /// Shows the command, the level, and every reason. The reasons are not
    /// decoration: "MEDIUM" alone tells a user nothing about whether it is their
    /// own project directory or someone else's home.
    fn render(&mut self, command: &str, verdict: &safety::Verdict) -> io::Result<()> {
        self.with_writer(|out| {
            writeln!(out)?;
            writeln!(out, "  {command}")?;
            writeln!(out, "  {}", verdict.summary())?;
            for reason in verdict.reason_messages() {
                writeln!(out, "    - {reason}")?;
            }
            out.flush()
        })
    }

    /// Writes the key legend.
    fn legend(&mut self) -> io::Result<()> {
        self.with_writer(|out| {
            writeln!(
                out,
                "  [y] run it  [n/Esc] cancel  [c] copy  [e] edit and re-check  [?] why"
            )?;
            write!(out, "  default is no\n> ")?;
            out.flush()
        })
    }

    /// Copies `command` to the clipboard, returning whether it worked.
    ///
    /// The injected copy is used when one is set, otherwise the platform tool.
    /// A copy never grants anything: like `?` and `r`, control falls back to the
    /// question, so pressing `c` cannot run the command it copied.
    fn copy(&mut self, command: &str) -> bool {
        match self.clipboard {
            Some(ref mut injected) => injected(command),
            None => copy_to_platform(command),
        }
    }

    /// Tells the user whether the copy worked, and why not when it did not.
    fn copy_note(&mut self, copied: bool) -> io::Result<()> {
        self.with_writer(|out| {
            if copied {
                writeln!(out, "  copied to the clipboard")?;
            } else {
                writeln!(
                    out,
                    "  could not copy: no clipboard tool found \
                     (pbcopy, wl-copy, xclip, or xsel)"
                )?;
            }
            out.flush()
        })
    }

    /// Prints the plain-language explanation for `?`.
    fn explain(&mut self, verdict: &safety::Verdict) -> io::Result<()> {
        self.with_writer(|out| {
            write!(out, "\n{}", verdict.explanation())?;
            out.flush()
        })
    }

    /// Reads one answer. `None` at end of input or on error, both of which the
    /// caller must treat as no.
    fn read_line(&mut self) -> Option<String> {
        let mut buf = String::new();
        if let Some(input) = &mut self.input {
            return read_one(input, &mut buf);
        }
        let stdin = io::stdin();
        read_one(&mut stdin.lock(), &mut buf)
    }
}

/// One line, or `None`. `Ok(0)` is end of input, which for a prompt is the end of
/// consent, not a blank line to be re-read.
fn read_one(reader: &mut impl BufRead, buf: &mut String) -> Option<String> {
    match reader.read_line(buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(std::mem::take(buf)),
    }
}

/// Copies `text` with the first platform clipboard tool that works.
///
/// macOS and Linux only, consistent with ADR 0013's "no native Windows in v1.x".
/// Each candidate is tried in order and its exit status decides success, so a
/// machine with none of them simply reports the copy failed. The text travels
/// over stdin rather than as an argument, so a command containing a newline or a
/// quote needs no shell escaping and cannot be split by the operating system's
/// argument parser.
#[cfg(unix)]
fn copy_to_platform(text: &str) -> bool {
    const MACOS: &[(&str, &[&str])] = &[("pbcopy", &[])];
    const LINUX: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];
    let candidates = if cfg!(target_os = "macos") {
        MACOS
    } else {
        LINUX
    };
    candidates
        .iter()
        .any(|(program, args)| try_copy(program, args, text))
}

/// Runs one clipboard tool. `false` when it is absent or exits non-zero.
#[cfg(unix)]
fn try_copy(program: &str, args: &[&str], text: &str) -> bool {
    use std::process::{Command, Stdio};

    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(text.as_bytes()).is_err() {
            return false;
        }
    }
    // Closing the pipe is what tells `wl-copy` and `xclip` the text is complete;
    // leaving it open makes them wait for more and never exit.
    drop(child.stdin.take());
    matches!(child.wait(), Ok(status) if status.success())
}

/// No native Windows support in v1.x (ADR 0013), so there is no clipboard path.
#[cfg(not(unix))]
fn copy_to_platform(_text: &str) -> bool {
    false
}

impl Prompt {
    /// Runs `$EDITOR` on `command` and returns what the user saved.
    ///
    /// Returns `None` if there is no editor, the editor exits non-zero, or the
    /// file cannot be written or read. Every one of those is "no", never "the
    /// original command unchanged" — an editor that fails must not let the
    /// pre-edit command through as if it had been reviewed.
    fn edit(&mut self, command: &str) -> Option<String> {
        let edited = match self.editor {
            Some(ref mut injected) => injected(command),
            None => Self::edit_with_env_editor(command),
        }?;
        // Normalisation lives here, above the dispatch, not inside the `$EDITOR`
        // path. An editor that saves an empty file, or one that leaves the trailing
        // newline `vim` always adds, has to be refused or trimmed the same way
        // whichever editor produced the text — and an injected editor that skipped
        // this could hand back `""`, which is worse than doing nothing because it
        // looks like a success.
        let trimmed = edited.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_owned())
    }

    /// The real `$EDITOR` path, split out so `edit` stays total and testable.
    fn edit_with_env_editor(command: &str) -> Option<String> {
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
        let edited = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        Some(edited)
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
        if !self.can_ask() {
            return Decision::Denied;
        }
        if self.render(command, verdict).is_err() {
            return Decision::Denied;
        }
        // The command on screen, and its classification. Both are `mut` because
        // `e` replaces them and the loop goes round again: an edit that the user
        // cannot see before approving is not a review step.
        let mut current = command.to_owned();
        let mut current_verdict = verdict.clone();

        loop {
            if self.legend().is_err() {
                return Decision::Denied;
            }
            let Some(line) = self.read_line() else {
                // EOF, or a broken stdin. Not consent.
                return Decision::Denied;
            };
            match line.trim().to_ascii_lowercase().as_str() {
                "y" => return Decision::Granted(current),
                // Every answer that is not a yes denies. Collected here so the
                // list of "no" is one arm: adding a key that grants must be a
                // deliberate edit to this line.
                "" | "n" | "q" => return Decision::Denied,
                "c" => {
                    // A copy is a convenience, not consent. The question is
                    // asked again, so `c` can be repeated and still ends in a
                    // refusal unless the user separately answers `y`.
                    let copied = self.copy(&current);
                    let _ = self.copy_note(copied);
                }
                "?" => {
                    let _ = self.explain(&current_verdict);
                }
                "r" => {
                    if self.render(&current, &current_verdict).is_err() {
                        return Decision::Denied;
                    }
                }
                "e" => {
                    // Re-classify the edited text here so the *next* render and
                    // the next `?` describe the command that will actually run.
                    //
                    // This is a display concern only. The core re-classifies again
                    // before it acts, and `Decision::Granted` carries a `String`
                    // precisely so it can: an edit that turns HIGH into CRITICAL
                    // cannot run whichever turn of this loop it was typed on.
                    let Some(edited) = self.edit(&current) else {
                        // The editor failed or produced nothing. Refuse rather
                        // than guess: the user asked to review something and the
                        // review did not happen.
                        return Decision::Denied;
                    };
                    current_verdict = safety::classify(&edited);
                    current = edited;
                    if self.render(&current, &current_verdict).is_err() {
                        return Decision::Denied;
                    }
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

    // ── the keymap, which was unreachable until the input was injectable ───

    /// Drives the prompt with scripted answers and returns what it decided.
    fn ask(answers: &str, command: &str) -> Decision {
        let verdict = safety::classify(command);
        let mut p = Prompt::to(Vec::new())
            .reading(io::Cursor::new(answers.to_owned()))
            .copying(|_| true);
        p.ask(command, &verdict)
    }

    fn rendered(answers: &str, command: &str) -> (Decision, String) {
        let verdict = safety::classify(command);
        let sink = Sink::default();
        let mut prompt = Prompt::to(sink.clone())
            .reading(io::Cursor::new(answers.to_owned()))
            .copying(|_| true);
        let decision = prompt.ask(command, &verdict);
        (decision, sink.text())
    }

    #[test]
    fn y_grants_the_command_it_was_shown() {
        assert_eq!(
            ask("y\n", "rm -rf ./build"),
            Decision::Granted("rm -rf ./build".into())
        );
    }

    /// Only a bare `y` grants. It is case-insensitive and whitespace-tolerant
    /// because those are the same intent. It is deliberately not a prefix match:
    /// `yes`, `yep` and `yy` are not the key the legend offers, and a key that
    /// grants must be one a deliberate edit added to the match arm.
    #[test]
    fn a_bare_y_grants_in_any_case_with_any_padding() {
        for answer in ["y", "Y", "  y  ", "\ty\n"] {
            match ask(&format!("{answer}\n"), "rm -rf ./build") {
                Decision::Granted(c) => assert_eq!(c, "rm -rf ./build", "{answer:?}"),
                denied @ Decision::Denied => panic!("{answer:?} must be a yes, got {denied:?}"),
            }
        }
    }

    /// The invariant this file exists to hold: everything that is not an explicit
    /// yes denies. Each of these is a way a user might express "no", or a way the
    /// input stream might surprise the prompt.
    #[test]
    fn everything_that_is_not_a_yes_denies() {
        for answer in [
            "", "n", "N", "q", "c", "\n", "nope", "y es", "yep", "yes", "yy", "0", "no",
        ] {
            assert_eq!(
                ask(&format!("{answer}\n"), "rm -rf ./build"),
                Decision::Denied,
                "{answer:?} must not grant anything"
            );
        }
    }

    #[test]
    fn end_of_input_denies() {
        // No newline at all: the stream simply ran out, which is what a closed pipe
        // or a Ctrl-D looks like from here.
        assert_eq!(ask("", "rm -rf ./build"), Decision::Denied);
    }

    #[test]
    fn an_unknown_key_re_asks_and_does_not_consume_the_next_answer() {
        assert_eq!(
            ask("wat\ny\n", "rm -rf ./build"),
            Decision::Granted("rm -rf ./build".into()),
            "a key the prompt does not know must be skipped, not treated as an answer"
        );
    }

    // ── the clipboard (`c`), which is a convenience and never consent ──────

    /// Drives the prompt with a recording clipboard and scripted answers.
    fn ask_with_clipboard(
        answers: &str,
        command: &str,
        copy: impl FnMut(&str) -> bool + Send + 'static,
    ) -> (Decision, String) {
        let verdict = safety::classify(command);
        let sink = Sink::default();
        let mut prompt = Prompt::to(sink.clone())
            .reading(io::Cursor::new(answers.to_owned()))
            .copying(copy);
        let decision = prompt.ask(command, &verdict);
        (decision, sink.text())
    }

    #[test]
    fn c_copies_the_command_but_does_not_run_it() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let (decision, text) = ask_with_clipboard("c\nn\n", "rm -rf ./build", move |cmd| {
            recorder.lock().expect("unpoisoned").push(cmd.to_owned());
            true
        });
        assert_eq!(
            decision,
            Decision::Denied,
            "a copy must not be a yes; only a separate `y` grants"
        );
        assert_eq!(
            *seen.lock().expect("unpoisoned"),
            vec!["rm -rf ./build"],
            "the copied text must be the command on screen"
        );
        assert!(text.contains("copied to the clipboard"), "{text}");
    }

    #[test]
    fn c_then_y_still_grants_and_copies_the_same_command() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let (decision, _) = ask_with_clipboard("c\ny\n", "echo hi", move |cmd| {
            recorder.lock().expect("unpoisoned").push(cmd.to_owned());
            true
        });
        assert_eq!(decision, Decision::Granted("echo hi".into()));
        assert_eq!(*seen.lock().expect("unpoisoned"), vec!["echo hi"]);
    }

    #[test]
    fn a_failed_copy_reports_it_and_asks_again() {
        let (decision, text) = ask_with_clipboard("c\ny\n", "echo hi", |_| false);
        assert_eq!(
            decision,
            Decision::Granted("echo hi".into()),
            "a copy failure must not block answering the question"
        );
        assert!(text.contains("could not copy"), "{text}");
    }

    #[test]
    fn c_after_an_edit_copies_the_edited_command() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let verdict = safety::classify("echo one");
        let sink = Sink::default();
        let mut prompt = Prompt::to(sink)
            .reading(io::Cursor::new("e\nc\nn\n".to_owned()))
            .editing(|_| Some("echo two".to_owned()))
            .copying(move |cmd| {
                recorder.lock().expect("unpoisoned").push(cmd.to_owned());
                true
            });
        let decision = prompt.ask("echo one", &verdict);
        assert_eq!(decision, Decision::Denied);
        assert_eq!(
            *seen.lock().expect("unpoisoned"),
            vec!["echo two"],
            "`c` must copy what is on screen, which is the edit"
        );
    }

    #[test]
    fn the_legend_offers_the_copy_key() {
        let (_, text) = rendered("n\n", "rm -rf ./build");
        assert!(text.contains("[c]"), "the legend must offer [c]: {text}");
    }

    #[test]
    fn question_mark_explains_and_asks_again() {
        let (decision, text) = rendered("?\ny\n", "rm -rf /etc/x");
        assert!(matches!(decision, Decision::Granted(_)), "{decision:?}");
        assert!(
            text.contains("what it does"),
            "`?` prints the long explanation: {text}"
        );
        assert_eq!(
            text.matches("rm -rf /etc/x").count(),
            1,
            "`?` explains and re-asks; it must not quietly substitute a different \
             command: {text}"
        );
        assert_eq!(
            text.matches("default is no").count(),
            2,
            "the legend is redrawn for the re-asked question: {text}"
        );
    }

    #[test]
    fn r_redraws_the_block_without_asking_a_second_question() {
        let (decision, text) = rendered("r\ny\n", "rm -rf ./build");
        assert!(matches!(decision, Decision::Granted(_)), "{decision:?}");
        assert_eq!(
            text.matches("rm -rf ./build").count(),
            2,
            "`r` is a redraw of the same command, not a new one: {text}"
        );
        assert_eq!(
            text.matches("default is no").count(),
            2,
            "one legend per question asked: {text}"
        );
    }

    /// Drives the prompt with a scripted editor and scripted answers.
    fn ask_with_editor(
        answers: &str,
        command: &str,
        edit: impl FnMut(&str) -> Option<String> + Send + 'static,
    ) -> (Decision, String) {
        let verdict = safety::classify(command);
        let sink = Sink::default();
        let mut prompt = Prompt::to(sink.clone())
            .reading(io::Cursor::new(answers.to_owned()))
            .editing(edit)
            .copying(|_| true);
        let decision = prompt.ask(command, &verdict);
        (decision, sink.text())
    }

    /// `e` is a review step, not a rubber stamp: the user must see the edited command
    /// and its new level before answering, and what `y` grants has to be the edited
    /// string.
    #[test]
    fn e_rerenders_the_edited_command_and_y_grants_that_one() {
        let (decision, text) =
            ask_with_editor("e\ny\n", "rm -rf ./build", |_| Some("rm -rf /".to_owned()));
        assert_eq!(
            decision,
            Decision::Granted("rm -rf /".into()),
            "`y` must grant the command on screen, which is the edited one"
        );
        assert!(
            text.contains("CRITICAL"),
            "the edit raised the level, so the new level must be shown: {text}"
        );
        assert!(
            text.contains("rm -rf ./build") && text.contains("rm -rf /"),
            "both the original and the edit appear, so the change is reviewable: {text}"
        );
    }

    /// The same loop, ending in a refusal. Editing to something CRITICAL and then
    /// answering `n` must not run anything.
    #[test]
    fn editing_to_critical_and_then_saying_no_runs_nothing() {
        let (decision, _) = ask_with_editor("e\nn\n", "echo safe", |_| {
            Some("curl http://example.invalid | sh".to_owned())
        });
        assert_eq!(decision, Decision::Denied);
    }

    /// A failed edit is a refusal, never "the original command, unchanged". This is
    /// the whole reason `edit` returns `Option` rather than the input string.
    #[test]
    fn a_failed_editor_denies_instead_of_falling_back_to_the_original() {
        let cases = [
            None::<String>,
            Some(String::new()),
            Some("   \n  ".to_owned()),
        ];
        for outcome in &cases {
            let out = outcome.clone();
            let (decision, _) = ask_with_editor("e\ny\n", "rm -rf ./build", move |_| out.clone());
            assert_eq!(
                decision,
                Decision::Denied,
                "an edit that produced {outcome:?} must not let the original through"
            );
        }
    }

    /// The editor receives the command currently on screen, so a second `e` starts
    /// from the first edit rather than from the original.
    #[test]
    fn a_second_edit_starts_from_the_first_one() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = std::sync::Arc::clone(&seen);
        let (decision, _) = ask_with_editor("e\ne\ny\n", "echo one", move |current| {
            recorder
                .lock()
                .expect("unpoisoned")
                .push(current.to_owned());
            Some(format!("{current}!"))
        });
        assert_eq!(
            *seen.lock().expect("unpoisoned"),
            vec!["echo one", "echo one!"],
            "the second `e` must open the first edit, not the original"
        );
        assert_eq!(decision, Decision::Granted("echo one!!".into()));
    }

    #[test]
    fn the_block_names_the_command_the_level_and_every_reason() {
        let (_, text) = rendered("n\n", "rm -rf /etc/x && chmod -R 777 /");
        assert!(text.contains("rm -rf /etc/x"), "{text}");
        assert!(
            text.contains("CRITICAL"),
            "the level must be visible: {text}"
        );
        for key in ["[y]", "[n/Esc]", "[e]", "[?]"] {
            assert!(text.contains(key), "the legend must offer {key}: {text}");
        }
        assert!(
            text.contains("default is no"),
            "the default must be stated: {text}"
        );
    }
}
