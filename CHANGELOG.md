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

### Fixed

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
| `cargo test` | 34 passed, 0 failed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo check --no-default-features` | passes |
| `target/release/gcode --version` | `gcode 0.1.0`, exit 0 |
| `target/release/gcode` | error, exit 1 |
| credential-shape scan of the tracked tree | no matches |

Linux x86_64 has not been built, and coverage has not been measured, because
`cargo llvm-cov` is not installed on this machine.

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
| 1 | Core inference pipeline | ⬜ |
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
