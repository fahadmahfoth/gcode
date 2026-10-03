//! Tests for `shell/gcode.zsh`, run in a real zsh.
//!
//! # Why a real shell and not a mock
//!
//! Everything this hook gets wrong is about zsh itself. `status` is a read-only
//! special variable in zsh, so `local status=$?` fails outright — the hook breaks
//! on every prompt and records nothing, with no obvious error message. That is
//! exactly the class of bug a mock cannot catch, so every test here starts a real
//! `zsh` and asks it what happened.
//!
//! # Isolation
//!
//! `HISTFILE` is cleared and `--no-rcs` is passed, so no test reads the
//! developer's `~/.zshrc` or their real shell history. Every test writes to a
//! temporary `GCODE_HISTORY_FILE`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn shell() -> Option<PathBuf> {
    for c in ["/bin/zsh", "/usr/local/bin/zsh", "/opt/homebrew/bin/zsh"] {
        let p = Path::new(c);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    assert!(
        std::env::var_os("GCODE_REQUIRE_SHELLS").is_none(),
        "zsh is not installed and GCODE_REQUIRE_SHELLS is set"
    );
    eprintln!("skipped: zsh is not installed");
    None
}

fn hook_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/gcode.zsh")
}

/// A temporary directory that removes itself.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("gcode-zsh-{}-{label}-{n}", std::process::id()));
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

/// Run a script in an interactive-ish zsh with the hook sourced.
///
/// `-i` matters: the history is empty in a non-interactive zsh and the hook reads
/// `fc -ln -1`. `INC_APPEND_HISTORY` makes each line available immediately, which is
/// what a real user's shell has and what `SHARE_HISTORY` (zsh's default) does
/// differently. `precmd_functions` is cleared before exit so the final prompt does
/// not record an extra entry and every count assertion is exact.
fn run(script: &str, scratch: &Scratch) -> String {
    let Some(sh) = shell() else {
        return String::new();
    };
    let prelude = format!(
        "HISTFILE=\n\
         setopt INC_APPEND_HISTORY\n\
         unsetopt SHARE_HISTORY\n\
         PROMPT='GCODE_TEST_PROMPT '\n\
         export GCODE_HISTORY_FILE='{}'\n\
         source '{}'\n",
        scratch.history().display(),
        hook_path().display()
    );
    let out = Command::new(sh)
        .arg("--no-rcs")
        .arg("-i")
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
                .write_all(format!("{prelude}{script}\nprecmd_functions=()\nexit\n").as_bytes())?;
            child.wait_with_output()
        });
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(e) => panic!("could not run zsh: {e}"),
    }
}

/// The history lines written by `script`.
///
/// The prelude's own `source` line is filtered out as a backstop. The hook now
/// skips it itself, and `the_hooks_own_source_line_is_never_recorded` proves that
/// directly; this filter remains so a future regression there shows up as one
/// failing test rather than as every count assertion in the file shifting by one.
fn lines(scratch: &Scratch) -> Vec<String> {
    std::fs::read_to_string(scratch.history())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.contains("gcode.zsh"))
        .map(str::to_owned)
        .collect()
}

// ── the guarantees that matter ─────────────────────────────────────────────

/// ADR 0007 rule 5, and the roadmap's "preserves the user's `$?`".
#[test]
fn a_failing_command_is_recorded_with_its_exit_code() {
    let scratch = Scratch::new("exit-code");
    if shell().is_none() {
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

/// The regression that motivated this file existing: in zsh, `status` is a
/// read-only alias for `?`. `local status=$?` fails on the first line of the hook,
/// so the hook records nothing and prints an error on every single prompt. The
/// variable is named `ret` for this reason.
#[test]
fn the_hook_does_not_trip_over_the_read_only_status_variable() {
    let scratch = Scratch::new("status-var");
    if shell().is_none() {
        return;
    }
    let out = run("ls\n", &scratch);
    assert!(
        !out.contains("read-only variable"),
        "the hook is writing to zsh's read-only `status`: {out}"
    );
    assert!(
        !out.contains("_gcode_capture"),
        "the hook is printing a diagnostic into the prompt: {out}"
    );
    assert_eq!(lines(&scratch).len(), 1, "{:?}", lines(&scratch));
}

/// The user still sees their own `$?` after the hook has run.
#[test]
fn the_users_exit_status_survives_the_hook() {
    let scratch = Scratch::new("dollar-question");
    if shell().is_none() {
        return;
    }
    let out = run("(exit 42)\necho \"USER_SEES=$?\"\n", &scratch);
    assert!(
        out.contains("USER_SEES=42"),
        "the hook ate the exit status: {out}"
    );
}

/// ADR 0007 rule 1: chain onto whatever is there. zsh keeps `precmd_functions` as
/// an array, so the existing entries must survive alongside ours.
#[test]
fn existing_precmd_functions_still_run_and_still_see_the_status() {
    let scratch = Scratch::new("chain");
    if shell().is_none() {
        return;
    }
    let out = run(
        &format!(
            "user_precmd() {{ echo \"USER_PC_SEES=$?\"; }}\n\
             precmd_functions=(user_precmd)\n\
             unset _GCODE_HOOK_LOADED\n\
             source '{}'\n\
             echo \"FUNCS=$precmd_functions\"\n\
             (exit 7)\n",
            hook_path().display()
        ),
        &scratch,
    );
    assert!(
        out.contains("USER_PC_SEES=7"),
        "the pre-existing precmd hook lost `$?`: {out}"
    );
    assert!(
        out.contains("user_precmd _gcode_capture") || out.contains("_gcode_capture user_precmd"),
        "the hook was not chained alongside the existing entry: {out}"
    );
}

/// A user's `precmd` hook that returns non-zero must not leak into the store. In
/// zsh this is safe — zsh restores `$?` for each `precmd` hook, unlike bash's
/// `PROMPT_COMMAND` — and this test is what proves that rather than assuming it.
#[test]
fn a_precmd_hook_that_returns_non_zero_does_not_corrupt_the_recorded_exit() {
    let scratch = Scratch::new("status-clobber");
    if shell().is_none() {
        return;
    }
    run(
        &format!(
            "noise_precmd() {{ return 9; }}\n\
             precmd_functions=(noise_precmd)\n\
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
        "another precmd hook overwrote the real exit status: {entry}"
    );
    assert!(
        !recorded.iter().any(|l| l.contains(r#""exit":9"#)),
        "another hook's own status leaked into the store: {recorded:?}"
    );
}

/// Chain twice. Sourcing twice must not record twice.
#[test]
fn sourcing_the_hook_twice_records_once() {
    let scratch = Scratch::new("twice");
    if shell().is_none() {
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

// ── what it records ────────────────────────────────────────────────────────

#[test]
fn the_command_directory_and_timestamp_are_recorded() {
    let scratch = Scratch::new("fields");
    if shell().is_none() {
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
    let ts: i64 = line
        .split(r#""ts":"#)
        .nth(1)
        .and_then(|s| s.split([',', '}']).next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no parseable ts in {line}"));
    assert!(ts > 1_700_000_000, "implausible timestamp {ts}: {line}");
}

#[test]
fn the_working_directory_is_the_one_the_command_ran_in() {
    let scratch = Scratch::new("cwd");
    if shell().is_none() {
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
    if shell().is_none() {
        return;
    }
    run("gcode --fix\ngcode -c hello\nls\n", &scratch);
    let recorded = lines(&scratch);
    for line in &recorded {
        assert!(
            !line.contains(r#""cmd":"gcode"#),
            "gcode recorded itself: {line}"
        );
    }
    assert!(
        recorded.iter().any(|l| l.contains(r#""cmd":"ls""#)),
        "the user's own command must still be recorded: {recorded:?}"
    );
}

#[test]
fn the_hooks_own_source_line_is_never_recorded() {
    let scratch = Scratch::new("skip-source");
    if shell().is_none() {
        return;
    }
    run("ls -la\n", &scratch);
    // Raw read: `lines()` filters source lines and would hide the regression.
    let raw = std::fs::read_to_string(scratch.history()).unwrap_or_default();
    for line in raw.lines() {
        assert!(
            !line.contains("gcode.zsh"),
            "the hook recorded its own source line: {line}"
        );
    }
    assert_eq!(raw.lines().count(), 1, "only the user's command: {raw}");
}

#[test]
fn a_users_own_source_of_an_unrelated_file_is_still_recorded() {
    let scratch = Scratch::new("skip-source-other");
    if shell().is_none() {
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
    if shell().is_none() {
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
    if shell().is_none() {
        return;
    }
    run("export GCODE_NO_HISTORY=1\nls -la\n", &scratch);
    assert!(
        !scratch.history().exists() || lines(&scratch).is_empty(),
        "GCODE_NO_HISTORY=1 must write nothing: {:?}",
        lines(&scratch)
    );
}

/// Set after the hook is sourced. A hook that decided once at load time would not
/// honour an emergency `export`.
#[test]
fn the_opt_out_is_read_per_prompt_not_per_shell() {
    let scratch = Scratch::new("opt-out-late");
    if shell().is_none() {
        return;
    }
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

// ── the file it writes ─────────────────────────────────────────────────────

#[test]
fn the_history_file_is_not_world_readable() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("mode");
    if shell().is_none() {
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
    if shell().is_none() {
        return;
    }
    run("ls\npwd\nfalse\necho done\n", &scratch);
    let recorded = lines(&scratch);
    assert_eq!(recorded.len(), 4, "{recorded:?}");
    for line in &recorded {
        assert!(line.starts_with('{'), "line must start with `{{`: {line}");
        assert!(line.ends_with('}'), "line must end with `}}`: {line}");
        for field in ["ts", "cmd", "exit", "cwd", "out"] {
            assert!(
                line.contains(&format!("\"{field}\":")),
                "missing {field}: {line}"
            );
        }
    }
}

/// The store parses these. If the hook's escaping and the store's parser disagree,
/// every recorded command with a quote in it is unreadable, so this is checked
/// against the real parser rather than a shape check.
#[test]
fn a_quoted_command_parses_as_a_history_entry() {
    let scratch = Scratch::new("quoting");
    if shell().is_none() {
        return;
    }
    run("echo 'quote\" back\\\\slash'\n", &scratch);
    let raw = std::fs::read_to_string(scratch.history()).expect("history file");
    let recorded = lines(&scratch);
    assert_eq!(
        recorded.len(),
        1,
        "the escaping leaked a newline: {recorded:?}"
    );
    let entry: gcode::context::history::HistoryEntry = serde_json::from_str(&recorded[0])
        .unwrap_or_else(|e| {
            panic!(
                "the store must be able to read this line: {e}\nline: {}\nraw: {raw}",
                recorded[0]
            )
        });
    assert_eq!(entry.cmd, "echo 'quote\" back\\\\slash'");
    assert_eq!(entry.exit, 0);
}

// ── not breaking the shell ─────────────────────────────────────────────────

/// zsh's `setopt nounset` equivalent is `setopt NO_UNSET`, and a hook that assumes a
/// variable exists breaks every prompt for a user who has it on.
#[test]
fn the_hook_survives_setopt_no_unset() {
    let scratch = Scratch::new("no-unset");
    if shell().is_none() {
        return;
    }
    let out = run("setopt NO_UNSET\nls -la\necho STILL_ALIVE\n", &scratch);
    assert!(out.contains("STILL_ALIVE"), "the shell died: {out}");
    assert!(
        !out.contains("parameter not set"),
        "an unset parameter broke the prompt: {out}"
    );
}

/// A user with no writable history location gets no diagnostic on every prompt.
#[test]
fn an_unwritable_history_location_does_not_break_the_shell() {
    if shell().is_none() {
        return;
    }
    let out = Command::new(shell().unwrap())
        .arg("--no-rcs")
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
                        "HISTFILE=\nPROMPT='GCODE_TEST_PROMPT '\nsource '{}'\n\
                         ls -la\necho STILL_ALIVE\nexit\n",
                        hook_path().display()
                    )
                    .as_bytes(),
                )?;
            child.wait_with_output()
        })
        .expect("run zsh");
    let out = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.contains("STILL_ALIVE"),
        "an unwritable store broke the prompt: {out}"
    );
}

/// A history that has been through many prompts is still one line per command.
/// A `for` loop is one command to the shell, so it must be one entry, not 25.
#[test]
fn many_prompts_in_a_row_stay_one_line_each() {
    let scratch = Scratch::new("many");
    if shell().is_none() {
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

/// The 5 ms budget is a real constraint: the hook runs on every prompt.
///
/// The shell prints the raw clock readings and this side does the arithmetic. That
/// split is deliberate: an earlier version did the comparison in the shell with
/// `perl` one-liners, and it reported "no usable result" every time — the probe was
/// broken, not the hook. Keeping the measurement in Rust means the probe only has
/// to produce numbers, and a number that is missing or implausible fails here with
/// a clear message instead of silently becoming a latency regression.
///
/// Several trials are run and the **minimum** is judged, because scheduler noise
/// from other tests running at the same time only ever adds time.
#[test]
fn the_hook_costs_under_five_milliseconds_per_prompt() {
    let scratch = Scratch::new("budget");
    if shell().is_none() {
        return;
    }
    let out = run(
        r#"
            for trial in 1 2 3 4 5; do
                echo "CLOCK_START=$(perl -MTime::HiRes=time -e 'print time')"
                for i in {1..200}; do
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

// ── control characters ──────────────────────────────────────────────────────

/// Runs `_gcode_json_escape` in a real zsh and returns its output.
///
/// Direct rather than through a prompt, for the reason the bash file gives: a
/// control character cannot be typed into an interactive session, and a command
/// containing one is stored as the literal bytes the user entered.
fn escape(value: &str) -> String {
    let Some(sh) = shell() else {
        return String::new();
    };
    let script = format!(
        "source '{hook}'; _gcode_json_escape \"$1\"",
        hook = hook_path().display()
    );
    let out = Command::new(sh)
        .arg("-f")
        .arg("-c")
        .arg(&script)
        .arg("gcode-escape")
        .arg(value)
        .output()
        .expect("run zsh");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_control_character_produces_a_line_the_store_can_read() {
    // zsh's escaper enumerated three control characters like bash's used to.
    // RFC 8259 forbids all of U+0000..U+001F, and a missed one loses the entry.
    if shell().is_none() {
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
    if shell().is_none() {
        return;
    }
    for text in ["café", "naïve", "λ", "日本語"] {
        assert_eq!(escape(text), text, "non-ASCII was mangled: {text:?}");
    }
}

#[test]
fn both_hooks_escape_identically() {
    // The two files are separate and will drift. A user who switches shells must
    // not find that the same command is recorded differently in each.
    if shell().is_none() {
        return;
    }
    for raw in ["a\"b", "a\\b", "a\u{8}b", "plain", "caf\u{e9}"] {
        assert_eq!(
            escape(raw),
            gcode_escaper::escape_for_comparison(raw),
            "the two hooks disagree about {raw:?}"
        );
    }
}

/// The bash escaper, for the comparison above.
mod gcode_escaper {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn bash() -> Option<PathBuf> {
        ["/bin/bash", "/usr/local/bin/bash", "/opt/homebrew/bin/bash"]
            .iter()
            .map(Path::new)
            .find(|p| p.is_file())
            .map(Path::to_path_buf)
    }

    pub fn escape_for_comparison(value: &str) -> String {
        let Some(bash) = bash() else {
            return String::new();
        };
        let script = format!(
            "source '{hook}'; _gcode_json_escape \"$1\"",
            hook = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("shell/gcode.bash")
                .display()
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
}
