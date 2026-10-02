//! Tests for `shell/gcode.bash`, run in a real bash.
//!
//! # Why a real shell and not a mock
//!
//! Everything this hook gets wrong is about bash itself: whether `$?` survives,
//! whether an existing `PROMPT_COMMAND` is still there afterwards, whether the
//! array form works, whether `set -u` breaks it. A mock of bash is a mock of
//! nothing, and would pass against a hook that is completely broken. So every test
//! here starts a real `bash` and asks it what happened.
//!
//! # What is not tested, and why
//!
//! - Command output capture. The hook does not capture output, by design; see the
//!   header. There is no test asserting it does, because it does not.
//! - Concurrent prompts, and behaviour under `set -e` in an interactive shell.
//!   `set -e` does not apply to `PROMPT_COMMAND` in the versions tested, so there
//!   is nothing to assert.
//! - Windows and fish. Out of scope; ADR 0013 and roadmap 8.
//!
//! Every test writes to a temporary `GCODE_HISTORY_FILE`, so nothing here touches a
//! real `~/.gcode`. No test reads the developer's `~/.bashrc`, and no test sources
//! the user's own shell configuration.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether a usable `bash` exists, and which one. `/bin/bash` on macOS is 3.2, which
/// is the oldest shell supported and the one where the bugs are, so it is the
/// preferred interpreter; a newer one is used if that is all there is.
fn bash() -> Option<PathBuf> {
    for candidate in ["/bin/bash", "/usr/local/bin/bash", "/opt/homebrew/bin/bash"] {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    None
}

fn hook_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/gcode.bash")
}

/// A temporary directory that removes itself.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("gcode-hook-{}-{label}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    fn history(&self) -> PathBuf {
        self.0.join("store/history.jsonl")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run a script in an interactive-ish bash with the hook sourced.
///
/// `-i` matters: `history` is empty in a non-interactive shell, and the hook reads
/// `history 1`. `--norc` keeps the developer's own `~/.bashrc` out of it.
///
/// `PS1` is set to something short and `history` is enabled explicitly, because
/// `set -o history` is off by default in a non-tty bash and the hook would
/// otherwise record nothing and every test would pass vacuously.
fn run(script: &str, scratch: &Scratch) -> String {
    let Some(bash) = bash() else {
        return String::new();
    };
    let prelude = format!(
        "PS1='$ '; set -o history\n\
         unset HISTFILE\n\
         export GCODE_HISTORY_FILE='{}'\n\
         source '{}'\n",
        scratch.history().display(),
        hook_path().display()
    );
    let out = Command::new(bash)
        .arg("--norc")
        .arg("-i")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            // The final prompt is suppressed by clearing PROMPT_COMMAND before
            // exiting. bash runs PROMPT_COMMAND once more on the way out, so
            // without this every command is recorded twice and every count
            // assertion in this file is off by exactly one.
            child
                .stdin
                .as_mut()
                .ok_or_else(|| std::io::Error::other("no stdin"))?
                .write_all(format!("{prelude}{script}\nPROMPT_COMMAND=\nexit\n").as_bytes())?;
            child.wait_with_output()
        });
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(e) => panic!("could not run bash: {e}"),
    }
}

/// The history lines written by `script`, one string per line.
///
/// The prelude's own `source` line is filtered out as a backstop. The hook now
/// skips it itself, and `the_hooks_own_source_line_is_never_recorded` proves that
/// directly; this filter remains so a future regression there shows up as one
/// failing test rather than as every count assertion in the file shifting by one.
/// Tests that care about which commands get recorded assert on the remaining lines.
fn lines(scratch: &Scratch) -> Vec<String> {
    std::fs::read_to_string(scratch.history())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.contains("gcode.bash"))
        .map(str::to_owned)
        .collect()
}

// ── the three guarantees that matter ───────────────────────────────────────

/// ADR 0007 rule 5, and the roadmap's "preserves the user's `$?`". This is the
/// test the ADR calls the most important line in the file.
#[test]
fn a_failing_command_is_recorded_with_its_exit_code() {
    let scratch = Scratch::new("exit-code");
    if bash().is_none() {
        eprintln!("skipped: no bash");
        return;
    }
    run("false\n", &scratch);

    let recorded = lines(&scratch);
    assert_eq!(recorded.len(), 1, "one command, one line: {recorded:?}");
    assert!(
        recorded[0].contains(r#""exit":1"#),
        "`false` exits 1: {}",
        recorded[0]
    );
}

/// The user still sees their own `$?`. The hook has to restore it, not merely
/// capture it: a user's `PROMPT_COMMAND` and their prompt both run after us.
#[test]
fn the_users_exit_status_survives_the_hook() {
    let scratch = Scratch::new("dollar-question");
    if bash().is_none() {
        return;
    }
    let out = run("(exit 42)\necho \"USER_SEES=$?\"\n", &scratch);
    assert!(
        out.contains("USER_SEES=42"),
        "the hook ate the exit status: {out}"
    );
}

/// ADR 0007 rule 1: chain onto whatever is there. A user's `PROMPT_COMMAND` that
/// runs after the hook must still run, and must see the real `$?`.
///
/// The script sets `PROMPT_COMMAND` *after* the prelude has already sourced the
/// hook, so the user's entry replaces ours — which is the opposite of chaining and
/// would make this test pass against a hook that clobbered the user's config. The
/// version below fixes that: it declares a function, points `PROMPT_COMMAND` at it,
/// and re-sources the hook so the chain is built over something already there.
#[test]
fn an_existing_prompt_command_still_runs_and_still_sees_the_status() {
    let scratch = Scratch::new("chain");
    if bash().is_none() {
        return;
    }
    let out = run(
        &format!(
            "user_pc() {{ echo \"USER_PC_SEES=$?\"; }}\n\
             PROMPT_COMMAND=user_pc\n\
             unset _GCODE_HOOK_LOADED\n\
             source '{0}'\n\
             echo \"CHAINED=[{1}]\"\n\
             (exit 7)\n",
            hook_path().display(),
            "$PROMPT_COMMAND"
        ),
        &scratch,
    );
    assert!(
        out.contains("USER_PC_SEES=7"),
        "the pre-existing PROMPT_COMMAND did not survive, or lost `$?`: {out}"
    );
    assert!(
        out.contains("CHAINED=[_gcode_capture"),
        "the hook was not installed into the existing PROMPT_COMMAND: {out}"
    );
    assert!(
        out.contains("user_pc"),
        "the user's own PROMPT_COMMAND entry was dropped: {out}"
    );
}

/// The failure this caught: appending the hook meant the capture ran *after* the
/// user's hook, so it recorded whatever that hook returned instead of what the
/// command did. A `PROMPT_COMMAND` that returns 7 made every command in the store
/// read `"exit":7` — a worse failure than recording nothing, because `--fix` would
/// learn that every command fails for one unrelated reason.
#[test]
fn a_prompt_command_that_changes_the_status_does_not_corrupt_the_recorded_exit() {
    let scratch = Scratch::new("status-clobber");
    if bash().is_none() {
        return;
    }
    run(
        &format!(
            "user_pc() {{ return 7; }}\n\
             PROMPT_COMMAND=user_pc\n\
             unset _GCODE_HOOK_LOADED\n\
             source '{}'\n\
             (exit 42)\n\
             sleep 0.05\n",
            hook_path().display()
        ),
        &scratch,
    );
    let recorded = lines(&scratch);
    let entry = recorded
        .iter()
        .find(|l| l.contains("(exit 42)"))
        .unwrap_or_else(|| panic!("no entry for the failing command: {recorded:?}"));
    assert!(
        entry.contains(r#""exit":42"#),
        "the user's PROMPT_COMMAND overwrote the real exit status: {entry}"
    );
    assert!(
        !recorded.iter().any(|l| l.contains(r#""exit":7"#)),
        "the user hook's own status leaked into the store: {recorded:?}"
    );
}

/// Chain twice. Sourcing twice must not record twice, or a user with the hook in
/// two files fills their history with duplicates of every command.
#[test]
fn sourcing_the_hook_twice_records_once() {
    let scratch = Scratch::new("twice");
    if bash().is_none() {
        return;
    }
    run(
        &format!(
            "source '{0}'\nsource '{0}'\nls -la\n",
            hook_path().display()
        ),
        &scratch,
    );
    let recorded = lines(&scratch);
    let matching: Vec<&String> = recorded.iter().filter(|l| l.contains("ls -la")).collect();
    assert_eq!(
        matching.len(),
        1,
        "the hook is not idempotent across two sources: {matching:?}"
    );
}

/// A user with two gcode blocks in their config is the same problem from the other
/// direction: the guard is what stops it, and this checks the guard directly.
#[test]
fn the_load_guard_is_set_after_sourcing() {
    let scratch = Scratch::new("guard");
    if bash().is_none() {
        return;
    }
    let out = run(
        &format!(
            "source '{}'\necho \"LOADED=[${{_GCODE_HOOK_LOADED:-unset}}]\"\n",
            hook_path().display()
        ),
        &scratch,
    );
    assert!(
        out.contains("LOADED=[1]"),
        "the double-source guard is not set: {out}"
    );
}

// ── what it records ────────────────────────────────────────────────────────

#[test]
fn the_command_directory_and_timestamp_are_recorded() {
    let scratch = Scratch::new("fields");
    if bash().is_none() {
        return;
    }
    run("ls -la\n", &scratch);
    let recorded = lines(&scratch);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    let line = &recorded[0];
    assert!(line.contains(r#""cmd":"ls -la""#), "{line}");
    assert!(line.contains(r#""exit":0"#), "{line}");
    assert!(
        line.contains(r#""out":"""#),
        "output is deliberately empty: {line}"
    );
    assert!(line.contains(r#""cwd":"#), "{line}");
    // The timestamp has to be a plausible unix time, not zero and not a literal.
    // `ts` is a JSON number, so the split takes everything up to the comma.
    let ts: i64 = line
        .split(r#""ts":"#)
        .nth(1)
        .and_then(|s| s.split([',', '}']).next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no parseable ts in {line}"));
    // The test asserts the timestamp is real, not a placeholder. A hook that wrote
    // 0 would pass a format check and silently break every time-ordered query.
    assert!(ts > 1_700_000_000, "implausible timestamp {ts}: {line}");
}

#[test]
fn the_working_directory_is_the_one_the_command_ran_in() {
    let scratch = Scratch::new("cwd");
    if bash().is_none() {
        return;
    }
    let subdir = scratch.0.join("deep dir");
    std::fs::create_dir_all(&subdir).expect("make subdir");
    run(&format!("cd '{}'\nls\n", subdir.display()), &scratch);
    let recorded = lines(&scratch);
    let line = recorded
        .iter()
        .find(|l| l.contains(r#""cmd":"ls""#))
        .unwrap_or_else(|| panic!("no entry for `ls`: {recorded:?}"));
    assert!(
        line.contains("deep dir"),
        "the cwd should show the directory the command ran in: {line}"
    );
}

// ── what it refuses to record ──────────────────────────────────────────────

#[test]
fn gcodes_own_commands_are_never_recorded() {
    let scratch = Scratch::new("skip-self");
    if bash().is_none() {
        return;
    }
    run("gcode --fix\ngcode -c hello\nls\n", &scratch);
    let recorded = lines(&scratch);
    for line in &recorded {
        assert!(
            !line.contains(r#""cmd":"gcode"#),
            "gcode recorded itself, which teaches --fix that its own calls fail: {line}"
        );
    }
    assert!(
        recorded.iter().any(|l| l.contains(r#""cmd":"ls""#)),
        "the user's own command must still be recorded: {recorded:?}"
    );
}

/// The regression guard for the source line. `gcode --init` writes
/// `source '<...>/gcode.bash'` into the user's rc file, so this line runs in every
/// new shell. It must not be recorded, or every fresh terminal opens with a
/// spurious entry and `--fix` is offered the hook itself as a failed command.
#[test]
fn the_hooks_own_source_line_is_never_recorded() {
    let scratch = Scratch::new("skip-source");
    if bash().is_none() {
        return;
    }
    run("ls -la\n", &scratch);
    // Read the raw file: `lines()` filters source lines, which would hide exactly
    // the regression this test exists to catch.
    let raw = std::fs::read_to_string(scratch.history()).unwrap_or_default();
    for line in raw.lines() {
        assert!(
            !line.contains("gcode.bash"),
            "the hook recorded its own source line: {line}"
        );
    }
    assert_eq!(raw.lines().count(), 1, "only the user's command: {raw}");
}

#[test]
fn a_users_own_source_of_an_unrelated_file_is_still_recorded() {
    // The skip pattern is about gcode's own hook, not about `source` in general.
    // A hook that swallowed every `source` would hide real commands.
    let scratch = Scratch::new("skip-source-other");
    if bash().is_none() {
        return;
    }
    run("source /dev/null\nls\n", &scratch);
    let recorded = lines(&scratch);
    assert!(
        recorded.iter().any(|l| l.contains("/dev/null")),
        "an unrelated source must still be recorded: {recorded:?}"
    );
}

#[test]
fn an_empty_prompt_records_nothing() {
    let scratch = Scratch::new("empty");
    if bash().is_none() {
        return;
    }
    run("\n\n\n", &scratch);
    assert!(
        lines(&scratch).is_empty(),
        "pressing enter at an empty prompt is not a command: {:?}",
        lines(&scratch)
    );
}

#[test]
fn no_history_disables_recording_entirely() {
    let scratch = Scratch::new("opt-out");
    if bash().is_none() {
        return;
    }
    run("export GCODE_NO_HISTORY=1\nls -la\n", &scratch);
    assert!(
        !scratch.history().exists() || lines(&scratch).is_empty(),
        "GCODE_NO_HISTORY=1 must write nothing, and the file must not even appear: {:?}",
        lines(&scratch)
    );
}

#[test]
fn the_opt_out_is_read_per_prompt_not_per_shell() {
    let scratch = Scratch::new("opt-out-late");
    if bash().is_none() {
        return;
    }
    // Set after the hook is sourced. If the hook decided once at load time, this
    // would record and the user's emergency `export` would not help.
    run("ls -la\nexport GCODE_NO_HISTORY=1\nls\n", &scratch);
    let recorded = lines(&scratch);
    assert!(
        recorded.iter().any(|l| l.contains("ls -la")),
        "the first command should be recorded: {recorded:?}"
    );
    assert!(
        !recorded.iter().any(|l| l.contains(r#""cmd":"ls""#)),
        "the command after the opt-out must not be: {recorded:?}"
    );
}

// ── the file it writes ────────────────────────────────────────────────────

#[test]
fn the_history_file_is_not_world_readable() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("mode");
    if bash().is_none() {
        return;
    }
    run("ls -la\n", &scratch);
    let meta = std::fs::metadata(scratch.history()).expect("history file exists");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode & 0o077,
        0,
        "the file must not be readable by group or other: {mode:o}"
    );
}

#[test]
fn every_line_is_one_complete_json_object() {
    let scratch = Scratch::new("one-per-line");
    if bash().is_none() {
        return;
    }
    // `false` rather than `exit 3`: `exit` ends the shell, so nothing can follow
    // it, and the point here is that a non-zero status is still a recorded command.
    run("ls\npwd\nfalse\necho done\n", &scratch);
    let recorded = lines(&scratch);
    assert_eq!(recorded.len(), 4, "{recorded:?}");
    for line in &recorded {
        assert!(line.starts_with('{'), "line must start with `{{`: {line}");
        assert!(line.ends_with('}'), "line must end with `}}`: {line}");
        assert!(
            !line[1..line.len() - 1].contains('\n'),
            "a raw newline inside an entry breaks one-object-per-line: {line}"
        );
        // Every entry carries the same five fields, in the store's order.
        for field in ["ts", "cmd", "exit", "cwd", "out"] {
            assert!(
                line.contains(&format!("\"{field}\":")),
                "missing {field}: {line}"
            );
        }
    }
}

/// A command containing the two characters that would break the format. The
/// roadmap's store reads these back, and a broken line there is a silent data loss.
#[test]
fn a_command_with_a_quote_or_backslash_produces_one_parseable_line() {
    let scratch = Scratch::new("quoting");
    if bash().is_none() {
        return;
    }
    run("echo 'a\\\\b\"c'\n", &scratch);
    let recorded = lines(&scratch);
    assert_eq!(
        recorded.len(),
        1,
        "the escaping leaked a newline: {recorded:?}"
    );
    let line = &recorded[0];
    assert!(line.contains(r"\\"), "a backslash must be escaped: {line}");
    assert!(line.contains(r#"\""#), "a quote must be escaped: {line}");
}

/// The store parses these. If the hook's escaping and the store's parser disagree,
/// every recorded command with a quote in it is unreadable, so this is checked
/// against the real parser rather than a shape check.
#[test]
fn what_the_hook_writes_parses_as_a_history_entry() {
    let scratch = Scratch::new("parses");
    if bash().is_none() {
        return;
    }
    run("echo 'quote\" back\\\\slash'\n", &scratch);
    let raw = std::fs::read_to_string(scratch.history()).expect("history file");
    let entry: gcode::context::history::HistoryEntry =
        if let Some(l) = lines(&scratch).iter().find(|l| l.contains("quote")) {
            serde_json::from_str(l).expect("the store must be able to read this")
        } else {
            panic!("no entry for the quoted command: {raw}");
        };
    assert_eq!(entry.cmd, "echo 'quote\" back\\\\slash'");
    assert_eq!(entry.exit, 0);
}

// ── not breaking the shell ─────────────────────────────────────────────────

/// A history that has been through many prompts is still one line per command.
/// A `for` loop is one command to the shell, so it must be one entry, not 25.
#[test]
fn many_prompts_in_a_row_stay_one_line_each() {
    let scratch = Scratch::new("many");
    if bash().is_none() {
        return;
    }
    run(
        "for i in {1..25}; do echo iteration-$i; done\nls\n",
        &scratch,
    );
    let recorded = lines(&scratch);
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    let iterations: Vec<&String> = recorded
        .iter()
        .filter(|l| l.contains("echo iteration-"))
        .collect();
    assert_eq!(iterations.len(), 1, "a loop is one entry: {iterations:?}");
    for line in &iterations {
        serde_json::from_str::<gcode::context::history::HistoryEntry>(line)
            .unwrap_or_else(|e| panic!("line does not parse: {line}: {e}"));
    }
}

/// Sourcing the hook under `set -u` must not produce an unbound-variable error.
/// This is the single most common way a `PROMPT_COMMAND` hook breaks a shell, and
/// it shows up on every prompt for a user who has the option on.
#[test]
fn the_hook_survives_set_u() {
    let scratch = Scratch::new("set-u");
    if bash().is_none() {
        return;
    }
    let out = run("set -u\nls -la\necho STILL_ALIVE\n", &scratch);
    assert!(
        out.contains("STILL_ALIVE"),
        "the shell died or complained under set -u: {out}"
    );
    assert!(
        !out.contains("unbound variable"),
        "unbound variable under set -u: {out}"
    );
    assert!(
        !out.contains("PROMPT_COMMAND"),
        "the shell reported a problem with PROMPT_COMMAND: {out}"
    );
}

#[test]
fn the_hook_survives_set_o_pipefail() {
    let scratch = Scratch::new("pipefail");
    if bash().is_none() {
        return;
    }
    let out = run("set -o pipefail\nls -la\necho STILL_ALIVE\n", &scratch);
    assert!(out.contains("STILL_ALIVE"), "{out}");
}

/// A user with no writable history location gets no diagnostic on every prompt. The
/// hook cannot record, and that is not the shell's problem.
#[test]
fn an_unwritable_history_location_does_not_break_the_shell() {
    if bash().is_none() {
        return;
    }
    let out = Command::new(bash().unwrap())
        .arg("--norc")
        .arg("-i")
        .env(
            "GCODE_HISTORY_FILE",
            "/proc/definitely/not/writable/history.jsonl",
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .ok_or_else(|| std::io::Error::other("no stdin"))?
                .write_all(
                    format!(
                        "PS1='$ '; set -o history\nsource '{}'\nls -la\necho STILL_ALIVE\nexit\n",
                        hook_path().display()
                    )
                    .as_bytes(),
                )?;
            child.wait_with_output()
        })
        .expect("run bash");
    let out = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.contains("STILL_ALIVE"),
        "an unwritable store broke the prompt: {out}"
    );
}

/// The 5 ms budget is a real constraint, not a nicety — it runs on every prompt.
///
/// The shell prints raw clock readings and this side does the arithmetic, for the
/// same reason as the zsh test: a version that computed the delta with `perl`
/// one-liners in the shell reported a ~9e9 ms "regression" that was really an
/// empty clock read being compared against 0, and separately reported "no usable
/// result" because `perl -e 'print (COND) ? A : B'` evaluates the ternary as a
/// boolean and prints the empty string. Keeping the arithmetic in Rust means a
/// missing or implausible number fails as a broken probe, not as a slow hook.
///
/// Several trials are run and the **minimum** is judged, because scheduler noise
/// from other tests running concurrently only ever adds time.
#[test]
fn the_hook_costs_under_five_milliseconds_per_prompt() {
    let scratch = Scratch::new("budget");
    if bash().is_none() {
        return;
    }
    let out = run(
        r#"
            for _trial in 1 2 3 4 5; do
                echo "CLOCK_START=$(perl -MTime::HiRes=time -e 'print time')"
                for _i in $(seq 200); do
                    (exit 1)
                    _gcode_capture
                done
                echo "CLOCK_END=$(perl -MTime::HiRes=time -e 'print time')"
            done
        "#,
        &scratch,
    );
    let trials = parse_clock_trials(&out, 200);
    let ms = trials.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        ms.is_finite() && ms > 0.0,
        "no usable timing came back from the probe: {out}"
    );
    assert!(
        ms < 5.0,
        "the hook costs {ms:.3} ms per prompt, over the 5 ms budget"
    );
    eprintln!(
        "measured {ms:.3} ms per prompt (budget 5 ms, best of {} trials)",
        trials.len()
    );
}

/// Per-prompt costs in milliseconds, from `CLOCK_START`/`CLOCK_END` pairs.
///
/// Pairs are formed by label, not by dropping implausible reads and chunking what is
/// left: dropping first would realign the sequence, so one empty `CLOCK_START`
/// would pair the *next* `CLOCK_END` with the wrong start and produce a plausible
/// looking number from two unrelated reads.
///
/// The clock did go empty in the wild, under load. Compared against 0, that
/// difference is a nine-billion-millisecond hook that never existed, so every read
/// is validated and an incomplete pair is discarded.
fn parse_clock_trials(out: &str, iterations: u32) -> Vec<f64> {
    let mut trials = Vec::new();
    let mut pending: Option<f64> = None;
    for line in out.lines() {
        let line = line.trim();
        let value = line
            .strip_prefix("CLOCK_START=")
            .map(|v| ("start", v))
            .or_else(|| line.strip_prefix("CLOCK_END=").map(|v| ("end", v)));
        let Some((kind, raw)) = value else {
            continue;
        };
        let Ok(v) = raw.trim().parse::<f64>() else {
            // An unreadable read breaks the pair it belongs to.
            if kind == "start" {
                pending = None;
            }
            continue;
        };
        if !v.is_finite() || v <= 1_000_000_000.0 {
            if kind == "start" {
                pending = None;
            }
            continue;
        }
        match kind {
            "start" => pending = Some(v),
            "end" => {
                if let Some(start) = pending.take() {
                    let ms = (v - start) * 1000.0 / f64::from(iterations);
                    if ms.is_finite() && ms > 0.0 {
                        trials.push(ms);
                    }
                }
            }
            _ => unreachable!("kind is either start or end"),
        }
    }
    trials
}

#[test]
fn a_broken_clock_reading_is_reported_as_a_broken_probe_not_a_slow_hook() {
    // The regression guard on `parse_clock_trials` itself. The clock went empty in
    // the wild under load, and the resulting `end - start` was compared against 0
    // and reported as a nine-billion-millisecond hook.
    let out =
        "CLOCK_START=\nCLOCK_END=1790859600.5\nCLOCK_START=1790859600.0\nCLOCK_END=1790859600.5\n";
    let trials = parse_clock_trials(out, 200);
    assert_eq!(trials.len(), 1, "{trials:?}");
    assert!(
        trials[0].is_finite() && trials[0] < 5.0,
        "a dropped empty read must not become an enormous cost: {trials:?}"
    );
}

// ── control characters ──────────────────────────────────────────────────────

/// Runs `_gcode_json_escape` in a real bash and returns its output.
///
/// The function is exercised directly rather than through a prompt, because a
/// control character cannot be typed into an interactive session and a command
/// containing one is stored in history as the literal bytes the user entered.
fn escape(value: &str) -> String {
    let Some(bash) = bash() else {
        return String::new();
    };
    let script = format!(
        "source '{hook}'; _gcode_json_escape \"$1\"",
        hook = hook_path().display()
    );
    let out = Command::new(bash)
        .arg("--norc")
        .arg("-c")
        .arg(&script)
        .arg("gcode-escape")
        .arg(value)
        .output()
        .expect("run bash");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_control_character_in_a_command_produces_a_line_the_store_can_read() {
    // The bug this replaces: only \n, \r, and \t were replaced, so a command
    // containing any other control byte wrote a line the JSON parser rejected —
    // losing the whole entry, not just the field.
    if bash().is_none() {
        return;
    }
    for raw in ["a\u{8}b", "a\u{c}b", "a\u{7}b", "a\u{1}b", "a\u{7f}b"] {
        let escaped = escape(raw);
        let json = format!("\"{escaped}\"");
        serde_json::from_str::<String>(&json)
            .unwrap_or_else(|e| panic!("{raw:?} escaped to {json:?}, which is not JSON: {e}"));
    }
}

#[test]
fn non_ascii_passes_through_the_escaper_untouched() {
    // The opposite failure. Replacing control characters must not mangle UTF-8,
    // or every command with an accented character in it would be corrupted.
    if bash().is_none() {
        return;
    }
    for text in ["café", "naïve", "λ", "日本語"] {
        assert_eq!(escape(text), text, "non-ASCII was mangled: {text:?}");
    }
}

#[test]
fn quotes_and_backslashes_still_survive_alongside_control_characters() {
    if bash().is_none() {
        return;
    }
    let escaped = escape("a\"b\\c\u{8}d");
    let json = format!("\"{escaped}\"");
    let parsed: String = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.contains('"'), "quote lost: {parsed}");
    assert!(parsed.contains('\\'), "backslash lost: {parsed}");
    assert!(
        !parsed.contains('\u{8}'),
        "control character survived: {parsed:?}"
    );
}
