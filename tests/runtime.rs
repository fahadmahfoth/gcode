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

use std::sync::Mutex;

use gcode::cli::{self, Mode, Parsed};
use gcode::error::Error;
use gcode::inference::{EngineInfo, GenParams, InferenceEngine};
use gcode::runtime::{run, Consenter, Decision, DenyAll};
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
const ALWAYS: Risk = Risk::Medium;

fn generate(args: &[&str], reply: &str) -> (Parsed, FakeEngine) {
    let parsed = parse(args);
    let engine = FakeEngine::new(reply);
    (parsed, engine)
}

// ── run() with a FakeEngine ─────────────────────────────────────────────────

/// The roadmap's headline test: `run()` with a `FakeEngine` produces the
/// expected `Output`.
#[test]
fn run_with_a_fake_engine_produces_the_expected_output() {
    let (parsed, engine) = generate(&["gcode", "-c", "list all files"], "ls -la");
    let out =
        run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("a SAFE command needs no consent");
    assert_eq!(out.command, "ls -la");
    assert_eq!(out.level, Risk::Safe);
    assert_eq!(out.mode, "generate");
    assert!(!out.executed, "nothing ran: no executor exists yet");
    assert_eq!(engine.prompts(), vec!["list all files".to_owned()]);
}

#[test]
fn a_safe_command_needs_no_consenter_at_all() {
    // `DenyAll` cannot consent to anything, so a SAFE command completing under it
    // proves consent is genuinely not being asked for.
    let (parsed, engine) = generate(&["gcode", "-c", "list files"], "ls");
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("safe commands run");
    assert_eq!(out.level, Risk::Safe);
}

#[test]
fn a_generated_command_is_classified_not_trusted() {
    // The engine says "rm -rf ./build". The tool does not take its word for it.
    let (parsed, engine) = generate(
        &["gcode", "-c", "clean everything", "--yes"],
        "rm -rf ./build",
    );
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("HIGH is not blocked");
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
        match run(&parsed, Some(&engine), &mut DenyAll, ALWAYS) {
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
        let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS);
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
                run(&parsed, Some(&engine), &mut DenyAll, ALWAYS),
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
        run(&parsed, Some(&engine), &mut yes, ALWAYS),
        Err(Error::RiskBlocked { .. })
    ));
}

// ── consent, --yes, and the pipe ────────────────────────────────────────────

/// Invariant 4: non-interactive stdin fails closed.
#[test]
fn a_pipe_cannot_run_a_command_that_needs_consent() {
    for reply in ["rm -rf ./build", "history -c", "kill -9 1"] {
        let (parsed, engine) = generate(&["gcode", "-c", "do a thing"], reply);
        let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS);
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
    let out = run(&parsed, Some(&engine), &mut yes, ALWAYS)
        .expect("a HIGH command may run once confirmed");
    assert_eq!(out.level, Risk::High);
    assert!(!out.executed, "still no executor in Phase 1.8");
}

/// Invariant 5: `--yes` suppresses the prompt and nothing else.
#[test]
fn yes_replaces_the_prompt_but_not_the_classification() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--yes"], "rm -rf ./build");
    let mut denied = DenyAll;
    let out = run(&parsed, Some(&engine), &mut denied, ALWAYS).expect("--yes skips the prompt");
    assert_eq!(out.level, Risk::High, "the level is unchanged by --yes");
    assert!(!out.reasons.is_empty(), "the reasons are still reported");
}

#[test]
fn yes_still_prints_the_level_and_reasons() {
    // Roadmap 3.7: "still classifies, still prints the level and reasons".
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--yes"], "history -c");
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("--yes runs it");
    assert_eq!(out.level, Risk::Medium);
    assert!(!out.reasons.is_empty());
}

/// Invariant 3: an edit is re-classified. `e` turning a HIGH command into a
/// CRITICAL one is re-blocked, not run.
#[test]
fn an_edit_that_turns_high_into_critical_is_re_blocked() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean"], "rm -rf ./build");
    let mut editor = Editor("rm -rf /".to_owned());
    match run(&parsed, Some(&engine), &mut editor, ALWAYS) {
        Err(Error::RiskBlocked { level, .. }) => assert_eq!(level, Risk::Critical),
        Ok(out) => panic!("the edited command ran: {out:?}"),
        Err(e) => panic!("wrong error: {e}"),
    }
}

#[test]
fn an_edit_that_stays_safe_is_classified_as_the_new_command() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean"], "rm -rf ./build");
    let mut editor = Editor("ls -la".to_owned());
    let out =
        run(&parsed, Some(&engine), &mut editor, ALWAYS).expect("a SAFE edit needs no consent");
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
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("a dry run always succeeds");
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
        run(&parsed, Some(&engine), &mut DenyAll, ALWAYS),
        Err(Error::RiskBlocked { .. })
    ));
}

#[test]
fn the_no_alias_behaves_identically_to_dry_run() {
    for flag in ["-n", "--no", "--dry-run"] {
        let (parsed, engine) = generate(&["gcode", "-c", "clean", flag], "history -c");
        let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS)
            .unwrap_or_else(|e| panic!("{flag}: {e}"));
        assert!(!out.executed, "{flag} executed");
    }
}

// ── --json ──────────────────────────────────────────────────────────────────

#[test]
fn json_is_a_single_object_on_one_line() {
    let (parsed, engine) = generate(&["gcode", "-c", "clean", "--json"], "ls -la");
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("SAFE needs no consent");
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
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("--json --yes runs it");
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
        run(&parsed, Some(&engine), &mut DenyAll, ALWAYS),
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
fn fix_and_interactive_name_themselves() {
    for args in [vec!["gcode", "--fix"], vec!["gcode"]] {
        match run(&parse(&args), None, &mut DenyAll, ALWAYS) {
            Err(Error::ModeNotWired { mode }) => {
                assert!(!mode.is_empty(), "unnamed mode for {args:?}");
            }
            Err(e) => panic!("{args:?}: wrong error {e}"),
            Ok(out) => panic!("{args:?}: invented {:?}", out.command),
        }
    }
    // The mode name in the error is the one a user would type.
    let err = run(&parse(&["gcode", "--fix"]), None, &mut DenyAll, ALWAYS).unwrap_err();
    assert!(err.to_string().contains("fix"), "{err}");
}

// ── completeness ────────────────────────────────────────────────────────────

#[test]
fn segments_are_reported_for_a_compound_command() {
    let (parsed, engine) = generate(
        &["gcode", "-c", "look around", "--yes"],
        "ls -la && history -c",
    );
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("--yes skips the prompt");
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
    let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("HIGH, consented by --yes");
    assert_eq!(out.level, Risk::High);
    assert_eq!(
        out.command, "ls -la && rm -rf ./build",
        "the whole command is reported"
    );
}

#[test]
fn mode_names_match_the_cli_enum() {
    // A typo in a mode string would show up only in `--json` output.
    for args in [vec!["gcode", "-c", "x"], vec!["gcode", "--explain", "ls"]] {
        let parsed = parse(&args);
        let expected = match &parsed.mode {
            Mode::Generate { .. } => "generate",
            Mode::Explain { .. } => "explain",
            _ => unreachable!("not used here"),
        };
        let (parsed, engine) = generate(&args, "ls");
        let out = run(&parsed, Some(&engine), &mut DenyAll, ALWAYS).expect("safe");
        assert_eq!(out.mode, expected);
    }
}
