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

### Fixed

- `safety.blocklist` is now applied. `gate` called `safety::classify`, which
  takes no user list, so an entry in the config file never reached the
  classifier; `--explain`, `--fix`, `--complete`, and generate now call
  `classify_with` with the configured list, and an edit at the prompt is
  re-checked against it. A match is `CRITICAL`, with or without `--yes`.
- A config file that fails to load now stops every mode that classifies or
  generates, with exit code 2. It used to print the error and carry on with the
  defaults, which silently dropped the user's own `always_confirm` and
  `blocklist`. `--explain`, `--list-models`, `--check`, and `--remove` still run
  on the defaults so a broken install can be diagnosed and removed.
- `an_unreadable_rc_file_is_an_error_not_a_silent_install` asserted only when it
  could read a `0000` file, which is the one case where the assertion is false,
  and passed vacuously for every other user. The condition is inverted and the
  skip says that it skipped.
- The bash and zsh hook tests passed without running when the shell was not
  installed. They now print a skip, and fail when `GCODE_REQUIRE_SHELLS` is set.
- `cargo clippy -D warnings` failed on `clippy::doc_markdown` (`SQLite`).
- `docs/gcode.1` documented twelve options the program does not have
  (`--doctor`, `--use-model`, `--update-model`, `--verbose`, `--bench`, and
  others) as if they worked, and a `RUST_LOG` variable and a `gcode doctor`
  example. They now appear only under "Not yet implemented", and `scripts/ci.sh`
  fails when the page and `--help` disagree. `README.md` no longer says the
  program is unwritten, and marks every install route that does not exist as
  planned.

### Changed

- `scripts/ci.sh --strict` (or `CI_STRICT=1`) turns every skip into a failure and
  sets `GCODE_REQUIRE_SHELLS`; it also checks the man page against `--help` and
  enforces 85 % line coverage when `cargo-llvm-cov` is installed.
- `deny.toml` added (ADR 0009). It allows `MPL-2.0` and `CDLA-Permissive-2.0`,
  which the ADR's list does not name and which are already in the tree; that
  needs a maintainer decision.
- `clap` is bounded to `>=4.5, <4.6`, and `clap_lex` and `indexmap` are bounded
  directly, so `cargo update` cannot move the lockfile past the 1.75 MSRV.

### Added

- `--list-models` and `--list-models --json` (Phase 1.3): print the embedded
  registry as an aligned table, or as a JSON array keyed by the same fields the
  downloader uses. Needs no model and no network, so it works on a fresh install,
  and the table marks the default model and warns when a digest is `unverified`.
  The JSON is an array rather than the single object every command mode emits,
  because a list is a list; `main` selects it by mode.

- The clipboard key `c` in the confirmation prompt (Phase 3.6): copies the
  command on screen — including one produced by a preceding `e` — by shelling out
  to the platform tool (`pbcopy`; `wl-copy`, `xclip`, or `xsel` on Linux), the
  same pattern as `e` and `$EDITOR`, so no dependency is added
  ([ADR 0020](docs/adr/0020-clipboard-via-platform-tool.md)). It is a
  convenience and never consent: it does not grant, and a failed copy is reported
  and re-asked rather than swallowed. The copy function is injectable, so the
  suite never starts a real clipboard process.

- The real HTTPS transport for the model downloader (Phase 1.4), behind the
  `download` feature: `ureq` with rustls, byte-range requests so an interrupted
  transfer resumes, and a streamed body so a 400 MB file is never buffered whole
  ([ADR 0021](docs/adr/0021-http-client-behind-a-feature.md)). `ureq` is named in
  exactly one file, `src/model/download.rs`, which is what the CI
  network-containment check enforces; a build without the feature carries no HTTP
  client at all.
  - `--download-model [NAME]` is wired through `runtime::download_model`: with no
    value it fetches the configured model or the registry default; with a value
    it fetches that entry, refusing a name the registry does not have before any
    transfer. Progress is a single stderr line and is suppressed for `--json`.
  - A build without the `download` feature reports `DownloadUnavailable` rather
    than doing nothing; `no_model` in the config refuses the download.

- `docs/adr/0020-clipboard-via-platform-tool.md`,
  `docs/adr/0021-http-client-behind-a-feature.md`, and
  `docs/adr/0022-no-signal-handler.md`, with rows in
  [docs/DECISIONS.md](docs/DECISIONS.md). ADR 0022 records that Ctrl+C is the
  process default: no handler (which would swallow the signal at the prompt), and
  an interrupted download still resumes from its `.part`.

- `shell/gcode.bash` (Phase 2.3): the bash history hook. One JSONL line per prompt,
  with the command, its exit status, the working directory, and the timestamp —
  the input `--fix` needs. Tested by sourcing it in a real `/bin/bash` 3.2, because
  everything it can get wrong is about bash itself and a mock of bash would pass
  against a hook that is completely broken. 27 tests in `tests/hook_bash.rs`.
  - **`$?` is preserved.** `local status=$?` on a single statement, because `local`
    is itself a command: writing `local status; status=$?` reads the status of
    `local`, which is always 0, and every entry in the store would claim every
    command succeeded. The status is returned before the hook returns, so the
    user's own `PROMPT_COMMAND` entries and their prompt still see it.
  - **The hook is prepended, not appended.** Appending is the intuitive choice and it
    is wrong: `PROMPT_COMMAND` entries run in sequence and each sees the previous
    one's status, so an appended capture records whatever the user's own hook last
    returned. With a `PROMPT_COMMAND` returning 7, every command in the store read
    `"exit":7` — a worse failure than recording nothing, because `--fix` would learn
    that every command fails for one unrelated reason.
  - **Output is deliberately not captured, and this is a deviation.** The roadmap
    asked for the last 2 KB of output. Capturing it means redirecting the command's
    stdout, which makes `[ -t 1 ]` false for everything the user runs: `ls` drops
    its colour, `less` stops paging, an editor refuses to start. That was measured.
    `out` is written empty, and `--fix` will work from the command and its exit
    status. Revising this needs a PTY-based capture or a new ADR.
  - No `jq`, no `date`, no `sed` on the hot path. `printf '%(%s)T'` needs bash 4.2
    and macOS still ships 3.2, so the timestamp comes from the shell clock and is
    refreshed at most once per second from a cached value. **Measured 2.2 ms per
    prompt** against the 5 ms budget (`date +%s` alone costs 3.2 ms).
  - Idempotent, chained rather than replacing, per-prompt `GCODE_NO_HISTORY`, `0600`
    file inside a `0700` directory, and it never fails the shell: every function that
    can fail is guarded, every expansion is `set -u`-safe, and an unwritable history
    location produces silence rather than a diagnostic on every prompt.
  - **Its own `source` line is skipped.** `--init` puts `source '<...>/gcode.bash'`
    in the user's rc file, and that is a real prompt like any other; before this it
    was recorded as the first command of every new shell, and `--fix` would have
    been offered the hook itself as the thing that failed. Only a `source` of a
    gcode hook file is skipped — a user's `source` of anything else still records.
  - **Every control character is replaced, not just three.** The escaper handled
    newline, carriage return, and tab, but RFC 8259 forbids all of U+0000..U+001F
    inside a JSON string, so a command containing a backspace or an arbitrary
    control byte wrote a line the reader rejected — losing the whole entry. It now
    uses the shell's `[[:cntrl:]]` class, verified to leave UTF-8 intact in bash
    3.2 and zsh.

- `shell/gcode.zsh` (Phase 2.4): the zsh hook, appending to `precmd_functions`.
  Measured 1.3 ms per prompt. Tested by sourcing it in a real zsh — 24 tests in
  `tests/hook_zsh.rs`, each clearing `HISTFILE` and passing `--no-rcs` so no test
  reads the developer's `.zshrc` or their real history.
  - **The exit code is captured into `ret`, not `status`.** `status` is a read-only
    special variable in zsh aliased to `?`, so `local status=$?` fails on the very
    first line of the hook: it errors on every prompt and records nothing. This was
    not hypothetical — it is what the first version of this file did, and the file's
    header now says so. A test fails if that diagnostic ever returns.
  - Appending to `precmd_functions` is safe here, unlike bash's `PROMPT_COMMAND`:
    zsh restores `$?` for each `precmd` hook, so a user's `precmd` returning non-zero
    cannot corrupt the recorded status. Asserted by a test rather than assumed.
  - The same two fixes as bash, because the two files are separate and will drift:
    the hook's own `source` line is skipped, and every control character is replaced
    via `[[:cntrl:]]` rather than only newline, carriage return, and tab. A test
    runs the same inputs through both hooks' escapers and fails if they disagree.

- `gcode --init`, `--check`, `--remove`, and `--shell <bash|zsh>` (Phase 2.5): the
  installer that wires a hook into the user's rc file. The shell is detected from
  `$SHELL`, overridable with `--shell`; an unrecognised shell is refused by name
  rather than guessed, and `--shell fish` is rejected at the argument layer. The
  block is four lines — begin marker, a version line, a `source` of the
  gcode-owned hook copy, end marker — so `--check` can report installed-and-current
  versus installed-and-stale without touching the file.
  - **The `source` line points at `~/.gcode/shell/`, not at the binary.** The hook
    text is embedded at compile time and copied there on install, so moving or
    repackaging the binary cannot break a user's prompt.
  - **Removal is byte-precise.** Install appends one separator newline; remove
    deletes the four lines plus exactly that one newline. Round trips that start
    from an empty file, a lone newline, a file with no trailing newline, and a file
    with trailing blank lines all restore byte-for-byte. An empty file comes back
    empty rather than as one stray newline.
  - **It never rewrites a line it did not add.** A user's own `PROMPT_COMMAND` or
    anything else in the rc file survives install and remove untouched, because the
    installer only ever appends and removes its own block.
  - **A corrupt block is refused, not repaired.** A begin marker with no matching
    end marker is `Error::ShellBlockMalformed`; marker matching is line-exact so a
    hand-edited lookalike cannot be mistaken for the block. An rc file that exists
    but cannot be read is `Error::ShellRcUnreadable` and nothing is written.
  - **Deviation from ADR 0007:** the ADR shows `--init --remove`, but the three
    flags are modes and any two conflict, so that spelling is a usage error.
    Choosing a winner between two contradictory verbs would silently do the
    opposite of half the command line. Recorded here and in ROADMAP 2.5.
  - 45 tests in `src/shell/tests.rs` plus the hook-mode cases in
    `tests/runtime.rs`, run through `runtime::shell(mode, &Installer)` against
    temporary directories. There is deliberately no test that runs the whole binary
    against the real home: one had been written, and it was removed after it
    resolved the real `$SHELL` and installed the hook into the developer's actual
    `~/.bashrc`. The accidental install was undone byte-precisely with
     `--remove --shell bash`; nothing else in that file was changed.

- `gcode --fix` (Phase 2.6): repair the most recent failed command. It reads up
  to 200 entries from the store, takes the newest with a non-zero exit, and asks
  the model for one corrected command. The prompt is assembled through
  `context::prompt::build_prompt`, so the failed command and its output are
  redacted and escaped by the same single function as every other piece of
  context (ADR 0006) rather than concatenated by hand. The failed entry and the
  entries before it are passed as history, ending the slice at the failure so the
  command being repaired cannot be pushed out of the prompt's window by newer,
  successful commands.
  - The model's answer goes through the same `gate` as any generation: classified
    from the emitted string, blocked if `CRITICAL`, and consented to above SAFE.
    A "fix" is not a bypass. `--yes` suppresses the prompt only.
  - The explanation is a diff — `- <failed>` then `+ <repaired>` — so the change
    is visible without re-reading history. If the model returns the original
    unchanged, the output says so instead of showing an empty diff.
  - A clean history (or `--no-history`) is not an error: the output carries
    "no failed command in history; nothing to fix", no command, and exit 0.
  - 7 tests in `tests/runtime.rs` against a `FakeEngine` and a temporary store,
    including redaction of a secret in the failure output, the
    most-recent-versus-older choice, and a repaired command that is refused as
    `CRITICAL`.
  - The `NoEngine` message now names `--fix` alongside generate and complete, since
    a failure with no model invokes the same error.
  - **`--complete` resolved.** The roadmap's "returns only the continuation" was
    set against the full completed command documented in USAGE.md, gcode.1, and
    plan.md, and against safety invariant 1: emitting a bare suffix would mean the
    string that is classified is not the string that is shown. `--complete` emits
    and classifies the whole completed command, and now feeds the partial through
    `context::prompt::build_prompt` so a credential typed into the partial is
    redacted before the prompt. Tests cover the full-command classification and
    the redaction.

- `src/context/env.rs` (Phase 2.2): the environment context. The working directory,
  OS, architecture, `$SHELL` reduced to a program name, its version, and the git
  branch, dirty flag, and last commit subject. Every fact is optional, and a fact
  that could not be read is absent rather than defaulted: there is no `unknown` branch
  name and no assumed `bash`, because a placeholder teaches the model that
  placeholders are what this field looks like.
  - The git probe is bounded by one deadline for the whole probe, not one per
    question, and runs its two `git` calls concurrently. **The deadline is 250 ms,
    not the 30 ms the roadmap first specified** — measured, not preferred: the
    concurrent pair costs 20–30 ms idle and ~40 ms under load, so a 30 ms budget
    expires before the work finishes and the feature would never report anything.
    The deadline still kills the child on overrun.
  - Branch and dirty flag come from a single `git status --porcelain=v2 --branch`,
    so a repository with no commits — every new project — still reports its branch,
    with no subject, instead of reporting nothing.
  - Failure outside a repository is decided by the **exit status**. `git` exits 128
    and prints nothing there, and empty stdout is also the correct answer to "which
    files changed?" in a clean repository, so stdout alone cannot distinguish them.
  - A detached `HEAD` is reported as no branch rather than as a branch named
    `detached`.
  - `GCODE_NO_GIT` omits the repository facts and keeps the directory and platform;
    `GCODE_NO_ENV` omits every environment fact and takes precedence.
  - The module renders nothing. Facts move into `prompt::Context` and are redacted
    and XML-escaped there (ADR 0006), because a branch name and a commit subject are
    both free text a user controls. 20 tests, with real git repositories in temporary
    directories and a real subprocess for the shell probe.
  - `EnvSnapshot::collect` still has no runtime caller: the run loop builds no
    environment `Context` yet, so `build_prompt` is called with `Context::default()`
    until Phase 1.5 wires the environment in.
- The run loop now assembles every generative prompt with
  `context::prompt::build_prompt` (Phase 1.8): `--generate` joins `--fix` and
  `--complete`, so the request is redacted and wrapped in the system instruction
  before it reaches the model, instead of the raw string being handed over. The
  prompt builder
  was implemented and tested in 1.7 but had no production caller; wiring it closes
  an ADR 0006 gap, because a credential pasted into `-c "…"` previously reached the
  engine unmasked. The environment `Context` is still `Context::default()` until
  Phase 1.5 wires `EnvSnapshot::collect`. Test:
  `tests/runtime.rs::a_secret_in_the_request_is_redacted_before_the_prompt`.
- `src/context/history.rs` (Phase 2.1): the JSONL history store. `HistoryEntry`,
  backwards `read_last(n)` that seeks a chunk at a time and stops at the `n`-th
  newline, single-`write` + `fsync` appends, stats, and rotation at 10 MiB keeping
  the newest 5 MiB as `.1`. A malformed line is skipped with one warning however many
  there are; a torn final line is discarded. 29 tests, including six that inject a
  real IO failure and assert it surfaces as `Error::History` rather than as an empty
  history. The shell hooks write it and `--fix` reads it; wiring it into the
  environment context is a later phase.
- `Prompt::reading` and `Prompt::editing` (Phase 3.6): the prompt's input and editor
  are injectable. `ask` bailed on the first line whenever stdin was not a terminal,
  which left the entire keymap — including re-classification after `e` — untestable.
  18 tests now cover it.
- [ADR 0017](docs/adr/0017-explicit-file-modes-over-umask.md), amending rule 6 of
  ADR 0005: the history store sets `0700`/`0600` on its own two paths instead of
  calling `umask`. A library that calls `umask` changes the mode of every file the
  host process creates afterwards. The explicit form also tightens a pre-existing
  `0644` file, which `umask` could not do.


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

### Fixed

- The inference timeout is now a real wall clock. Generation runs on a detached
  thread that owns the engine, so a stuck call can no longer hold the deadline open
  past it; the previous `thread::scope` version joined on the way out, so the
  "timeout" was only reached after the engine returned anyway. A regression test pins
  the difference.
- `e` in the prompt loops back instead of ending the turn. The edited text is
  re-classified for display and re-rendered before the question is asked again, so
  `y` cannot answer for a command that is no longer on screen. A second `e` starts
  from the first edit.
- An injected `Prompt` writer no longer stops receiving output after the first
  block. The writer was taken out of `self` and dropped at the end of each write, so
  every later block silently fell through to stderr. Harmless with the real prompt,
  which wants stderr anyway; it made the `r`, `?`, and `e` paths untestable.
- A `--complete` run reported `mode: "generate"` in its output and in `--json`,
  because `gate` passed the literal. The name now comes from the mode enum.
- A failed `$EDITOR`, or one that saves an empty or whitespace-only file, is refused
  rather than falling back to the pre-edit command. The trim-and-reject now sits
  above the editor dispatch, so it applies to an injected editor too.
- Removed an `escalate` helper in `src/safety/classifier.rs` that had no production
  caller left, and the taint branch that could never fire.

### Changed

- A **release** build is refused while any entry in `models/registry.toml` is
  `verified = false`. The registry now holds three real, commit-pinned models —
  `qwen3-0.6b` (default, Arabic and English), `kitty-bash-llm` (English), and
  `qwen3-1.7b` — with their sizes, URLs, and HuggingFace-published digests
  (ADR 0018, superseding the ADR 0014 default). The digests are claims until a
  local download confirms them, which is why every entry is still unverified and
  no installable artefact can be produced. Debug builds and the test suite are
  unaffected.
- CI is a local script, `scripts/ci.sh`, and GitHub Actions was removed from the
  repository (ADR 0019). The script runs format, clippy, the tests, the policy
  greps, the secret scan, the man-page lint, and the documentation link check,
  and prints a skip rather than a pass for anything the machine cannot run. The
  account was locked for a billing reason and no workflow could execute, so the
  gate no longer depends on a hosted runner.
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
