# Changelog

All notable changes to this project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with an honest reading of the pre-1.0 part: anything may change before `1.0.0`.

**A feature is not shipped until it appears here under a released version.**
This rule is what keeps the documentation honest — see
[docs/README.md § The three truths](docs/README.md#the-three-truths-in-this-repository).

---

## [Unreleased]

### Added

- Complete documentation set: install, usage, architecture, safety, models,
  testing, troubleshooting, contributing, releasing, decisions, and a man page
- 16 architecture decision records under `docs/adr/`
- Build roadmap with 11 phases and per-phase acceptance criteria
- OpenCode agent tooling in `.opencode/` — 7 specialist agents, 8 commands, and
  a delivery skill — plus `AGENTS.md` and `opencode.json`
- Project rules for the agent: no secrets, no writes outside the repository, no
  commits without review
- GitHub issue, pull request, and config templates
- A release checklist at `docs/RELEASE_CHECKLIST.md`
- `src/cli.rs` (Phase 1.1): the full flag surface from `docs/USAGE.md` except
  `--edit`, `--log-level`, and the model-management flags, which need modules
  that do not exist yet. Mode selection, range validation, and a `Mode` type
  with `Display` for `--json`.
- CI, release, and nightly workflows. **Written, not yet executed** — they have
  never run against a real build.
- `Cargo.toml`: package metadata, a feature table that keeps the inference FFI
  and the only network path behind named switches, and a release profile with
  `panic = "abort"`, fat LTO, and symbol stripping per ADR 0002. Compiles.
- `rust-toolchain.toml` pinning the stable channel plus `rustfmt` and `clippy`.
  The patch version is deliberately not pinned; the reason is in
  [docs/ROADMAP.md § Note on task 0.2](docs/ROADMAP.md#note-on-task-02).
- `src/lib.rs` and `src/main.rs`. The binary accepts `--version`/`-V` and
  refuses everything else with a non-zero exit, including a bare `gcode`, so an
  unimplemented mode cannot look like a successful run.
- `src/error.rs`: a `thiserror` error enum (`HomeDirUnavailable`,
  `InvalidEnvPath`) and a crate `Result` alias. No `unwrap` outside tests.
- `src/utils/paths.rs`: all path resolution, with `XDG_CONFIG_HOME`,
  `XDG_DATA_HOME`, `GCODE_CONFIG`, and `GCODE_HISTORY_FILE` overrides. History
  stays at `~/.gcode/history.jsonl` on every platform per ADR 0005.
- `.rustfmt.toml` (stable options only) and `.clippy.toml`, plus crate-level
  `pedantic` lints in `Cargo.toml`.

### Changed

- The library's path resolution takes an explicit `Overrides` struct instead of
  reading the environment inline, and carries an `Os` field. The suite can now
  verify the Linux layout on a Mac and the macOS layout on Linux; before this,
  each platform's CI row only ever exercised its own branch.
- Argument parsing moved from an ad-hoc `parse()` in `main.rs` to `src/cli.rs`,
  on `clap` 4.5. Bare `gcode` is now the interactive mode as `docs/USAGE.md`
  documents, instead of an error that said the CLI layer had not arrived.

### Fixed

- **CI could never have run.** The push trigger named `main` and `develop`;
  this repository's default branch is `master`, so nothing has ever triggered a
  run. The trigger is corrected to `master`.
- `--help` and `--version` exited 2. clap delivers both through its error
  channel, and the first draft treated every error as a usage failure — so
  `gcode --version` looked broken to any script probing the binary. Both are now
  a separate `ParseOutcome::Handled` that exits 0.
- `--temperature -0.1` reported `unexpected argument '-0'` instead of a range
  error, because clap read the leading hyphen as another flag. The flag now
  allows hyphen values so the range check produces an actionable message.
- The mode-conflict error read `more than one mode selected: generate (generate),
  fix (fix). Choose exactly one of one of: …`. It now names the flags the user
  actually typed and lists the choices once.
- The repository URL in the documentation read `fahadmf/gcode`, which was
  guessed and wrong. It is now `fahadmahfoth/gcode` in all eight places. The
  `yourname` placeholders in `plan.md` are the maintainer's and are untouched.
- `build/` and `dist/` in `.gitignore` were unanchored, so they would also have
  matched a future `src/build/` module and dropped it from the tree.

### Verified

On macOS aarch64, rustc 1.98.1:

| Command | Result |
|---|---|
| `cargo build --release --locked` | passes |
| `cargo test` | 61 passed, 0 failed (54 lib + 2 bin + 5 integration) |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo check --no-default-features` | passes |
| MSRV audit over `cargo metadata --locked` | all 43 packages build on 1.75 |
| `gcode --version`, `gcode --help` | exit 0 |
| usage errors (two modes, out-of-range, unknown flag) | exit 2 |
| parsed-but-unimplemented modes | exit 1, naming the mode |
| credential-shape scan of the tracked tree | no matches |

Linux x86_64 has not been built, CI has never run, and coverage has not been
measured, because `cargo llvm-cov` is not installed on this machine.

---

## [0.1.0] — Unreleased

The Rust foundation described in Phase 0 of
[docs/ROADMAP.md](docs/ROADMAP.md). The commands shown in the README are the
target interface, not shipped features.

---

## Roadmap summary

Full detail in [docs/ROADMAP.md](docs/ROADMAP.md).

| Phase | Name | Status |
|---|---|---|
| 0 | Repository foundation | 🟡 code done and verified on macOS, awaiting CI |
| 1 | Core inference pipeline | 🟡 1.1 CLI done, no model yet |
| 2 | Shell integration & history | ⬜ |
| 3 | Safety layer | ⬜ |
| 4 | Packaging | ⬜ |
| 5 | Distribution & CI/CD | ⬜ |
| 6 | Polish & launch | ⬜ |
| 7 | Reliability hardening | ⛔ planned |
| 8 | Shell coverage: fish, nushell | ⛔ planned |
| 9 | Agentic multi-step execution | ⛔ planned |
| 10 | WASM plugin system | ⛔ planned |

---

## Planned for 1.0.0

Not implemented. Listed so the intent is on record.

- Natural language to shell command generation, entirely offline
- Session-history context, with secret redaction before prompt assembly
- Five-level risk classifier with an unrunnable `CRITICAL` tier
- bash and zsh integration via `gcode --init`
- `gcode --fix` for failed commands, `--complete`, `--explain`
- One-line installer, plus packages for Debian, Fedora, Arch, Alpine, macOS, Nix
- Sub-second inference on a two-core CPU, under 1.5 GB RSS

---

## Version history

| Version | Date | Highlights |
|---|---|---|
| 0.1.0 | — | Not started. Documentation and tooling so far. |
| 1.0.0 | — | Planned |

---

## Authoring

Every entry corresponds to a merged change, and the claims here are verifiable
against the repository history.
