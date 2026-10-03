//! The run loop, tested without a process boundary and without a model.
//!
//! Phase 1.8's contract: `run(cli) -> Result<Output>`, testable from here, with
//! a `FakeEngine` standing in for the sampler that Phase 1.5 has not built yet.
//! Nothing in this file loads a model or reads a real terminal, so the whole
//! safety surface of the tool is exercised in milliseconds.
//!
//! What is verified here is the part of the pipeline that does not need
//! inference: classification, the block, consent, `--yes`, `--dry-run`, and the
//! shape of `--json`.

use std::sync::Arc;
use std::sync::Mutex;

/// Erase a concrete engine at the call site. `Arc::clone` alone does not coerce to
/// `Arc<dyn InferenceEngine>`; the ascription inside the helper does.
fn arc<T: InferenceEngine + 'static>(engine: &Arc<T>) -> Arc<dyn InferenceEngine> {
    engine.clone()
}

use gcode::cli::{self, Parsed};
use gcode::context::history::{History, HistoryEntry};
use gcode::error::Error;
use gcode::exec::Executor;
use gcode::inference::{EngineInfo, GenParams, InferenceEngine};
use gcode::runtime::{
    config_failure_is_fatal, fix, run, run_configured, Consenter, Decision, DenyAll, Runner,
};
use gcode::safety::Risk;

// ── the fake ────────────────────────────────────────────────────────────────

/// Returns a canned command and records what it was asked for.
struct FakeEngine {
    reply: String,
    seen: Mutex<Vec<String>>,
}

impl FakeEngine {
    fn new(reply: &str) -> Self {
        Self {
            reply: reply.to_owned(),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn prompts(&self) -> Vec<String> {
        self.seen.lock().expect("unpoisoned").clone()
    }
}

impl InferenceEngine for FakeEngine {
    fn generate(&self, prompt: &str, _params: &GenParams) -> gcode::Result<String> {
        self.seen
            .lock()
            .expect("unpoisoned")
            .push(prompt.to_owned());
        Ok(self.reply.clone())
    }

    fn info(&self) -> EngineInfo {
        EngineInfo {
            model_name: "fake".to_owned(),
            context_size: 4096,
            threads: 1,
            greedy: true,
        }
    }
}

// ── a consenter that answers whatever the test needs ─────────────────────────

/// Answers from a script, so a test can say "the user typed y then y" without a
/// terminal.
struct Scripted(Vec<Decision>);

impl Scripted {
    fn yes() -> Self {
        Self(vec![Decision::Granted(String::new())])
    }
}

impl Consenter for Scripted {
    fn ask(&mut self, command: &str, _verdict: &gcode::safety::Verdict) -> Decision {
        // Both a scripted denial and running out of script are denials. A test
        // that grants consent twice and finds the second answer also granted
        // would be testing the script, not the tool, so an exhausted script says
        // no — the same answer an unreachable terminal gives.
        match self.0.pop() {
            Some(Decision::Granted(_)) => Decision::Granted(command.to_owned()),
            Some(Decision::Denied) | None => Decision::Denied,
        }
    }
}

/// A consenter that hands back a different command than it was offered, which is
/// what an `e` edit does.
struct Editor(String);

impl Consenter for Editor {
    fn ask(&mut self, _command: &str, _verdict: &gcode::safety::Verdict) -> Decision {
        Decision::Granted(self.0.clone())
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn parse(args: &[&str]) -> Parsed {
    cli::parse_from(args.iter().copied()).expect("valid arguments")
}

/// The default prompt threshold, so a test that does not care about consent is
/// written against the same value `main` uses.
const ALWAYS: Risk = Risk::Safe;

fn generate(args: &[&str], reply: &str) -> (Parsed, Arc<FakeEngine>) {
    let parsed = parse(args);
    let engine = Arc::new(FakeEngine::new(reply));
    (parsed, engine)
}

// ── run() with a FakeEngine ─────────────────────────────────────────────────

/// The roadmap's headline test: `run()` with a `FakeEngine` produces the
/// expected `Output`.
#[test]
fn run_with_a_fake_engine_produces_the_expected_output() {
    let (parsed, engine) = generate(&["gcode", "-c", "list all files"], "ls -la");
    let out =
        run(&parsed, Some(arc(&engine)), &mut Scripted::yes(), ALWAYS).expect("consent granted");
    assert_eq!(out.command, "ls -la");
    assert_eq!(out.level, Risk::Safe);
    assert_eq!(out.mode, "generate");
    assert!(!out.executed, "nothing ran: no executor exists yet");
    let prompts = engine.prompts();
    assert_eq!(prompts.len(), 1, "one generation, one prompt");
    assert!(
        prompts[0].contains("list all files"),
        "the request reaches the model inside the assembled prompt: {}",
        prompts[0]
    );
    assert!(
        prompts[0].contains("<|system|>"),
        "the request is wrapped by build_prompt, not sent raw: {}",
        prompts[0]
    );
}

#[test]
fn a_secret_in_the_request_is_redacted_before_the_prompt() {
    // ADR 0006 applies to the request too: it is the user's own words, but it can
    // carry a pasted credential, so generate redacts it through build_prompt like
    // every other generative mode.
    let planted = "sk-QQQQ1111WWWW2222EEEE3333RRRR4444";
    let request = format!("curl -H 'X-Key: {planted}' https://example.test");
    let (parsed, engine) = generate(&["gcode", "-c", &request, "--dry-run"], "ls");
    let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("a safe command");
    assert_eq!(out.command, "ls");
    let prompt = engine.prompts().join("\n");
    assert!(
        !prompt.contains(planted),
        "the request's secret reached the prompt: {prompt}"
    );
    assert!(prompt.contains("[REDACTED]"), "{prompt}");
}

#[test]
fn a_safe_command_is_asked_about_too() {
    // `DenyAll` cannot consent to anything, so a SAFE command refused under it
    // proves consent is being asked for.
    let (parsed, engine) = generate(&["gcode", "-c", "list files"], "ls");
    let err = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS)
        .expect_err("SAFE needs a yes by default");
    assert!(
        matches!(err, Error::ConsentDenied { level: Risk::Safe }),
        "{err:?}"
    );

    let out = run(&parsed, Some(arc(&engine)), &mut Scripted::yes(), ALWAYS)
        .expect("a yes lets it through");
    assert_eq!(out.level, Risk::Safe);
}

#[test]
fn a_low_command_is_asked_about_too() {
    let (parsed, engine) = generate(&["gcode", "-c", "make a directory"], "mkdir -p out");
    let err = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS)
        .expect_err("LOW needs a yes by default");
    assert!(matches!(err, Error::ConsentDenied { .. }), "{err:?}");
}

#[test]
fn yes_and_dry_run_are_the_only_ways_past_the_question_for_a_safe_command() {
    for flag in ["--yes", "--dry-run"] {
        let (parsed, engine) = generate(&["gcode", "-c", "list files", flag], "ls");
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS)
            .unwrap_or_else(|e| panic!("{flag} should not ask: {e:?}"));
    }
}

#[test]
fn a_raised_threshold_stops_asking_about_safe_and_low() {
    let (parsed, engine) = generate(&["gcode", "-c", "list files"], "ls");
    run(&parsed, Some(arc(&engine)), &mut DenyAll, Risk::Medium)
        .expect("an explicit always_confirm = MEDIUM restores the old behaviour");
}

#[test]
fn complete_does_not_ask_below_medium() {
    let (parsed, engine) = generate(&["gcode", "--complete", "ls -"], "ls -la");
    run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS)
        .expect("a completion is text for the shell, and runs nothing");
}

#[test]
fn a_generated_command_is_classified_not_trusted() {
    // The engine says "rm -rf ./build". The tool does not take its word for it.
    let (parsed, engine) = generate(
        &["gcode", "-c", "clean everything", "--yes"],
        "rm -rf ./build",
    );
    let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("HIGH is not blocked");
    assert_eq!(out.level, Risk::High);
    assert!(
        !out.reasons.is_empty(),
        "a HIGH command must carry its reasons"
    );
    assert!(!out.executed);
}

#[test]
fn the_level_comes_from_the_classifier_never_from_the_prompt() {
    // Two different requests, two different dangerous replies: the level must
    // follow the command the engine emitted.
    for (reply, expected) in [
        ("rm -rf /", Risk::Critical),
        ("ls -la", Risk::Safe),
        ("history -c", Risk::Medium),
    ] {
        let (parsed, engine) = generate(&["gcode", "-c", "do something"], reply);
        match run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS) {
            Ok(out) => assert_eq!(out.level, expected, "{reply}"),
            Err(Error::ConsentDenied { level }) => {
                // A command needing consent is still classified. The refusal is
                // the prompt's doing, not the classifier's.
                assert_eq!(level, expected, "{reply}");
            }
            Err(Error::RiskBlocked { level, .. }) => assert_eq!(level, expected, "{reply}"),
            Err(e) => panic!("unexpected error for {reply}: {e}"),
        }
    }
}

// ── the block ───────────────────────────────────────────────────────────────

/// A CRITICAL command from the engine is refused and nothing runs.
#[test]
fn a_critical_command_is_refused_whatever_the_flags() {
    for extra in [
        vec![],
        vec!["--yes"],
        vec!["--dry-run"],
        vec!["--yes", "--dry-run"],
    ] {
        let mut args = vec!["gcode", "-c", "delete everything"];
        args.extend(extra.iter().copied());
        let (parsed, engine) = generate(&args, "rm -rf /");
        let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS);
        match out {
            Err(Error::RiskBlocked { level, reasons }) => {
                assert_eq!(level, Risk::Critical);
                assert!(!reasons.is_empty(), "the refusal must say why");
            }
            Ok(out) => panic!("ran a CRITICAL command with {extra:?}: {out:?}"),
            Err(e) => panic!("wrong error with {extra:?}: {e}"),
        }
    }
}

/// Invariant 6, end to end: no flag reaches past the block.
#[test]
fn no_flag_combination_unblocks_critical() {
    let flag_sets: Vec<Vec<&str>> = vec![
        vec![],
        vec!["--yes"],
        vec!["-y"],
        vec!["--dry-run"],
        vec!["-n"],
        vec!["--json"],
        vec!["--yes", "--json"],
        vec!["--yes", "--dry-run", "--json"],
        vec!["--no-history"],
    ];
    for flags in flag_sets {
        let mut args = vec!["gcode", "-c", "nuke"];
        args.extend(flags.iter().copied());
        let (parsed, engine) = generate(&args, "rm -rf /");
        assert!(
            matches!(
                run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS),
                Err(Error::RiskBlocked { .. })
            ),
            "flags {flags:?} got past the block"
        );
    }
}

/// A CRITICAL command is refused even when a human says yes, because the block
/// happens before the prompt.
#[test]
fn a_human_cannot_approve_a_critical_command() {
    let (parsed, engine) = generate(&["gcode", "-c", "delete everything"], "rm -rf /");
    let mut yes = Scripted::yes();
    assert!(matches!(
        run(&parsed, Some(arc(&engine)), &mut yes, ALWAYS),
        Err(Error::RiskBlocked { .. })
    ));
}

// ── consent, --yes, and the pipe ────────────────────────────────────────────

/// Invariant 4: non-interactive stdin fails closed.
#[test]
fn a_pipe_cannot_run_a_command_that_needs_consent() {
    for reply in ["rm -rf ./build", "history -c", "kill -9 1"] {
        let (parsed, engine) = generate(&["gcode", "-c", "do a thing"], reply);
        let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS);
        assert!(
            matches!(out, Err(Error::ConsentDenied { .. })),
            "{reply} ran in a pipe: {out:?}"
        );
    }
}

#[test]
fn consent_granted_lets_a_high_command_proceed() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean the build"], "rm -rf ./build");
    let mut yes = Scripted::yes();
    let out = run(&parsed, Some(arc(&engine)), &mut yes, ALWAYS)
        .expect("a HIGH command may run once confirmed");
    assert_eq!(out.level, Risk::High);
    assert!(!out.executed, "still no executor in Phase 1.8");
}

/// Invariant 5: `--yes` suppresses the prompt and nothing else.
#[test]
fn yes_replaces_the_prompt_but_not_the_classification() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--yes"], "rm -rf ./build");
    let mut denied = DenyAll;
    let out =
        run(&parsed, Some(arc(&engine)), &mut denied, ALWAYS).expect("--yes skips the prompt");
    assert_eq!(out.level, Risk::High, "the level is unchanged by --yes");
    assert!(!out.reasons.is_empty(), "the reasons are still reported");
}

#[test]
fn yes_still_prints_the_level_and_reasons() {
    // Roadmap 3.7: "still classifies, still prints the level and reasons".
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--yes"], "history -c");
    let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("--yes runs it");
    assert_eq!(out.level, Risk::Medium);
    assert!(!out.reasons.is_empty());
}

/// Invariant 3: an edit is re-classified. `e` turning a HIGH command into a
/// CRITICAL one is re-blocked, not run.
#[test]
fn an_edit_that_turns_high_into_critical_is_re_blocked() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean"], "rm -rf ./build");
    let mut editor = Editor("rm -rf /".to_owned());
    match run(&parsed, Some(arc(&engine)), &mut editor, ALWAYS) {
        Err(Error::RiskBlocked { level, .. }) => assert_eq!(level, Risk::Critical),
        Ok(out) => panic!("the edited command ran: {out:?}"),
        Err(e) => panic!("wrong error: {e}"),
    }
}

#[test]
fn an_edit_that_stays_safe_is_classified_as_the_new_command() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean"], "rm -rf ./build");
    let mut editor = Editor("ls -la".to_owned());
    let out = run(&parsed, Some(arc(&engine)), &mut editor, ALWAYS)
        .expect("a SAFE edit needs no consent");
    assert_eq!(
        out.command, "ls -la",
        "the edited command is what is reported"
    );
    assert_eq!(out.level, Risk::Safe);
}

// ── --dry-run ───────────────────────────────────────────────────────────────

#[test]
fn dry_run_returns_the_command_and_the_level_and_runs_nothing() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--dry-run"], "rm -rf ./build");
    // DenyAll would refuse this command without --dry-run, so a success here also
    // proves the flag short-circuits before the prompt.
    let out =
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("a dry run always succeeds");
    assert_eq!(out.command, "rm -rf ./build");
    assert_eq!(out.level, Risk::High);
    assert!(!out.executed);
}

#[test]
fn dry_run_still_refuses_critical_and_exits_non_zero() {
    // The acceptance criterion in Phase 3: `gcode -c "…" -y` exits non-zero and
    // runs nothing. A dry run is not an exemption.
    let (parsed, engine) = generate(&["gcode", "-c", "nuke", "--dry-run"], "rm -rf /");
    assert!(matches!(
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS),
        Err(Error::RiskBlocked { .. })
    ));
}

#[test]
fn the_no_alias_behaves_identically_to_dry_run() {
    for flag in ["-n", "--no", "--dry-run"] {
        let (parsed, engine) = generate(&["gcode", "-c", "clean", flag], "history -c");
        let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS)
            .unwrap_or_else(|e| panic!("{flag}: {e}"));
        assert!(!out.executed, "{flag} executed");
    }
}

// ── --json ──────────────────────────────────────────────────────────────────

#[test]
fn json_is_a_single_object_on_one_line() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--json", "--yes"], "ls -la");
    let out =
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("SAFE needs no consent");
    let json = out.to_json();
    assert!(!json.contains('\n'), "not one line: {json}");
    assert!(
        json.starts_with('{') && json.ends_with('}'),
        "not an object: {json}"
    );
}

#[test]
fn json_carries_the_level_reasons_and_executed_flag() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--json", "--yes"], "history -c");
    let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("--json --yes runs it");
    let json = out.to_json();
    for key in [
        "\"command\":",
        "\"level\":",
        "\"mode\":",
        "\"reasons\":",
        "\"segments\":",
        "\"executed\":",
    ] {
        assert!(json.contains(key), "missing {key} in {json}");
    }
    assert!(json.contains("\"level\":\"MEDIUM\""), "{json}");
    assert!(json.contains("\"executed\":false"), "{json}");
}

#[test]
fn json_never_runs_a_command_needing_consent() {
    // `--json` is wired to `DenyAll` in `main`, so it cannot prompt. Here the
    // same rule is checked directly: the caller has to opt in to consent.
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--json"], "history -c");
    assert!(matches!(
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS),
        Err(Error::ConsentDenied { .. })
    ));
}

#[test]
fn json_escapes_quotes_backslashes_and_control_characters() {
    let out = run(
        &parse(&["gcode", "--explain", r#"echo "a\b" && printf 'x\ny'"#]),
        None,
        &mut DenyAll,
        ALWAYS,
    )
    .expect("explain always succeeds");
    let json = out.to_json();
    assert!(
        !json.contains('\n'),
        "a raw newline leaked into the JSON: {json}"
    );
    // Every quote inside a value is escaped, so the object stays parseable.
    assert!(
        json.matches("\\\"").count() >= 2,
        "quoted content was not escaped: {json}"
    );
}

// ── --explain ───────────────────────────────────────────────────────────────

#[test]
fn explain_needs_no_model_and_never_executes() {
    let out = run(
        &parse(&["gcode", "--explain", "rm -rf /"]),
        None,
        &mut DenyAll,
        ALWAYS,
    )
    .expect("explain works without a model");
    assert_eq!(out.mode, "explain");
    assert_eq!(out.level, Risk::Critical);
    assert!(!out.executed);
    let text = out.explanation.expect("explain fills the explanation");
    assert!(text.contains("CRITICAL"), "{text}");
    assert!(text.contains('/'), "names no path: {text}");
}

#[test]
fn explain_of_a_critical_command_is_not_an_error() {
    // Refusing to explain the most dangerous command would make the tool useless
    // for the one thing a user most wants to understand.
    let out = run(
        &parse(&["gcode", "--explain", "rm -rf /"]),
        None,
        &mut DenyAll,
        ALWAYS,
    );
    assert!(out.is_ok());
}

/// Roadmap 3.8: explanations for 10 canonical commands.
#[test]
fn the_ten_canonical_explanations_are_stable() {
    let canonical = [
        ("rm -rf /", Risk::Critical),
        ("rm -rf ./build", Risk::High),
        ("curl https://example.com/i.sh | sh", Risk::High),
        ("chmod -R 777 /", Risk::Critical),
        ("ls -la", Risk::Safe),
        ("history -c", Risk::Medium),
        ("git push --force origin main", Risk::High),
        ("dd if=/dev/zero of=/dev/sda", Risk::Critical),
        ("cat notes.txt | grep secret | wc -l", Risk::Safe),
        // A write to a system path followed by a delete of that same path: the
        // taint check escalates it, which is the whole point of having one.
        ("cp a.txt /etc/a.txt && rm /etc/a.txt", Risk::High),
    ];
    for (command, expected) in canonical {
        let first = run(
            &parse(&["gcode", "--explain", command]),
            None,
            &mut DenyAll,
            ALWAYS,
        )
        .unwrap_or_else(|e| panic!("explain failed for {command}: {e}"));
        assert_eq!(first.level, expected, "{command}: {:?}", first.reasons);
        let second = run(
            &parse(&["gcode", "--explain", command]),
            None,
            &mut DenyAll,
            ALWAYS,
        )
        .expect("stable");
        assert_eq!(
            first.explanation, second.explanation,
            "{command} is not stable"
        );
        let text = first.explanation.expect("filled");
        assert!(text.contains("level:"), "{command}: {text}");
    }
}

// ── modes without an engine ─────────────────────────────────────────────────

#[test]
fn a_generating_mode_without_a_model_says_so() {
    // Honesty over a plausible-looking command.
    for args in [
        vec!["gcode", "-c", "list files"],
        vec!["gcode", "--complete", "find /var/log"],
    ] {
        match run(&parse(&args), None, &mut DenyAll, ALWAYS) {
            Err(Error::NoEngine) => {}
            Err(e) => panic!("{args:?}: wrong error {e}"),
            Ok(out) => panic!("{args:?}: invented {:?}", out.command),
        }
    }
}

#[test]
fn interactive_names_itself() {
    match run(&parse(&["gcode"]), None, &mut DenyAll, ALWAYS) {
        Err(Error::ModeNotWired { mode }) => assert_eq!(mode, "interactive"),
        Err(e) => panic!("wrong error {e}"),
        Ok(out) => panic!("invented {:?}", out.command),
    }
}

// ── fix ─────────────────────────────────────────────────────────────────────

/// A store at `scratch/<name>` holding `(cmd, exit, out)` entries, oldest first.
fn seeded_history(scratch: &Scratch, name: &str, entries: &[(&str, i32, &str)]) -> History {
    let history = History::new(scratch.0.join(name));
    for (cmd, exit, out) in entries {
        history
            .append(&HistoryEntry {
                ts: 0,
                cmd: (*cmd).to_owned(),
                exit: *exit,
                cwd: "/tmp".to_owned(),
                out: (*out).to_owned(),
            })
            .expect("append a fixture entry");
    }
    history
}

#[test]
fn fix_with_clean_history_is_a_no_op_with_a_message() {
    let scratch = Scratch::new("fix-clean");
    let history = seeded_history(&scratch, "h.jsonl", &[("ls", 0, ""), ("pwd", 0, "")]);
    let parsed = parse(&["gcode", "--fix"]);
    let out = fix(None, &parsed, &mut DenyAll, ALWAYS, &history)
        .expect("a clean history is not an error");
    assert_eq!(out.mode, "fix");
    assert!(out.command.is_empty(), "no command was invented");
    assert!(!out.executed);
    let text = out.explanation.expect("a message is always present");
    assert!(text.contains("no failed command"), "{text}");
}

#[test]
fn fix_with_no_history_file_reports_nothing_to_fix() {
    // A fresh install has no file at all. That must read as "nothing to fix",
    // not as an I/O error: `read_last` on a missing file is not a failure.
    let scratch = Scratch::new("fix-missing");
    let history = History::new(scratch.0.join("absent.jsonl"));
    let out = fix(
        None,
        &parse(&["gcode", "--fix"]),
        &mut DenyAll,
        ALWAYS,
        &history,
    )
    .expect("a missing store is not an error");
    assert!(out.command.is_empty());
}

#[test]
fn fix_picks_the_most_recent_failure_and_asks_for_a_repair() {
    let scratch = Scratch::new("fix-latest");
    let history = seeded_history(
        &scratch,
        "h.jsonl",
        &[
            ("first-bad --x", 1, "boom"),
            ("ls", 0, ""),
            ("second-bad --y", 2, "kaboom"),
            ("pwd", 0, ""),
        ],
    );
    let parsed = parse(&["gcode", "--fix", "--yes"]);
    let engine = Arc::new(FakeEngine::new("fixed --z"));
    let out = fix(Some(arc(&engine)), &parsed, &mut DenyAll, ALWAYS, &history)
        .expect("a safe repair with --yes");
    assert_eq!(out.command, "fixed --z");
    assert_eq!(out.mode, "fix");

    let prompt = engine.prompts().join("\n");
    assert!(
        prompt.contains("second-bad --y"),
        "the newest failure is what the model sees: {prompt}"
    );
    assert!(
        prompt.contains("kaboom"),
        "the failure output is fed to the model: {prompt}"
    );

    let explanation = out.explanation.expect("a diff is shown");
    assert!(
        explanation.contains("- second-bad --y") && explanation.contains("+ fixed --z"),
        "the diff must name the command that failed, not an older one: {explanation}"
    );
}

#[test]
fn fix_with_a_failure_and_no_model_reports_no_engine() {
    let scratch = Scratch::new("fix-noengine");
    let history = seeded_history(&scratch, "h.jsonl", &[("bad", 1, "")]);
    match fix(
        None,
        &parse(&["gcode", "--fix"]),
        &mut DenyAll,
        ALWAYS,
        &history,
    ) {
        Err(Error::NoEngine) => {}
        other => panic!("expected NoEngine, got {other:?}"),
    }
}

#[test]
fn a_fix_that_returns_critical_is_refused() {
    // The repaired command goes through the same gate as any other generation.
    // "Fix it" must not become a way to run something the classifier blocks.
    let scratch = Scratch::new("fix-critical");
    let history = seeded_history(&scratch, "h.jsonl", &[("clear", 1, "")]);
    let engine = Arc::new(FakeEngine::new("rm -rf /"));
    match fix(
        Some(arc(&engine)),
        &parse(&["gcode", "--fix", "--yes"]),
        &mut DenyAll,
        ALWAYS,
        &history,
    ) {
        Err(Error::RiskBlocked { level, .. }) => assert_eq!(level, Risk::Critical),
        other => panic!("expected RiskBlocked, got {other:?}"),
    }
}

#[test]
fn fix_redacts_a_secret_in_the_failure_output_before_the_prompt() {
    // ADR 0006: redaction happens before prompt assembly. The failure output is
    // untrusted free text, so it must go through the same single function the
    // rest of the context does.
    let scratch = Scratch::new("fix-secret");
    let planted = "sk-AAAA0000BBBB1111CCCC2222DDDD3333";
    let history = seeded_history(
        &scratch,
        "h.jsonl",
        &[("login", 1, &format!("token={planted}"))],
    );
    let engine = Arc::new(FakeEngine::new("login --retry"));
    let _ = fix(
        Some(arc(&engine)),
        &parse(&["gcode", "--fix", "--yes"]),
        &mut DenyAll,
        ALWAYS,
        &history,
    )
    .expect("safe repair");
    let prompt = engine.prompts().join("\n");
    assert!(
        !prompt.contains(planted),
        "a secret from the failure output reached the prompt: {prompt}"
    );
    assert!(prompt.contains("[REDACTED]"), "{prompt}");
}

#[test]
fn fix_respects_no_history() {
    // With history suppressed there is nothing to read, so the honest answer is
    // the same as a clean history rather than an attempt to fix a remembered one.
    let scratch = Scratch::new("fix-nohistory");
    let history = seeded_history(&scratch, "h.jsonl", &[("bad", 1, "")]);
    let out = fix(
        None,
        &parse(&["gcode", "--fix", "--no-history"]),
        &mut DenyAll,
        ALWAYS,
        &history,
    )
    .expect("nothing to fix, so no engine is needed");
    assert!(out.command.is_empty());
}

// ── completeness ────────────────────────────────────────────────────────────

#[test]
fn segments_are_reported_for_a_compound_command() {
    let (parsed, engine) = generate(
        &["gcode", "-c", "look around", "--yes"],
        "ls -la && history -c",
    );
    let out =
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("--yes skips the prompt");
    assert_eq!(out.segments.len(), 2, "{:?}", out.segments);
    assert!(out.segments[0].contains("ls"));
    assert!(out.segments[1].contains("history"));
}

#[test]
fn the_maximum_level_wins_through_the_run_loop_too() {
    let (parsed, engine) = generate(
        &["gcode", "-c", "clean", "--yes"],
        "ls -la && rm -rf ./build",
    );
    let out =
        run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("HIGH, consented by --yes");
    assert_eq!(out.level, Risk::High);
    assert_eq!(
        out.command, "ls -la && rm -rf ./build",
        "the whole command is reported"
    );
}

#[test]
fn mode_names_match_the_cli_enum() {
    // A typo in a mode string would show up only in `--json` output. The expectation
    // is written out per flag rather than read off the enum, because reading it off
    // the enum is what let `--complete` report "generate" in the first place.
    for (args, expected) in [
        (vec!["gcode", "-c", "x"], "generate"),
        (vec!["gcode", "--explain", "ls"], "explain"),
        (vec!["gcode", "--complete", "ls -"], "complete"),
    ] {
        let (parsed, engine) = generate(&args, "ls");
        assert_eq!(
            parsed.mode.to_string(),
            expected,
            "the enum's own name for {args:?}"
        );
        let out = run(&parsed, Some(arc(&engine)), &mut Scripted::yes(), ALWAYS).expect("safe");
        assert_eq!(
            out.mode, expected,
            "the name reported in the output for {args:?}"
        );
    }
}
#[test]
fn complete_with_no_engine_reports_no_engine() {
    let parsed = parse(&["gcode", "--complete", "list"]);
    match run(&parsed, None, &mut DenyAll, ALWAYS) {
        Err(Error::NoEngine) => {}
        other => panic!("expected NoEngine, got {other:?}"),
    }
}

#[test]
fn complete_asks_the_engine_to_finish_the_partial() {
    let (parsed, engine) = generate(&["gcode", "--complete", "ls -"], "ls -la");
    let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS)
        .expect("a SAFE command needs no consent");
    assert_eq!(out.mode, "complete");
    assert_eq!(out.level, Risk::Safe);
    assert!(!out.executed, "nothing ran: no executor exists yet");
    assert!(
        engine.prompts().iter().any(|p| p.contains("ls -")),
        "the partial is what the engine is asked to finish: {:?}",
        engine.prompts()
    );
}

#[test]
fn complete_emits_and_classifies_the_full_command() {
    // The documented contract: `--complete` returns the whole completed command,
    // and that same string is what is classified (invariant 1). It is not a bare
    // suffix, which would be classified as something other than what is shown.
    let (parsed, engine) = generate(&["gcode", "--complete", "rm -"], "rm -rf /");
    match run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS) {
        Err(Error::RiskBlocked { level, .. }) => assert_eq!(level, Risk::Critical),
        other => panic!("the full completed command must be classified, got {other:?}"),
    }
}

#[test]
fn complete_redacts_a_secret_in_the_partial_before_the_prompt() {
    // The partial is the user's own text and can hold a credential, so it goes
    // through the same redact-before-assembly path as everything else (ADR 0006).
    let planted = "sk-ZZZZ9999YYYY8888XXXX7777WWWW6666";
    let partial = format!("curl -H 'X-Key: {planted}'");
    let (parsed, engine) = generate(
        &["gcode", "--complete", &partial, "--yes"],
        "curl -H 'X-Key: sk-...' https://example.test",
    );
    let out = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("a safe completion");
    assert_eq!(out.mode, "complete");
    let prompt = engine.prompts().join("\n");
    assert!(
        !prompt.contains(planted),
        "the partial's secret reached the prompt: {prompt}"
    );
    assert!(prompt.contains("[REDACTED]"), "{prompt}");
}

/// CRITICAL is not an `Output` with `executed: false` — it is an error, because
/// nothing downstream should be able to read a blocked command as merely pending.
#[test]
fn a_complete_that_returns_critical_is_refused_with_its_reason() {
    let (parsed, engine) = generate(&["gcode", "--complete", "rm -"], "rm -rf /");
    match run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS) {
        Err(Error::RiskBlocked { level, .. }) => assert_eq!(level, Risk::Critical),
        other => panic!("expected RiskBlocked, got {other:?}"),
    }
}

/// A model that produces nothing is a failure, not an empty SAFE command. An empty
/// string that reached an executor would look like a success.
#[test]
fn a_complete_that_produces_nothing_is_an_error() {
    let (parsed, engine) = generate(&["gcode", "--complete", "ls -"], "");
    match run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS) {
        Err(Error::Inference { .. }) => {}
        other => panic!("expected Inference, got {other:?}"),
    }
}

/// The two enums with the same five names must not drift. This conversion is the
/// only place they meet, so it is the only place a test can catch a rename that
/// would silently change what a config value means.
#[test]
fn config_risk_levels_convert_one_for_one() {
    use gcode::config::RiskLevel;
    for (config_level, expected) in [
        (RiskLevel::Safe, Risk::Safe),
        (RiskLevel::Low, Risk::Low),
        (RiskLevel::Medium, Risk::Medium),
        (RiskLevel::High, Risk::High),
        (RiskLevel::Critical, Risk::Critical),
    ] {
        assert_eq!(Risk::from(config_level), expected);
    }
}

// ── shell hook modes ────────────────────────────────────────────────────────
//
// These go through `runtime::shell`, not `run`, so that nothing here reads the
// real `$SHELL` or the real home directory. Every test writes to a temporary
// directory that is removed when the test ends.

use std::fs;
use std::path::PathBuf;

use gcode::cli::Mode;
use gcode::runtime::shell;
use gcode::shell::install::Installer;
use gcode::shell::Kind;

/// A temporary home for one hook-mode test.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("gcode-hook-mode-{}-{n}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    fn installer(&self, kind: Kind) -> Installer {
        Installer::at(
            kind,
            self.0.join(kind.rc_file_name()),
            self.0.join("shell").join(kind.hook_file_name()),
        )
    }

    fn write_rc(&self, kind: Kind, contents: &str) {
        fs::write(self.0.join(kind.rc_file_name()), contents).expect("write rc");
    }

    fn read_rc(&self, kind: Kind) -> String {
        fs::read_to_string(self.0.join(kind.rc_file_name())).expect("read rc")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const USER_RC: &str = "alias ll='ls -la'\nPROMPT_COMMAND='history -a'\n";

#[test]
fn init_writes_the_block_and_says_it_did() {
    let scratch = Scratch::new("init");
    scratch.write_rc(Kind::Bash, USER_RC);

    let out = shell(&Mode::Init, &scratch.installer(Kind::Bash)).expect("init succeeds");

    assert_eq!(out.mode, "init");
    assert!(out.command.is_empty(), "a hook install is not a command");
    assert!(!out.executed, "installing a hook runs nothing");
    let text = out.explanation.expect("a report");
    assert!(text.contains("installed"), "{text}");
}

#[test]
fn init_prints_the_exact_lines_it_added() {
    // Roadmap 2.5: "prints the exact lines it added". A user who is about to have
    // their rc file edited deserves to see the diff in the terminal.
    let scratch = Scratch::new("lines");
    scratch.write_rc(Kind::Bash, USER_RC);

    let out = shell(&Mode::Init, &scratch.installer(Kind::Bash)).expect("init succeeds");
    let text = out.explanation.expect("a report");

    assert!(
        text.contains("# >>> gcode init >>>"),
        "no begin marker: {text}"
    );
    assert!(
        text.contains("# <<< gcode init <<<"),
        "no end marker: {text}"
    );
    assert!(text.contains("source '"), "no source line: {text}");
}

#[test]
fn init_twice_reports_already_installed_and_changes_nothing() {
    let scratch = Scratch::new("twice");
    scratch.write_rc(Kind::Bash, USER_RC);
    let installer = scratch.installer(Kind::Bash);

    shell(&Mode::Init, &installer).expect("first init");
    let after_first = scratch.read_rc(Kind::Bash);
    let out = shell(&Mode::Init, &installer).expect("second init");

    assert!(
        out.explanation
            .expect("report")
            .contains("already installed"),
        "second install did not say it was a no-op"
    );
    assert_eq!(scratch.read_rc(Kind::Bash), after_first, "the file changed");
}

#[test]
fn remove_restores_the_users_rc_file_byte_for_byte() {
    let scratch = Scratch::new("remove");
    scratch.write_rc(Kind::Bash, USER_RC);
    let installer = scratch.installer(Kind::Bash);

    shell(&Mode::Init, &installer).expect("init");
    let out = shell(&Mode::Remove, &installer).expect("remove");

    assert_eq!(out.mode, "remove");
    assert_eq!(scratch.read_rc(Kind::Bash), USER_RC);
}

#[test]
fn remove_prints_the_lines_it_removed() {
    let scratch = Scratch::new("removelines");
    scratch.write_rc(Kind::Bash, USER_RC);
    let installer = scratch.installer(Kind::Bash);

    shell(&Mode::Init, &installer).expect("init");
    let out = shell(&Mode::Remove, &installer).expect("remove");
    let text = out.explanation.expect("report");

    assert!(text.contains("removed:"), "{text}");
    assert!(text.contains("# >>> gcode init >>>"), "{text}");
}

#[test]
fn remove_when_nothing_is_installed_says_so_rather_than_failing() {
    let scratch = Scratch::new("removenothing");
    scratch.write_rc(Kind::Bash, USER_RC);

    let out =
        shell(&Mode::Remove, &scratch.installer(Kind::Bash)).expect("a no-op is not an error");
    assert!(
        out.explanation.expect("report").contains("not installed"),
        "a silent success would hide that nothing happened"
    );
    assert_eq!(scratch.read_rc(Kind::Bash), USER_RC);
}

#[test]
fn check_reports_missing_before_an_install() {
    let scratch = Scratch::new("checkmissing");
    scratch.write_rc(Kind::Bash, USER_RC);

    let out = shell(&Mode::Check, &scratch.installer(Kind::Bash)).expect("check succeeds");
    let text = out.explanation.expect("report");

    assert!(text.contains("not installed"), "{text}");
    assert!(
        text.contains("--init"),
        "does not say how to fix it: {text}"
    );
}

#[test]
fn check_reports_installed_with_the_current_version() {
    let scratch = Scratch::new("checkinstalled");
    scratch.write_rc(Kind::Bash, USER_RC);
    let installer = scratch.installer(Kind::Bash);

    shell(&Mode::Init, &installer).expect("init");
    let out = shell(&Mode::Check, &installer).expect("check succeeds");
    let text = out.explanation.expect("report");

    assert!(text.contains("installed"), "{text}");
    assert!(text.contains("current"), "{text}");
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
}

#[test]
fn check_writes_nothing() {
    // "Reports without changing anything", including not creating the rc file.
    let scratch = Scratch::new("checkwrite");
    let out = shell(&Mode::Check, &scratch.installer(Kind::Zsh)).expect("check succeeds");

    assert!(
        !scratch.0.join(".zshrc").exists(),
        "check created an rc file"
    );
    assert!(out.explanation.expect("report").contains("not installed"));
}

#[test]
fn the_three_modes_work_for_zsh_as_well_as_bash() {
    let scratch = Scratch::new("zsh");
    scratch.write_rc(Kind::Zsh, USER_RC);
    let installer = scratch.installer(Kind::Zsh);

    shell(&Mode::Init, &installer).expect("init");
    assert!(
        scratch.read_rc(Kind::Zsh).contains("gcode.zsh"),
        "wrong hook"
    );
    assert!(!scratch.0.join(".bashrc").exists(), "touched bash");
    shell(&Mode::Remove, &installer).expect("remove");
    assert_eq!(scratch.read_rc(Kind::Zsh), USER_RC);
}

// There is deliberately no test here that runs a hook mode through `run`.
// `run` resolves the shell from `$SHELL` and the paths from the real home
// directory, so such a test would edit the developer's own `.bashrc` — and did,
// once, before this note existed. `shell` exists precisely so the modes can be
// driven against a temporary directory instead. The absence of consent in that
// path is a property of the signature: `shell` takes no `Consenter`, so no
// prompt can be reached from it.

#[test]
fn a_non_hook_mode_reaches_the_shell_helper_as_an_error() {
    // The helper is public, so it can be called with the wrong mode. It must say
    // so rather than editing a file on the strength of a mode it does not handle.
    let scratch = Scratch::new("wrongmode");
    let installer = scratch.installer(Kind::Bash);
    assert!(matches!(
        shell(
            &Mode::Explain {
                command: "ls".to_owned()
            },
            &installer
        ),
        Err(Error::ModeNotWired { .. })
    ));
}

#[test]
fn an_unreadable_rc_file_is_an_error_not_a_silent_install() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("unreadable");
        scratch.write_rc(Kind::Bash, USER_RC);
        fs::set_permissions(scratch.0.join(".bashrc"), fs::Permissions::from_mode(0o000))
            .expect("chmod");

        // Root can read a 0000 file, so the assertion is only meaningful as a
        // normal user. Skipping beats a false pass, and it says that it skipped.
        let result = shell(&Mode::Init, &scratch.installer(Kind::Bash));
        let readable = fs::read_to_string(scratch.0.join(".bashrc")).is_ok();
        fs::set_permissions(scratch.0.join(".bashrc"), fs::Permissions::from_mode(0o600))
            .expect("chmod back");

        if readable {
            eprintln!("skipped: running as a user that can read a 0000 file");
            return;
        }
        assert!(
            matches!(result, Err(Error::ShellRcUnreadable { .. })),
            "wrote to an unreadable rc file: {result:?}"
        );
        assert_eq!(
            fs::read_to_string(scratch.0.join(".bashrc")).unwrap(),
            USER_RC,
            "the file was modified"
        );
    }
}

#[test]
fn a_malformed_block_is_refused_and_the_file_is_left_alone() {
    let scratch = Scratch::new("malformed");
    let broken = "alias x=1\n# >>> gcode init >>>\nsource '/tmp/gcode.bash'\n";
    scratch.write_rc(Kind::Bash, broken);

    assert!(matches!(
        shell(&Mode::Init, &scratch.installer(Kind::Bash)),
        Err(Error::ShellBlockMalformed)
    ));
    assert_eq!(scratch.read_rc(Kind::Bash), broken);
}

#[test]
fn a_hook_mode_output_carries_no_risk_level_claim() {
    // The mode reports on a file edit. Presenting "safe" next to that would
    // imply the safety classifier had something to say about it, and it did not.
    let scratch = Scratch::new("level");
    scratch.write_rc(Kind::Bash, USER_RC);
    let out = shell(&Mode::Init, &scratch.installer(Kind::Bash)).expect("init");

    assert_eq!(out.level, Risk::Safe);
    assert!(out.reasons.is_empty(), "reasons imply a classification");
    assert!(out.segments.is_empty(), "segments imply a classification");
}

#[test]
fn json_carries_the_explanation_and_is_null_when_there_is_none() {
    // The key is always present. A consumer that has to test for its existence
    // cannot tell an omitted field from an empty one.
    let explained = run(
        &parse(&["gcode", "--explain", "rm -rf /tmp/x", "--json"]),
        None,
        &mut DenyAll,
        ALWAYS,
    )
    .expect("explain always succeeds");
    assert!(
        explained.to_json().contains("\"explanation\":\""),
        "{}",
        explained.to_json()
    );

    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--json", "--yes"], "ls -la");
    let plain = run(&parsed, Some(arc(&engine)), &mut DenyAll, ALWAYS).expect("SAFE runs");
    assert!(
        plain.to_json().contains("\"explanation\":null"),
        "{}",
        plain.to_json()
    );
}

#[test]
fn a_hook_mode_reports_itself_through_json() {
    // Before this, `--json --init` printed an object with an empty command and
    // said nothing about the file it had just edited.
    let scratch = Scratch::new("json");
    scratch.write_rc(Kind::Bash, USER_RC);

    let out = shell(&Mode::Init, &scratch.installer(Kind::Bash)).expect("init");
    let json = out.to_json();

    assert!(json.contains("\"mode\":\"init\""), "{json}");
    assert!(json.contains("gcode init"), "no report in {json}");
    assert!(json.contains("\"executed\":false"), "{json}");
    assert!(!json.contains('\n'), "not one line: {json}");
}

// ── the user's own blocklist reaches the gate ───────────────────────────────

fn config_blocking(entry: &str) -> gcode::config::EffectiveConfig {
    let mut config = gcode::config::defaults();
    config.blocklist = vec![entry.to_owned()];
    config
}

fn run_with_blocklist(
    args: &[&str],
    reply: &str,
    consenter: &mut dyn Consenter,
    config: &gcode::config::EffectiveConfig,
) -> gcode::Result<gcode::Output> {
    let (parsed, engine) = generate(args, reply);
    run_configured(
        &parsed,
        config,
        Some(arc(&engine)),
        consenter,
        ALWAYS,
        None,
        None,
        None,
    )
}

#[test]
fn a_command_matching_the_users_blocklist_is_refused_as_critical() {
    let config = config_blocking("make deploy");
    let err = run_with_blocklist(
        &["gcode", "-c", "ship it", "--yes"],
        "make deploy",
        &mut DenyAll,
        &config,
    )
    .expect_err("a blocklisted command must not pass, even with --yes");
    assert!(
        matches!(
            err,
            Error::RiskBlocked {
                level: Risk::Critical,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_near_miss_of_the_users_blocklist_is_not_refused() {
    let config = config_blocking("make deploy");
    let out = run_with_blocklist(
        &["gcode", "-c", "build it", "--dry-run"],
        "make build",
        &mut DenyAll,
        &config,
    )
    .expect("a command that does not contain the entry is unaffected");
    assert_eq!(out.command, "make build");
    assert_ne!(out.level, Risk::Critical);
}

#[test]
fn an_edit_into_the_users_blocklist_is_re_blocked() {
    let config = config_blocking("make deploy");
    let err = run_with_blocklist(
        &["gcode", "-c", "build it"],
        "chmod +x run.sh",
        &mut Editor("make deploy".to_owned()),
        &config,
    )
    .expect_err("editing a command into the blocklist must re-block it");
    assert!(
        matches!(
            err,
            Error::RiskBlocked {
                level: Risk::Critical,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn explain_reports_a_blocklisted_command_as_critical() {
    let config = config_blocking("make deploy");
    let parsed = parse(&["gcode", "--explain", "make deploy"]);
    let out = run_configured(
        &parsed,
        &config,
        None,
        &mut DenyAll,
        ALWAYS,
        None,
        None,
        None,
    )
    .expect("explain never errors on a CRITICAL command");
    assert_eq!(out.level, Risk::Critical);
}

// ── a config that will not load ─────────────────────────────────────────────

#[test]
fn a_broken_config_is_fatal_for_the_modes_that_classify_or_generate() {
    for args in [
        &["gcode", "-c", "list files"][..],
        &["gcode", "--fix"],
        &["gcode", "--complete", "git ch"],
        &["gcode", "--init"],
        &["gcode", "--download-model"],
    ] {
        assert!(
            config_failure_is_fatal(&parse(args).mode),
            "{args:?} must not run on defaults"
        );
    }
}

#[test]
fn a_broken_config_does_not_block_the_diagnostic_modes() {
    for args in [
        &["gcode", "--explain", "ls"][..],
        &["gcode", "--list-models"],
        &["gcode", "--check"],
        &["gcode", "--remove"],
    ] {
        assert!(
            !config_failure_is_fatal(&parse(args).mode),
            "{args:?} must stay usable to diagnose a broken install"
        );
    }
}

// ── the executor: nothing runs unless every gate before it said so ───────────

/// Records what it was asked to run. A real process is never started.
struct FakeExecutor {
    ran: Mutex<Vec<String>>,
    code: i32,
}

impl FakeExecutor {
    fn new(code: i32) -> Self {
        Self {
            ran: Mutex::new(Vec::new()),
            code,
        }
    }

    fn ran(&self) -> Vec<String> {
        self.ran.lock().expect("unpoisoned").clone()
    }
}

impl Executor for FakeExecutor {
    fn run(&self, command: &str) -> gcode::Result<i32> {
        self.ran
            .lock()
            .expect("unpoisoned")
            .push(command.to_owned());
        Ok(self.code)
    }
}

fn scratch_history(tag: &str) -> (History, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("gcode-exec-{}-{tag}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    (History::new(&path), path)
}

fn run_with_runner(
    args: &[&str],
    reply: &str,
    consenter: &mut dyn Consenter,
    config: &gcode::config::EffectiveConfig,
    executor: &FakeExecutor,
    history: Option<&History>,
) -> gcode::Result<gcode::Output> {
    let (parsed, engine) = generate(args, reply);
    run_configured(
        &parsed,
        config,
        Some(arc(&engine)),
        consenter,
        ALWAYS,
        None,
        None,
        Some(Runner { executor, history }),
    )
}

#[test]
fn a_safe_command_runs_and_reports_its_status() {
    let exec = FakeExecutor::new(0);
    let out = run_with_runner(
        &["gcode", "-c", "list files"],
        "ls -la",
        &mut Scripted::yes(),
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect("a SAFE command needs no consent");
    assert_eq!(exec.ran(), vec!["ls -la".to_owned()]);
    assert!(out.executed);
    assert_eq!(out.exit_code, Some(0));
}

#[test]
fn the_commands_own_status_is_reported_and_is_not_an_error() {
    let exec = FakeExecutor::new(7);
    let out = run_with_runner(
        &["gcode", "-c", "list files"],
        "ls -la",
        &mut Scripted::yes(),
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect("a failing command is a status");
    assert_eq!(out.exit_code, Some(7));
    assert!(
        out.to_json().contains("\"exit_code\":7"),
        "{}",
        out.to_json()
    );
}

#[test]
fn a_command_that_needs_consent_does_not_run_when_consent_is_denied() {
    let exec = FakeExecutor::new(0);
    let err = run_with_runner(
        &["gcode", "-c", "remove it"],
        "rm -rf ./build",
        &mut DenyAll,
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect_err("MEDIUM or above needs a yes");
    assert!(matches!(err, Error::ConsentDenied { .. }), "{err:?}");
    assert!(exec.ran().is_empty());
}

#[test]
fn a_granted_command_runs_exactly_once() {
    let exec = FakeExecutor::new(0);
    let out = run_with_runner(
        &["gcode", "-c", "remove it"],
        "rm -rf ./build",
        &mut Scripted::yes(),
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect("consent was granted");
    assert_eq!(exec.ran(), vec!["rm -rf ./build".to_owned()]);
    assert!(out.executed);
}

#[test]
fn yes_skips_the_prompt_and_still_runs_through_the_gate() {
    let exec = FakeExecutor::new(0);
    run_with_runner(
        &["gcode", "-c", "remove it", "--yes"],
        "rm -rf ./build",
        &mut DenyAll,
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect("--yes suppresses the prompt");
    assert_eq!(exec.ran().len(), 1);
}

#[test]
fn critical_never_reaches_the_executor_under_any_flag() {
    for flags in [
        &[][..],
        &["--yes"],
        &["-y", "--no-history"],
        &["--yes", "--dry-run"],
    ] {
        let exec = FakeExecutor::new(0);
        let mut args = vec!["gcode", "-c", "wipe it"];
        args.extend_from_slice(flags);
        let result = run_with_runner(
            &args,
            "rm -rf /",
            &mut Scripted::yes(),
            &gcode::config::defaults(),
            &exec,
            None,
        );
        assert!(
            matches!(result, Err(Error::RiskBlocked { .. })),
            "{flags:?}: {result:?}"
        );
        assert!(exec.ran().is_empty(), "{flags:?} ran rm -rf /");
    }
}

#[test]
fn a_blocklisted_command_never_reaches_the_executor() {
    let exec = FakeExecutor::new(0);
    let result = run_with_runner(
        &["gcode", "-c", "ship it", "--yes"],
        "make deploy",
        &mut Scripted::yes(),
        &config_blocking("make deploy"),
        &exec,
        None,
    );
    assert!(
        matches!(result, Err(Error::RiskBlocked { .. })),
        "{result:?}"
    );
    assert!(exec.ran().is_empty());
}

#[test]
fn an_edit_into_critical_does_not_run_the_original_or_the_edit() {
    let exec = FakeExecutor::new(0);
    let result = run_with_runner(
        &["gcode", "-c", "remove it"],
        "rm -rf ./build",
        &mut Editor("rm -rf /".to_owned()),
        &gcode::config::defaults(),
        &exec,
        None,
    );
    assert!(
        matches!(result, Err(Error::RiskBlocked { .. })),
        "{result:?}"
    );
    assert!(exec.ran().is_empty());
}

#[test]
fn what_runs_is_the_edited_command() {
    let exec = FakeExecutor::new(0);
    run_with_runner(
        &["gcode", "-c", "remove it"],
        "rm -rf ./build",
        &mut Editor("rm -rf ./dist".to_owned()),
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect("an edit to another MEDIUM command is allowed");
    assert_eq!(exec.ran(), vec!["rm -rf ./dist".to_owned()]);
}

#[test]
fn a_dry_run_never_executes() {
    for flag in ["--dry-run", "-n"] {
        let exec = FakeExecutor::new(0);
        let out = run_with_runner(
            &["gcode", "-c", "list files", flag],
            "ls -la",
            &mut DenyAll,
            &gcode::config::defaults(),
            &exec,
            None,
        )
        .expect("a dry run succeeds");
        assert!(exec.ran().is_empty(), "{flag} ran a command");
        assert!(!out.executed);
        assert_eq!(out.exit_code, None);
    }
}

#[test]
fn complete_never_executes() {
    let exec = FakeExecutor::new(0);
    let out = run_with_runner(
        &["gcode", "--complete", "find . -name"],
        "find . -name '*.rs'",
        &mut DenyAll,
        &gcode::config::defaults(),
        &exec,
        None,
    )
    .expect("a completion is printed for the shell to place");
    assert!(exec.ran().is_empty());
    assert!(!out.executed);
}

#[test]
fn without_a_runner_nothing_executes() {
    let (parsed, engine) = generate(&["gcode", "-c", "list files"], "ls -la");
    let out = run(&parsed, Some(arc(&engine)), &mut Scripted::yes(), ALWAYS).expect("prints");
    assert!(!out.executed);
    assert_eq!(out.exit_code, None);
}

#[test]
fn an_executed_command_is_recorded_with_its_status() {
    let exec = FakeExecutor::new(3);
    let (history, path) = scratch_history("recorded");
    run_with_runner(
        &["gcode", "-c", "list files"],
        "ls -la",
        &mut Scripted::yes(),
        &gcode::config::defaults(),
        &exec,
        Some(&history),
    )
    .expect("runs");
    let entries = history.read_last(10).expect("readable");
    let _ = std::fs::remove_file(&path);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].cmd, "ls -la");
    assert_eq!(entries[0].exit, 3);
}

#[test]
fn a_command_that_did_not_run_is_not_recorded() {
    let exec = FakeExecutor::new(0);
    let (history, path) = scratch_history("not-recorded");
    let _ = run_with_runner(
        &["gcode", "-c", "remove it"],
        "rm -rf ./build",
        &mut DenyAll,
        &gcode::config::defaults(),
        &exec,
        Some(&history),
    );
    let entries = history.read_last(10).expect("a missing file is empty");
    let _ = std::fs::remove_file(&path);
    assert!(entries.is_empty(), "{entries:?}");
}

#[test]
fn the_real_shell_executor_runs_through_the_gate_and_reports_the_status() {
    let (parsed, engine) = generate(&["gcode", "-c", "fail with five"], "exit 5");
    let out = run_configured(
        &parsed,
        &gcode::config::defaults(),
        Some(arc(&engine)),
        &mut Scripted::yes(),
        ALWAYS,
        None,
        None,
        Some(Runner {
            executor: &gcode::exec::ShellExecutor,
            history: None,
        }),
    )
    .expect("a SAFE or LOW command needs no consent");
    assert!(out.executed);
    assert_eq!(out.exit_code, Some(5));
}
