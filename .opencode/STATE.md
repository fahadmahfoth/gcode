# STATE — machine-readable build state

> **Maintained by the OpenCode agent.** This file is the quick pointer; the
> human-readable contract is [../docs/ROADMAP.md](../docs/ROADMAP.md).
> Last synced by hand: 2026-10-03.

```yaml
phase: 1
phase_name: Core inference pipeline
phase_status: in_progress
first_incomplete_task: 1.5

# Phase 1.5 (the sampler) and 1.6 (the grammar) are the only phase-1 tasks left;
# both need llama-cpp-rs, a C++ toolchain, and real weights, so they are blocked
# on the model and disk, not on code. 1.7, 1.8, and the whole of phase 2 are done.
#
# Phase 3 was started ahead of 1.5 because it is pure Rust, has no dependency on
# the model, and gates everything that later runs a command.
#
# 3.1-3.5, 3.7, and 3.8 are written, tested, and passing. 3.6 has its keymap, the
# default-to-no behaviour, the non-interactive refusal, re-prompting after an
# edit, the clipboard key `c` via a platform tool (ADR 0020), and Ctrl+C resolved
# as the process default with no handler (ADR 0022). The only open phase-3 item is
# the MEDIUM+ cost estimate, which needs a model to estimate. Phase 3 stays
# in_progress, not done.

# Executor (ADR 0023): src/exec/runner.rs, the gate's Runner, history append and
# exit_code are done and tested against a FakeExecutor and the real ShellExecutor.
# main wires it for an interactive terminal only. It cannot be run end to end until
# the engine (1.5) exists.

# LlamaEngine (1.5) is written and wired under the inference feature but has never
# run against real weights: do not tick 1.5 until tests/engine.rs passes with
# GCODE_TEST_MODEL. generate/complete prompts still carry Context::default().

# ADR 0024: the default always_confirm is SAFE, so every command is asked about.

# Phases, in order. `next` is the only actionable one.
phases:
  - id: 0
    name: "Repository foundation"
    # Verified on macOS aarch64 and, 2026-10-03, Linux x86_64: scripts/ci.sh
    # exits 0 (1 skip: the release build, refused by design while a registry
    # entry is unverified). The only open item is that release build, which needs
    # one real model download (blocked here: huggingface.co is not on the
    # network allowlist). GitHub Actions is not used (ADR 0019).
    status: in_progress
    next: false
  - id: 1
    name: "Core inference pipeline"
    # The model registry now holds three real, commit-pinned entries (ADR 0018),
    # but all are `verified = false`: the hashes are published by HuggingFace and
    # have not been confirmed by a local download. A release build is refused
    # until then. 1.4 is now finished: --list-models prints the registry and
    # --download-model [NAME] drives a real HTTPS transport (ureq behind the
    # `download` feature, ADR 0021), with stderr progress and resume. 1.5 (sampler)
    # and 1.6 (grammar) still need llama-cpp-rs, a C++ toolchain, real weights, and
    # disk.
    status: in_progress
    next: true
  - id: 2
    name: "Shell integration & history"
    # 2.1 through 2.6 are written and verified: the JSONL store (29 tests), the
    # environment context (20 tests), the bash hook (27 tests) and zsh hook
    # (24 tests, each sourcing a real shell), the --init/--check/--remove
    # installer (45 tests in src/shell/tests.rs plus the hook-mode cases in
    # tests/runtime.rs), and --fix (7 tests in tests/runtime.rs). The hooks write
    # the store and --fix now reads it; before 2.6 it was write-only. 2.6 is done:
    # --fix repairs the newest failed entry, and --complete emits and classifies
    # the full completed command (the "return only the continuation" wording in
    # ROADMAP 2.6 was reconciled against USAGE.md/gcode.1/plan.md and invariant 1,
    # and the partial now goes through build_prompt for redaction). Nothing calls
    # EnvSnapshot::collect: the run loop builds no environment Context yet, so
    # build_prompt uses Context::default.
    status: in_progress
  - id: 3
    name: "Safety layer"
    # 3.1-3.5, 3.7, 3.8 written and verified, and 3.6 has its keymap, the
    # default-to-no behaviour, the non-interactive refusal, re-prompting after an
    # edit, the clipboard key `c` (ADR 0020), and Ctrl+C as the process default
    # (ADR 0022). The only open item is the MEDIUM+ cost estimate, which needs a
    # model. Coverage is measured and above target.
    status: in_progress
  - id: 4
    name: "Packaging"
    status: blocked
  - id: 5
    name: "Distribution & CI/CD"
    status: blocked
  - id: 6
    name: "Polish & launch"
    status: blocked
  - id: 7
    name: "Reliability hardening"
    status: planned
  - id: 8
    name: "Shell coverage: fish, nushell"
    status: planned
  - id: 9
    name: "Agentic multi-step execution"
    status: planned
  - id: 10
    name: "WASM plugin system"
    status: planned

code_written: true
tests_written: true
# Measured with `cargo llvm-cov --summary-only --all-features`. Workspace total is
# 91.1 % of lines / 88.5 % of regions (91.06 % and 88.53 % on the 1.4/3.6 run).
# The figure moved down from 92.7 % because the new code lives in paths the suite
# cannot reach: `main.rs` and `ui/progress.rs` are process-level and 0 % in a
# library run, and `runtime.rs` lost a few points to the new download glue. Still
# well above the 85 % target. Per module: context/env.rs 96.34 % lines;
# context/prompt.rs 97.95 %; model/download.rs 88.57 % (the HTTP transport branch
# is not exercised offline); model/registry.rs 96.11 %; shell/install.rs 78.08 %
# (the lowest module: file-writing branches need a real disk error);
# context/history.rs 76.79 % (same IO-failure reason); runtime.rs 82.63 %;
# ui/prompt.rs 80.33 %; safety/patterns.rs 98.33 %; safety/classifier.rs 99.20 %;
# safety/mod.rs 100 %.
coverage_measured: 91.06
coverage_target: 85

# Everything that needs a human. Empty means nothing is blocked on the human.
needs_human:
  - >-
    The model registry now holds three real, commit-pinned entries, but all are
    `verified = false`. The sha256 values are the ones HuggingFace publishes for
    the LFS objects; they have NOT been confirmed by downloading the files here.
    ADR 0010 requires a download before a checksum is trusted, and build.rs now
    refuses a release build while any entry is unverified. Debug builds and the
    test suite are unaffected. The user must free enough disk, run
    `gcode --download-model`, compare the digest, and set `verified = true`.
  - >-
    Disk is tight: 2.3 GiB free on a 99%-full volume. Phase 1.5's LlamaEngine
    needs llama-cpp-rs, a C++ toolchain, and real weights, and a C++ build plus
    several GB of weights will not fit in 2.3 GiB. The user should free 15-20
    GiB before that task. Until then 1.5's trait, post-processing, and timeout
    are complete and tested against FakeEngine, and only the sampler is
    outstanding.
  - >-
    A decision is needed on the release signing mechanism. Keyless Sigstore via
    GitHub OIDC is no longer available because the project left GitHub Actions
    (ADR 0019). No release can be signed until a replacement is chosen and
    documented in a new ADR.
# Things that will need a human later, so nobody is surprised at the end.
needs_human_later:
  - AUR account
  - Homebrew tap
  - COPR project
  - APT GPG key (maintainer custody)
  - get.gcode.dev domain
  - Public launch posting accounts

# Verified present. See `last_verified` for what was actually run against it.
toolchain_present: true

# Compiled, and `--locked` clean. Direct dependencies: thiserror, anyhow, dirs,
# clap, toml, serde, sha2, serde_json, and an optional ureq (the `download`
# feature, ADR 0021) — ureq is named only in src/model/download.rs. `inference`
# stays empty until Phase 1.5/1.6 attach llama-cpp-rs and llguidance.
manifest_written: true
manifest_compiled: true
platforms_verified:
  - macOS aarch64 (rustc 1.98.1)
platforms_unverified:
  - Linux x86_64
last_verified: >-
  macOS aarch64, rustc 1.98.1, after --list-models, the clipboard key `c`
  (ADR 0020), and the HTTPS downloader plus --download-model (ADR 0021):
  `./scripts/ci.sh` exits 0 — fmt clean, clippy `-D warnings` clean,
  `cargo test --all-features --locked` 612 passed / 0 failed (466 lib,
  14 tests/api.rs, 27 tests/hook_bash.rs, 24 tests/hook_zsh.rs, 59 tests/runtime.rs,
  19 tests/safety.rs incl. the 10 000-command fuzz, 3 doc), `cargo build --locked`
  passed, the policy greps pass (network confined to src/model/download.rs), the
  credential scan matches only the two reviewed false positives,
  `mandoc -T lint docs/gcode.1` clean, and the documentation link check passes.
  `cargo build --release --locked` is skipped by the `verified = false` gate;
  `cargo audit` and `cargo deny` are not installed.
  `cargo llvm-cov --summary-only --all-features` reports 91.06 % lines /
  88.53 % regions; model/registry.rs 96.11 % lines; model/download.rs 88.57 %;
  runtime.rs 82.63 %.

older_previous_last_verified: "macOS aarch64, rustc 1.98.1, after Phase 1.7: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo test` 223 passed / 0 failed; `cargo build --locked` passed; MSRV audit clean over 65 locked packages; `mandoc -T lint docs/gcode.1` clean; secret scan clean; `cargo build --release --locked` failed by design on the placeholder registry"

last_command: "scripts/ci.sh (fmt, clippy, cargo test --all-features, cargo build --locked, policy greps, secret scan, mandoc, doc-link check), cargo test --all-features, cargo llvm-cov --summary-only --all-features, python3 scripts/check-doc-links.py, and git branch/status checks"

```

### Known open items, not yet recorded above

- There is no GitHub Actions in this repository. The account is locked for a
  billing reason, so the workflows were removed and CI became `scripts/ci.sh`,
  run locally (ADR 0019). "CI is green" now means that script exits zero. The
  consequences — no Linux runner, no automatic release, no OIDC keyless signing
  — are recorded in ADR 0019 and in `needs_human`.
- `Cargo.toml`'s release profile sets `panic = "abort"` while ADR 0002 mandates
  `panic = "unwind"`. The comment above the profile claims the profile meets the
  ADR. One of the two is wrong and only the human can say which. Human-only per
  AGENTS.md section 6.
- The canonical branch is `master` (a human decision, 2026-10-03, reversing the
  earlier choice of `main`). `origin/HEAD` points at it, the work is on it, and
  `origin/main` was deleted. Protecting `master` against deletion and force pushes
  is a GitHub setting only the human can change.
- Disk: 2.3 GiB free on a 99%-full volume. Phase 1.5 needs `llama-cpp-rs`, which
  builds C++ and will fail partway through on a full disk. `cargo clean` between
  phases, or a larger volume, is the cheapest thing the human can do before then.
- Phase 1.4 is now finished: `--list-models` prints the embedded registry,
  `--download-model [NAME]` drives a real HTTPS transport (`ureq` behind the
  `download` feature, ADR 0021) with byte-range resume and a stderr progress line.
  What remains unproven is the model itself: every entry is `verified = false`,
  so a real download+hash is still the 🔒 step (ADR 0010).
- Ctrl+C (ROADMAP 3.6) is resolved by decision, not by code: ADR 0022 records
  that gcode installs no signal handler and lets the operating system default
  terminate the process (`128 + SIGINT`). A `ctrlc` handler is process-wide and
  would swallow Ctrl+C at the prompt, where `read_line` retries on `EINTR` so a
  flag cannot unblock it; the handler bought for the downloader would regress the
  prompt. The default already leaves a resumable `.part`. No dependency, no
  `unsafe`, no handler.
- Phase 1.5's timeout is not a wall clock. It uses `std::thread::scope`, which
  joins the generation thread, so a model that never finishes will not be
  abandoned at the deadline. A real timeout needs `CancellationToken` or an
  owned handle plus `catch_unwind`, and the abort path must be designed with the
  grammar-constrained sampler in mind. Worth an ADR when it is done.
- `Cargo.toml` requires `clap = "4.5"`, which is a caret requirement: a fresh
  `cargo update` may resolve 4.6, whose `rust-version` is 1.85. The lockfile
  currently pins 4.5.61, and the MSRV audit above passes because of the lockfile
  rather than because of the requirement. The manifest comment says this; the
  requirement does not enforce it.
- `Cargo.lock` pins `indexmap` to 2.11.4 because 2.12+ needs Rust 1.82. A
  `cargo update` will undo that too. Same caveat as the one above.

---

## How to read this

| Field | Meaning |
|---|---|
| `phase` | The phase an agent should work on next |
| `first_incomplete_task` | The exact task in `docs/ROADMAP.md` to start with |
| `blocked` | A previous phase is not done. Do not start. |
| `code_written` | `false` until `src/` contains an implementation |
| `needs_human` | Anything currently blocking. Must be empty for work to continue |
| `needs_human_later` | Known future blockers, recorded so they are not discovered at the end |
| `last_verified` | The last verification command that actually ran, and its result |
| `toolchain_present` | `false` means `cargo` is unavailable, so no Rust claim can be verified |
| `platforms_unverified` | Platforms in the build matrix with no successful build recorded |
| `needs_human` | Account, key, or decision that blocks progress. Work stops here |

## Rules for updating

1. Update this file in the **same commit** as the change it describes.
2. `last_verified` records a command **that ran**, with its result. Never a
   belief. `cargo test: 87 passed` is real; `cargo test: probably fine` is not.
3. `coverage_measured` is `null` until `cargo llvm-cov` has actually run. Do not
   estimate it. It is now measured: `cargo-llvm-cov` and `llvm-tools-preview` are
   installed in this environment.
4. Move `next: true` to exactly one phase. If two are marked, the file is wrong.
5. Never mark a phase `done` unless every acceptance criterion in
   `docs/ROADMAP.md` is ticked.
