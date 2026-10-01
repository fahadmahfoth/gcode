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

- `src/model/registry.rs` (Phase 1.3): the model registry, embedded from
  `models/registry.toml` with `include_str!` and parsed once into a `OnceLock`.
  There is no search path and no environment variable pointing at it, so a user
  cannot end up with a registry newer than the tool that trusts it.
- `build.rs`, which enforces the registry rules at compile time (ADR 0010). A
  missing checksum, a non-HTTPS URL, two defaults, a duplicate name, a zero
  size, a missing licence, or an unrecognised key is a build failure naming the
  file and every problem found, not just the first.
- `ModelEntry::size_human` — binary units, computed in integer arithmetic at
  u128. A file size is the one number here a user has to be able to trust
  exactly, so there is no float anywhere near it.

- `src/safety/` (Phase 3.1–3.5): the risk classifier. `Risk` with an `Ord`
  ordering and a single `is_runnable` decision point, `Verdict` carrying the
  per-segment levels and the reason for each, and a pure `classify(&str)` that
  takes nothing but the command string. The module cannot reach the model: it
  imports nothing from `crate::inference`, and a test asserts it.
- The compound-command pipeline: comments are stripped outside quotes, line
  continuations joined, `$VAR` and `${VAR}` resolved when set, and then the
  command is split on `;`, `&&`, `||`, `|`, `&`, and newline — tracking quotes
  and brace depth, so a function body stays one segment and the fork bomb cannot
  be broken into harmless pieces. Every segment is classified and the maximum
  wins. An unset `$VAR` is left literal on purpose: blanking it would turn
  `rm -rf $TARGET` into `rm -rf`, which reads as harmless.
- `src/safety/patterns.rs`: 39 pattern rows across CRITICAL, HIGH, MEDIUM, and
  LOW, matched by an ordered token matcher rather than a regex. Command names are
  anchored to the head token, so `echo rm -rf /` does not match a root-delete
  rule. Every row has a positive test and a near-miss negative test, and a test
  asserts the declared test list equals the table exactly, so adding a row
  without tests fails the build.
- An unknown-command floor. A command whose head is in neither the table nor
  `KNOWN_SAFE` is `LOW`, never `SAFE`. A table that calls unknown commands safe
  fails open, which is the wrong direction for the module whose job is not to
  fail open.
- Structural checks: a write to path P followed by a delete of P escalates a
  level; `sudo`/`doas`/`pkexec` anywhere is at least MEDIUM; and a fetch chained
  with an execute is at least HIGH, checked *across* segments, because
  `curl … | bash` is two segments and no per-segment rule can see it.
- The blocklist runs before level assignment and is unoverridable. It matches
  against the whole command as well as per segment, because Bash lexes the fork
  bomb's trailing `;` as a separator.
- `src/runtime.rs` (Phase 1.8, 3.6, 3.7): the run loop. `run()` takes its
  `InferenceEngine` and its `Consenter` as parameters rather than reaching for
  globals, which is what lets the entire pipeline be tested in milliseconds
  against a fake model and no terminal.
- A `Consenter` trait with two implementations: `DenyAll`, which cannot consent
  to anything, and `ui::prompt::Prompt`, which asks a human. `main` picks `DenyAll`
  for a pipe or for `--json`, so a machine-readable invocation can never block on
  a prompt and never runs an unconfirmed command. This is the "piped stdin never
  executes anything" invariant as a fact about `main`, not as a runtime check.
- `Decision::Granted(String)` rather than a bare yes: a prompt hands back the
  command to act on, and the run loop re-classifies it from scratch. That is
  invariant 3 — editing re-runs classification — placed in the core so no UI can
  skip it. A tool that classified one command and ran another is the exact bug
  this type makes inexpressible.
- `needs_confirmation(level, always_confirm, yes)`: `--yes` suppresses the prompt
  and does nothing else. This corrects a contradiction between two existing
  documents — `config.rs` said `always_confirm` applies "regardless of `--yes`",
  which at the default `MEDIUM` would have made `--yes` unusable on every install.
  The roadmap is the build contract, so the flag wins and the field comment was
  corrected.
- `--explain` (Phase 3.8), which needs no model and is therefore the one mode
  that works today. It prints what the command does, what it runs, what paths it
  touches, the per-segment levels, and every reason with the pattern id behind it.
  `touched_paths` reads the normalised segments, so a path inside a stripped
  comment is not reported as touched while one inside a quoted argument still is.
  An explain of a `CRITICAL` command is not an error: refusing to explain the most
  dangerous command would make the tool useless for the one thing a user most
  wants to understand.
- `Output::to_json`, hand-built with no serde dependency, matching the decision
  already taken in `context/prompt.rs` at MSRV 1.75. Escape order is backslash,
  quote, then control characters.
- New error variants: `ConsentDenied`, `RiskBlocked`, `NoEngine`, `ModeNotWired`.
  `NoEngine` exists so a generating mode with no model says so and exits 1
  rather than inventing a plausible-looking command.
- `tests/runtime.rs`: 28 tests over the run loop, including a `FakeEngine` test for
  each of `--dry-run`, `--json`, `--yes`, the `CRITICAL` block across nine flag
  combinations, the pipe refusal, and the `HIGH`→`CRITICAL` edit.

- `tests/safety.rs`: the safety suite proper — one test per invariant in
  AGENTS.md §3, a corpus of 23 unoverridable commands with 35 near misses that
  must *not* be blocked, and a 10 000-command mutation fuzz that must produce
  zero panics.

- `src/config.rs` (Phase 1.2): the configuration layer. A `Config` whose every
  field is optional, a four-source merge (default → file → environment → flag)
  in one function, and an `EffectiveConfig` with no optional fields left.
  `Config::load` treats a missing file as normal and a malformed one as an error
  naming the file, the line, and the field. `resolve` validates the merged
  value, so a file that never passed through the argument parser is still range
  checked.
- `always_confirm` may be set to `SAFE`, `LOW`, `MEDIUM`, or `HIGH`. `CRITICAL`
  is refused: a critical command is never run, so that threshold could never be
  reached by anything that prompts.
- `RiskLevel` in the library, ordered so that the threshold comparison is a
  `>=` rather than a hand-written match.
- A `CONFIGURATION` section in the man page, and a defaults table in
  `docs/USAGE.md` with the bound on every setting.
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

### Changed

- A **release** build is refused while `models/registry.toml` carries the
  placeholder checksum, which is the SHA-256 of the empty string. Debug builds
  and the test suite are unaffected, so the placeholder blocks shipping without
  blocking development. A checksum is a claim about the bytes of a specific file
  and cannot be written honestly without the file; that is the human's to
  provide, verified once by downloading it.
- The numeric defaults moved out of `clap`
### Added

- `src/context/redact.rs` (Phase 1.7): one `redact` function, applied before
  prompt assembly and nowhere else (ADR 0006). Covers PEM key blocks whole,
  `Authorization: Bearer …`, API keys with recognisable prefixes (OpenAI,
  GitHub, Slack, AWS, Google, GitLab), and inline `password=` / `token=` /
  `api_key=` / `secret=` assignments. Redaction consumes only the credential,
  not the rest of the line, so `token=abc && ls` keeps its `&& ls`. An
  assignment with no value is left alone, so `grep -r 'token=' src/` is not
  rewritten. `tail_bytes` truncates from the end and moves a cut that lands
  mid-UTF-8 to a character boundary.
- `src/context/prompt.rs` (Phase 1.7): prompt assembly per MODELS.md, with
  context and caps injected so it is testable without a shell. History goes
  inside untrusted-data delimiters under a system line that says outright the
  data is not instructions. Attribute values are escaped, so a quote in a git
  branch name cannot close the attribute and put the rest of it into the
  instruction region.
- `src/inference/mod.rs` (Phase 1.5): the `InferenceEngine` boundary, with
  `GenParams`, `EngineInfo`, and `InferenceError`. `generate_command` is the
  single entry point, so post-processing, the output bound, and the wall clock
  cannot be bypassed by calling `generate` directly. Post-processing strips
  markdown fences, `bash` tags, `Command:` labels, `$ ` prompts, and a leading
  prose line, and deliberately keeps a comment line and any command whose first
  word is not on a list of common utilities. Output is bounded at 8 KiB
  regardless of what `max_tokens` claims. A timeout returns an error and never a
  late or partial command. The engine is installed once per process behind a
  `OnceLock`; a second install is refused so test ordering cannot swap a real
  model for a fake.
- `src/model/download.rs` (Phase 1.4): model download with hash verification.
  A `Transport` trait keeps the network behind a seam; progress is reported via
  a callback, and bytes are hashed as they stream past so the file is never
  read twice. Existing correct files return `AlreadyPresent` with no network
  access. A partial `.part` file is resumed, the transfer keeps a good prefix
  when the connection drops, and a hash mismatch triggers a mirror attempt
  before refusing. The final file is `rename`d into place and given `0644` on
  Unix. and into `config::defaults`.
  `--temperature` with no argument is now "not specified" rather than 0.2, which
  is what makes "the flag beats the file" true: a flag carrying a hidden default
  outranks every source below it. Range checks are unchanged and still apply to
  a value the user typed.
- Unknown config keys are refused rather than ignored. A misspelled setting that
  is silently dropped is a config that appears to do nothing.
- `shell.redact_env` extends the built-in redaction patterns and can never
  replace them, per ADR 0006. `safety.blocklist` still replaces, because it is
  the user's list of commands they would rather not see.
- Dependencies added: `toml` 0.8 and `serde` 1.0 with `derive`. `indexmap` is
  pinned to 2.11.4 in the lockfile because 2.12+ requires Rust 1.82, above the
  1.75 floor in ADR 0002.

### Fixed

- `--no-git` and `--no-env` were documented in `docs/USAGE.md` and absent from
  the parser. Both are implemented, and both map to `context.include_git` and
  `context.include_env`.
- `docs/gcode.1` no longer claims a fixed default for `--context`,
  `--n-threads`, `--context-size`, and `--temperature`. Those now depend on the
  config file, and the man page says so.
- Three over-long lines in `docs/gcode.1`, flagged by `mandoc -T lint`.
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
