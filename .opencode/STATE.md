# STATE — machine-readable build state

> **Maintained by the OpenCode agent.** This file is the quick pointer; the
> human-readable contract is [../docs/ROADMAP.md](../docs/ROADMAP.md).
> Last synced by hand: 2026-10-01.

```yaml
phase: 1
phase_name: Core inference pipeline
phase_status: in_progress
first_incomplete_task: 1.8

# Phase 3 was started ahead of the rest of 1.8 because it is pure Rust, has no
# dependency on the model, and gates everything that later runs a command.
#
# 3.1-3.5, 3.7, and 3.8 are written, tested, and passing, and 3.6 is mostly
# written. Still open in phase 3: Ctrl+C handling, the MEDIUM+ cost estimate, the
# clipboard key, and looping back to the prompt after an edit. Phase 3 stays
# in_progress, not done.

# Phases, in order. `next` is the only actionable one.
phases:
  - id: 0
    name: "Repository foundation"
    # Rust half is done and verified on macOS. Still open: the Linux x86_64
    # build and a green CI run, which is what tasks 0.8 and 0.12 need.
    status: in_progress
    next: false
  - id: 1
    name: "Core inference pipeline"
    status: in_progress
    next: true
  - id: 2
    name: "Shell integration & history"
    status: blocked
  - id: 3
    name: "Safety layer"
    # 3.1-3.5, 3.7, 3.8 written and verified, 3.6 mostly: Risk/Verdict,
    # normalisation and splitting, the blocklist, 39 pattern rows, the structural
    # checks, the prompt, --yes semantics, and --explain. Still open: Ctrl+C, the
    # cost estimate, the clipboard key, re-prompting after an edit, and the
    # coverage criterion, which needs cargo-llvm-cov.
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
coverage_measured: null
coverage_target: 85

# Everything that needs a human. Empty means nothing is blocked on the human.
needs_human:
  - >-
    GitHub Actions is refusing to start jobs: "The job was not started because
    your account is locked due to a billing issue" (run 36765649289, every job,
    2026-09-30). This is an account/billing state, not a repository problem,
    and no change to a workflow can clear it. Until it is resolved, CI cannot
    verify anything and Phase 0 tasks 0.8 and 0.12 stay open. The user must fix
    the account.
  - >-
    models/registry.toml holds one placeholder entry, not a real model: its
    sha256 is the SHA-256 of the empty string and its url points at a .invalid
    host. A checksum is a claim about the bytes of a specific file and cannot be
    written honestly without downloading that file, which is also what ADR 0010
    requires before a sha256 may be recorded. build.rs refuses a release build
    while the placeholder digest is present, so debug builds and the test suite
    work and no installable artefact can be produced. The user must supply a
    real model, its URL, its size, and a checksum they have verified by
    downloading the file once.
  - >-
    Disk is tight: 2.3 GiB free on a 99%-full volume. Better than the 281 MiB
    recorded before, but Phase 1.5's LlamaEngine still needs llama-cpp-rs, a C++
    toolchain, and real weights, and a C++ build plus several GB of weights will
    not fit in 2.3 GiB. The user should free 15-20 GiB before that task.
    Until then 1.5's trait, post-processing, and timeout are complete and tested
    against FakeEngine, and only the sampler is outstanding.
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

# Compiled, and `--locked` clean. Five dependencies are declared: thiserror,
# anyhow, dirs, clap, and toml with serde for the config layer. `inference` and
# `download` stay empty until Phase 1.4 resolves their versions.
manifest_written: true
manifest_compiled: true
platforms_verified:
  - macOS aarch64 (rustc 1.98.1)
platforms_unverified:
  - Linux x86_64
last_verified: "macOS aarch64, rustc 1.98.1, after Phase 1.8 run loop and Phase 3.1-3.8: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo test` 388 passed / 0 failed (326 lib incl. 100 safety patterns + 71 safety classifier + 40 context + 29 inference + 28 download + 20 registry + prompt tests, 14 integration, 28 tests/runtime.rs, 17 tests/safety.rs incl. the 10 000-command fuzz, 3 doc); `cargo build --locked` passed; MSRV audit over `cargo metadata --format-version 1 --locked` confirms all 65 locked packages declare rust-version <= 1.75. `cargo build --release --locked` FAILS BY DESIGN: build.rs refuses the placeholder model registry (see needs_human). `mandoc -T lint docs/gcode.1` clean. Secret scan clean apart from two reviewed false positives, both the same illustrative `grep -r 'token=' src/` example. CLI verified by hand: `--explain 'rm -rf /'` exits 0 with a CRITICAL explanation and no model; `--explain --json` emits one object; `-c` with no model exits 1 with an honest message; `--fix --explain ls` exits 2; `--version` exits 0. Eight defects were found and fixed by this work: `targets_for` returned `(path, command)` but was destructured as `(command, path)`, so taint never fired; the SQL matcher compared a whole quoted token against a single word; the home blocklist matched any path under `~`, over-blocking `rm -rf ~/Documents`; the coverage test compared list lengths instead of set equality; `classify_in_env` was over Clippy's 100-line limit; `needs_confirmation` implemented `config.rs`'s 'regardless of --yes', which would have made `--yes` unusable at the default MEDIUM threshold and contradicted the roadmap's own 3.7 test; the `Consenter` trait originally returned a bare yes, so a prompt returning an edited command would have had the core run the pre-edit string; and the first `run()` draft classified the natural-language request instead of the generated command"
previous_last_verified: "macOS aarch64, rustc 1.98.1, after Phase 1.7: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo test` 223 passed / 0 failed; `cargo build --locked` passed; MSRV audit clean over 65 locked packages; `mandoc -T lint docs/gcode.1` clean; secret scan clean; `cargo build --release --locked` failed by design on the placeholder registry"

last_command: "cargo fmt --all -- --check, cargo clippy --all-targets --all-features -- -D warnings, cargo test, cargo build --locked, the MSRV audit over cargo metadata, mandoc -T lint, and the credential-shaped-assignment scan"

```

### Known open items, not yet recorded above

- The CI push trigger was fixed and verified: a push to `master` now produces a
  run. The run fails, but not on the code — every job reports "The job was not
  started because your account is locked due to a billing issue". See
  `needs_human`. Until that clears, the CI-dependent acceptance criteria for
  Phase 0 and the Linux x86_64 build cannot be settled.
- `Cargo.toml`'s release profile sets `panic = "abort"` while ADR 0002 mandates
  `panic = "unwind"`. The comment above the profile claims the profile meets the
  ADR. One of the two is wrong and only the human can say which. Human-only per
  AGENTS.md section 6.
- `docs/CONTRIBUTING.md` says pull requests target `main`. There is no `main`
  branch and the default is `master`. The CI push trigger was corrected to
  `master`; the prose in CONTRIBUTING is still wrong.
- Disk: 2.3 GiB free on a 99%-full volume. Phase 1.5 needs `llama-cpp-rs`, which
  builds C++ and will fail partway through on a full disk. `cargo clean` between
  phases, or a larger volume, is the cheapest thing the human can do before then.
- Phase 1.4 is checked off in the roadmap but is not finished: there is no real
  HTTPS transport behind the `Transport` trait, the `download` feature is not
  wired, there is no stderr progress renderer, and nothing calls
  `ensure_model()`. What is finished and tested is the engine underneath: the
  checksum, the `.part` file, resume, mirrors, and atomic rename.
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
   estimate it.
4. Move `next: true` to exactly one phase. If two are marked, the file is wrong.
5. Never mark a phase `done` unless every acceptance criterion in
   `docs/ROADMAP.md` is ticked.
