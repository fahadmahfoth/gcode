# STATE — machine-readable build state

> **Maintained by the OpenCode agent.** This file is the quick pointer; the
> human-readable contract is [../docs/ROADMAP.md](../docs/ROADMAP.md).
> Last synced by hand: 2026-09-30.

```yaml
phase: 0
phase_name: Repository foundation
phase_status: in_progress
first_incomplete_task: 0.8

# Phases, in order. `next` is the only actionable one.
phases:
  - id: 0
    name: "Repository foundation"
    status: in_progress
    next: true
  - id: 1
    name: "Core inference pipeline"
    status: not_started
  - id: 2
    name: "Shell integration & history"
    status: blocked
  - id: 3
    name: "Safety layer"
    status: blocked
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
needs_human: []
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

# Compiled, and `--locked` clean. The three declared dependencies are the only
# ones the Phase 0 modules use; `inference` and `download` stay empty until
# Phase 1 resolves their versions.
manifest_written: true
manifest_compiled: true
platforms_verified:
  - macOS aarch64 (rustc 1.98.1)
platforms_unverified:
  - Linux x86_64
last_verified: "macOS aarch64, rustc 1.98.1: `cargo build --release --locked` passed; `cargo test` 34 passed / 0 failed (22 lib + 7 bin + 5 integration); `cargo fmt --all -- --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo check --no-default-features` passed; `gcode --version` -> 'gcode 0.1.0' exit 0; bare `gcode` and unknown flags exit 1; credential-shape scan of the tree found nothing"
last_command: "cargo test, then cargo clippy --all-targets --all-features -- -D warnings, then the release smoke test"
```

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

## Rules for updating

1. Update this file in the **same commit** as the change it describes.
2. `last_verified` records a command **that ran**, with its result. Never a
   belief. `cargo test: 87 passed` is real; `cargo test: probably fine` is not.
3. `coverage_measured` is `null` until `cargo llvm-cov` has actually run. Do not
   estimate it.
4. Move `next: true` to exactly one phase. If two are marked, the file is wrong.
5. Never mark a phase `done` unless every acceptance criterion in
   `docs/ROADMAP.md` is ticked.
