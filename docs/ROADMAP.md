# Build Roadmap

This is the **contract**, not the vision. `plan.md` at the repository root is the
long-range ambition; this document is what is actually being built, in order,
with acceptance criteria that must be demonstrably true before a phase is closed.

Every task below is executed by the OpenCode agent defined in
[`../AGENTS.md`](../AGENTS.md). You do not write the code, the tests, the docs, or
the plan updates. You review and decide.

---

## Status legend

| Marker | Meaning |
|---|---|
| ⬜ | Not started |
| 🟡 | In progress |
| ✅ | Done, acceptance criteria verified |
| ⛔ | Blocked — see the note under the phase |
| 🔒 | Requires a human decision (secrets, keys, domains, signing) |

🔒 is the only category that ever needs you. Everything else is autonomous.

---

## How the agent reads this file

1. Find the first phase that is not ✅.
2. Work its tasks in order, top to bottom.
3. For each task: write code → write the test → run the test → run lint →
   update this file's status marker → commit.
4. Do not start a phase until the previous phase is ✅.
5. If a task is 🔒, stop and ask. Do not invent a value.

The current state of the build is also written to
[`../.opencode/STATE.md`](../.opencode/STATE.md), which the agent updates
mechanically. This file is the human-readable source of truth; STATE.md is the
machine-readable pointer.

---

## Phase overview

| Phase | Name | Status | Blocks the next? |
|---|---|---|---|
| [0](#phase-0--repository-foundation) | Repository foundation | 🟡 in progress | Yes |
| [1](#phase-1--core-inference-pipeline) | Core inference pipeline | 🟡 in progress | Yes |
| [2](#phase-2--shell-integration) | Shell integration & history | 🟢 shipped; acceptance pending | Yes |
| [3](#phase-3--safety-layer) | Safety layer | 🟡 3.6 has one item open | Yes |
| [4](#phase-4--packaging) | Packaging | ⬜ not started | Yes |
| [5](#phase-5--distribution--cicd) | Distribution & CI/CD | ⬜ not started | No |
| [6](#phase-6--polish--launch) | Polish & launch | ⬜ not started | No |
| [7](#phase-7--reliability) | Reliability hardening | ⛔ planned | No |
| [8](#phase-8--shell-coverage) | Shell coverage: fish, nushell | ⛔ planned | No |
| [9](#phase-9--agentic-mode) | Agentic multi-step execution | ⛔ planned | No |
| [10](#phase-10--plugin-system) | WASM plugin system | ⛔ planned | No |

Phases 0–6 are the v1.0 definition of done. Phases 7–10 are post-v1.0 and exist
only so the design decisions of v1.0 do not paint us into a corner.

---

## Phase 0 — Repository foundation

**Status: 🟡 in progress — the Rust half builds, tests, and lints clean on macOS
aarch64 and Linux x86_64; the local CI gate (`scripts/ci.sh`) is green on both
([ADR 0019](adr/0019-local-ci-not-github-actions.md)). The one open item is the
release build, which is refused until a registry entry is verified by a real
download.**
**Goal: a repository where `cargo build` works on a maintainer's laptop.**

### Tasks

| # | Task | File | Status |
|---|---|---|---|
| 0.1 | Cargo manifest, features, release profile | `Cargo.toml` | ✅ compiles, 3 deps |
| 0.2 | Dependency pinning + MSRV | `rust-toolchain.toml` | ✅ stable + rustfmt/clippy |
| 0.3 | `src/main.rs` skeleton that prints version | `src/main.rs` | ✅ `--version`, `-V` |
| 0.4 | Error types | `src/error.rs` | ✅ 2 variants, no `unwrap` |
| 0.5 | Path resolution | `src/utils/paths.rs` | ✅ 4 env overrides, 22 unit tests |
| 0.6 | Lint + format config | `.rustfmt.toml`, `.clippy.toml` | ✅ `pedantic` clean |
| 0.7 | Gitignore, deny list | `.gitignore` | ✅ |
| 0.8 | CI: fmt, clippy, test, policy greps | `scripts/ci.sh` | ✅ runs green on macOS aarch64 and Linux x86_64 |
| 0.9 | Licence, contributing, security policy | `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md` | ✅ |
| 0.10 | Documentation set | `docs/**` | ✅ |
| 0.11 | OpenCode agent + command tooling | `.opencode/**`, `AGENTS.md` | ✅ |
| 0.12 | Issue/PR templates, local secret scan | `.github/**`, `scripts/ci.sh` | ✅ |

### Acceptance criteria

- [ ] `cargo build --release` succeeds on Linux x86_64 **and** macOS aarch64
      — ⛔ a release build is now refused while any registry entry is
      `verified = false` ([ADR 0018](adr/0018-bilingual-default-model.md)),
      which is the current state. The debug build passes on macOS aarch64;
      Linux is unverified. Clearing this needs one real model download and
      hash, then flipping `verified = true`.
- [x] `cargo test` passes — 34 passed, 0 failed
- [x] `cargo fmt --check` and `cargo clippy -D warnings` are clean
- [x] The local CI gate exits zero — `./scripts/ci.sh`
      — ✅ macOS aarch64, and ✅ Linux x86_64 (2026-10-03: 0 failed, 1 skipped,
      the release build, which is refused by design while a registry entry is
      unverified; includes the man-page check, `cargo llvm-cov` ≥ 85 %,
      `cargo audit`, and `cargo deny`).
- [x] `README.md`, `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md` present
- [x] No credential-shaped string anywhere in the tracked tree
- [x] `AGENTS.md` tells an agent what to do next

One unchecked item remains: a Linux x86_64 build. The project moved off GitHub
Actions ([ADR 0019](adr/0019-local-ci-not-github-actions.md)) after the account
was locked for billing, so there is no hosted runner to settle it. A workflow
that has never run is not a passing workflow; `scripts/ci.sh` runs locally
instead, and its printed output is the evidence.

### What unblocks this phase

Running `scripts/ci.sh` on a Linux x86_64 machine, or adding a self-hosted
runner for it. The macOS half is green; nothing in this repository can produce a
Linux result by itself.

### Note on task 0.1

**The manifest declares three dependencies and no more** — `thiserror`,
`anyhow`, `dirs` — because those are the only ones the Phase 0 modules use.
`llama-cpp-rs`, `llguidance`, and `reqwest` are deliberately absent. Their
versions are resolved in Phase 1 with `cargo add`, where the registry is
reachable. Writing a guessed version here would produce a manifest that fails
`--locked` on its first build, which is the one outcome worse than not having
written it.

The `inference` and `download` features are declared but empty for the same
reason: they must exist before any code can be gated behind them, and they get
their dependency in Phase 1.

### Note on task 0.2

`rust-toolchain.toml` pins the **stable channel**, not a patch version. Two
reasons. A hard `1.98.1` pin makes a contributor on a newer patch unable to build
without editing the file, and the pin is what triggered a duplicate ~45 MB
toolchain download on the machine that wrote it — a failure mode a channel pin
does not have. `rust-version = "1.75"` in `Cargo.toml` remains the real floor, and
`clippy::incompatible_msrv` is enabled to enforce it.

### Note on task 0.3

Bare `gcode` exits non-zero with a message. It does **not** print the version and
exit 0. USAGE.md documents bare `gcode` as the interactive mode, which is
⛔ planned; printing a version there would make an unimplemented mode look like a
completed run, which is the exact failure the same file's unknown-flag branch was
written to prevent. Argument parsing is a pure function with unit tests, so the
real dispatch in 1.1 lands in a tested function rather than in `main`.

---

## Phase 1 — Core inference pipeline

**Status: 🟡 1.1–1.4, 1.7, and 1.8 done. 1.5 is complete except the sampler
(`LlamaEngine::load` + stop conditions), which is blocked on `llama-cpp-rs`, a C++
toolchain, and real weights. 1.6 (grammar) is not started and depends on 1.5. The
CLI cannot generate anything yet, because there is no model**
**Goal: `gcode -c "list all files" --dry-run` prints a valid command and exits.**

This is the phase that decides whether the project is real. Nothing else matters
until a command comes out of a model.

### 1.1 CLI layer — `src/cli.rs`

```rust
pub enum Mode {
    Generate { request: String },
    Fix,
    Complete { partial: String },
    Explain { command: String },
    Doctor,
    Interactive,
}

pub struct Cli {
    pub mode: Mode,
    pub yes: bool,
    pub dry_run: bool,
    pub edit: bool,
    pub context_entries: usize,
    pub no_history: bool,
    pub model: Option<String>,
    pub n_threads: Option<usize>,
    pub n_gpu_layers: i32,
    pub context_size: u32,
    pub temperature: f32,
    pub json: bool,
    pub no_color: bool,
    pub log_level: Level,
}
```

- [x] `clap` derive struct, `-c` required unless another mode flag is present
- [x] Mode selection: exactly one mode, error listing the valid ones otherwise
- [x] `#[clap(long)]` on everything; no positional arguments
- [x] Validation: `context_entries` ≤ 1000, `temperature` in 0.0..=2.0,
      `context_size` in 512..=32768
- [x] `Mode` implements `Display` for the `--json` output
- [x] **Test:** every invalid combination has a specific error message
- [x] **Test:** `gcode --version` and `gcode --help` are covered

Verified on macOS aarch64: 54 unit tests, 2 binary tests, 5 integration. Every
mode pair is checked, and each range is tested at and past its bound.

Three deliberate departures from the sketch above:

- **`Cli` and `Parsed` are separate types.** `Cli` is what clap fills in; `Parsed`
  is the validated result. Range and mode checks run in between, so nothing
  downstream re-checks them. A `Parsed` cannot be built without passing
  validation, which is the point of having it.
- **The conflict error names flags, not modes.** Reporting "generate" to a user
  who typed `-c` makes them search the help text for a flag that does not exist.
  The error says `--command/-c, --fix`.
- **`ParseOutcome` separates "answered" from "wrong".** clap delivers `--help`
  and `--version` through its error channel, so a naive caller exits 2 on
  `gcode --version` and every script that probes the binary sees a failure. The
  first draft did exactly that; the smoke test caught it, not the unit tests.

`Doctor` is deferred to 1.8 rather than added here: it needs the config, model,
and history layers to report on, so a mode that exists before those would have
nothing to say.

### Note on `--temperature` and leading hyphens

`--temperature -0.1` is parsed by clap as the start of another flag, so the
value never reached the range check and the user was told about an "unexpected
argument '-0'" — a message that says nothing about the real problem. The flag
carries `allow_hyphen_values` so the value arrives intact and the range check
produces the error the user can act on. There is a test for both the
space-separated and the `=`-separated form.

### 1.2 Config layer — `src/config.rs`

- [x] `Config` struct, every field `Option<T>` (defaults live in one place)
- [x] Load order: defaults → config file → env vars → CLI flags
- [x] Missing file is not an error; malformed file is an error naming the path
- [x] `Config::resolve()` returns a fully-defaulted `EffectiveConfig`
- [x] **Test:** a fixture config file with one bad field reports that field
- [x] **Test:** env var `GCODE_MODEL` beats the file, CLI beats both

Beyond the listed tasks, this slice also had to:

- move the numeric defaults out of `clap` and into `config::defaults`, because
  a flag carrying `default_value_t` cannot be distinguished from a flag the user
  typed, so "CLI beats the file" was untrue for every numeric setting
- add `--no-git` and `--no-env`, which USAGE.md documented and Phase 1.1 omitted
- refuse unknown config keys, on the grounds that a silently ignored setting is
  worse than a rejected one

### 1.3 Model registry — `src/model/registry.rs`, `models/registry.toml`

- [x] Parse `registry.toml`; embed with `include_str!` so there is no runtime file
- [x] Validate at compile time that exactly one entry sets `default = true`
- [x] `ModelEntry { name, url, sha256, size_bytes, default, context_size, license }`
- [x] A `ModelEntry` without `sha256` is a hard parse error
- [x] **Test:** a fixture registry with two defaults fails
- [x] **Test:** a fixture registry with a missing hash fails
- [ ] 🔒 **The registry data itself is a placeholder** *(needs a real model and
      a checksum you have verified by downloading the file once)*

The code is done. What is not done is the data, and the difference is not
cosmetic: a checksum is a claim about the bytes of a specific file, and there is
no way to write one honestly without the file. The shipped
`models/registry.toml` therefore holds one entry that is deliberately useless —
the SHA-256 of the empty string, an `.invalid` host, and `PLACEHOLDER` in the
name — and `build.rs` refuses a release build while that digest is present. A
debug build and the whole test suite work, so nothing upstream is blocked, and no
artefact a user could install can be produced from it.

Two additions beyond the listed tasks:

- `description` and `recommended_threads` are accepted, because `docs/models.md`
  documents them and `--list-models` needs the first. The roadmap's field list
  was the incomplete one.
- Model names may contain dots. `qwen2.5-0.5b-instruct` and `gemma-2-2b` are
  the names of models this project intends to ship, and a kebab-case rule that
  rejected them would have been weakened at the first conflict.

### 1.4 Model download — `src/model/download.rs`

- [x] `resolve_path` honours `$GCODE_MODEL` and config overrides and search
      dirs (injected `ResolveInput`; an explicit path is taken at face value,
      bypassing hashing at startup)
- [x] Already present + hash matches → return immediately, no network
- [x] Present + hash differs → delete, then download
- [x] Download to `<name>.part`, streaming, with progress to **stderr** (via a
      callback)
- [x] Hash while streaming, not after, so the file is never read twice
- [x] Verify → atomic `rename` into place, set `chmod 0644` on Unix
- [x] Mismatch → delete, try the mirror, then error with both hashes printed
- [x] HTTP Range resume when `.part` exists and the transport supports ranges
- [x] The network is behind a `Transport` trait so the module is testable
      entirely offline; the real HTTPS transport (`ureq`, behind the `download`
      feature) implements it in this same file, the only file CI allows to name
      an HTTP client (ADR 0021)
- [x] `--download-model [NAME]` wired through `runtime::download_model`, with
      `--download-model` choosing the configured model and the value choosing a
      named one; a build without the `download` feature reports
      `DownloadUnavailable` rather than doing nothing (ADR 0021)
- [x] **Test:** correct file downloaded, path created, digest verified
- [x] **Test:** server returns wrong bytes → partial deleted, error with the hashes
- [x] **Test:** pre-existing `.part` resumes from the right offset
- [x] **Test:** mirror fallback, wrong-size file replaced, oversized partial
      discarded, dropped connection leaves the prefix and the error explains
      that the next attempt resumes
- [x] `sha256_hex` exposed for tests; the SHA-256 of empty string matches the
      placeholder sentinel used by the registry gate

### 1.5 Inference engine — `src/inference/engine.rs`

The key design decision, and the one that makes the rest of the project
testable:

```rust
pub trait InferenceEngine: Send + Sync {
    fn generate(&self, prompt: &str, params: &GenParams) -> Result<String>;
    fn info(&self) -> EngineInfo;
}

pub struct LlamaEngine { /* mmap'd model */ }
```

Everything downstream of the prompt takes `&dyn InferenceEngine`. Tests use
`FakeEngine`, so the whole pipeline is testable in milliseconds without 400 MB.

- [x] `InferenceEngine` trait as above, with `EngineInfo` and `GenParams`
- [ ] `LlamaEngine::load(path, ModelParams)` — mmap, warm up — **written
      (`src/inference/engine.rs`), compiles, passes clippy, and is wired into
      `main` under the `inference` feature; ⛔ never run against real weights, so
      not ticked.** A missing model file gives a clear error in the real binary
      (checked). The `#[ignore]`d tests in `tests/engine.rs` run with
      `GCODE_TEST_MODEL` and are what ticks this
- [x] Load once per process, cache behind a `OnceLock`; a second `install` is
      refused so test order cannot poison the cache
- [x] `generate()` with temperature, top_p, max_tokens, seed; params validated
      before the engine is called, so a zero `max_tokens` never reaches a model
- [ ] Stop on EOG, on a newline that closes the command, or at max_tokens —
      the newline rule (`inference/stop.rs`, quotes, backslash, trailing `|`/`&&`)
      is tested without a model; EOG and `max_tokens` are in the sampling loop
      and unverified until a model runs it
- [x] Strip markdown fences, `bash` language tags, `Command:` labels, `$ `
      prompts, and a leading prose line — one function, run before
      classification, so `safety` only ever sees the command
- [x] 30-second wall-clock timeout; on timeout, return an error, never a
      partial command. The timer is a real wall clock: the generation runs on a
      detached thread that keeps the engine alive, so a stuck call cannot hold the
      deadline. An earlier version used `thread::scope` and joined on the way out,
      which meant the "timeout" was only reached after the engine returned — a
      regression test pins the difference (old path ~55 ms against a 10 ms limit,
      fixed path returns immediately)
- [x] Output bounded at 8 KiB regardless of what `max_tokens` claims
- [x] **Test:** `FakeEngine` returns a fixed string; pipeline consumes it
- [x] **Test:** post-processing strips fences, tags, labels, and prose, and
      keeps a comment line and an unusual command word
- [x] **Test:** a timeout produces an error, not a truncated command, and returns
      at the deadline rather than after the engine finishes
- [x] **Test:** zero `max_tokens`, zero timeout, and out-of-range `top_p` are
      refused before the engine is called; a runaway generation is refused

### 1.6 Grammar constraints — `src/inference/grammar.rs`

- [ ] Port the bash EBNF from the llama.cpp grammar reference
- [ ] Add a `no-fork-bomb` restriction to the grammar itself
- [ ] Apply via `llguidance`; fall back to unconstrained on grammar failure
- [ ] **Test:** generation with the grammar yields 100 % parseable shell
- [ ] **Test:** the grammar rejects `rm -rf /` before the classifier sees it
- [ ] **Snapshot:** `.snap` files of accepted completions

### 1.7 Prompt assembly — `src/context/prompt.rs`

- [x] `build_prompt(request, context) -> String`, per the format in
      [MODELS.md § Prompt format](MODELS.md#prompt-format); context and caps are
      injected so the builder is testable without a shell or a git repo
- [x] `redact(s)` applied to every history command, every output tail, every
      env-derived string, and the request itself — one function, ADR 0006
- [x] History wrapped in untrusted-data markers with an explicit
      "data not instructions" system line; attribute values escaped so a quote in
      a git branch name cannot close the attribute and escape into the
      instruction region
- [x] Output tails truncated to `output_tail_bytes` (default 2048) **from the
      end**, and redacted *before* truncation — the other order can cut a rule's
      marker off a long line and leave the payload
- [x] History capped at the 15 most recent entries, output lines escaped into
      valid JSON
- [x] **Test:** `sk-…` in history is redacted before assembly
- [x] **Test:** `Authorization: Bearer …` is redacted
- [x] **Test:** a PEM `BEGIN … PRIVATE KEY` block is redacted, terminated or not
- [x] **Test:** a hostile filename stays inside the history delimiters
- [x] **Test:** ordinary commands are not over-redacted (`grep -r 'token=' src/`)
- [x] **Snapshot:** the canonical prompt asserted inline, and for 5 contexts

### 1.8 Assembly and `main.rs`

- [x] `run(parsed, engine, consenter, always_confirm) -> Result<Output>` in
      `src/runtime.rs`, so it is testable without a process boundary. It takes
      its engine and its consenter as parameters rather than reaching for
      globals, which is what lets `tests/runtime.rs` run the whole safety
      pipeline against a `FakeEngine` with no model and no terminal
- [x] `main.rs` is a thin `main` that calls `run`, prints, and maps the error
- [x] `--dry-run` returns the command and the risk line without prompting
- [x] `--json` emits a single JSON object, no colour, no prompts. Hand-built in
      `Output::to_json` for the same reason the prompt is: no serde at MSRV 1.75
- [x] **Test:** `run()` with a `FakeEngine` produces the expected `Output`
      (`tests/runtime.rs::run_with_a_fake_engine_produces_the_expected_output`)
- [x] Every generative mode (`--generate`, `--fix`, `--complete`) assembles its
      prompt with `build_prompt`, so the request and any failure output are
      redacted (ADR 0006) and wrapped in the system instruction rather than the raw
      string being handed to the model. The environment `Context` stays default
      until Phase 1.5 wires it

### Acceptance criteria

- [ ] `gcode -c "list all files" --dry-run` prints a valid, plausible command —
      **blocked on the model.** The run loop is built and tested against a
      `FakeEngine`, but no sampler exists (1.5), so the CLI reports "no model is
      loaded" and exits 1 rather than inventing a command
- [ ] `gcode -c "find files larger than 10GB" --dry-run` produces a `find` —
      blocked on the model, same reason
- [ ] 10 of 10 commands in `tests/fixtures/bench_prompts.json` are syntactically
      valid shell — blocked on the model. The grammar that guarantees this is 1.6,
      which depends on 1.5
- [ ] p50 inference < 1000 ms on 2 physical cores
- [ ] Grammar prevents invalid shell in 100 % of the 200-run fuzz test — blocked
      on 1.6
- [ ] No history secrets appear in any snapshot — Phase 2; there is no history
      store yet, so there is nothing to snapshot. The prompt-side half is done:
      `context::redact` has 24 tests, including a canonical snapshot that asserts
      a synthetic `sk-…` value never reaches the prompt
- [x] Unit test coverage ≥ 80 % for the modules above — measured with
      `cargo llvm-cov`: `runtime.rs` 94.23 %, `ui/prompt.rs` 82.37 %,
      `safety/patterns.rs` 98.33 %, `safety/classifier.rs` 99.20 %,
      `safety/mod.rs` 100 %; workspace total 92.57 % of lines. The two below 100 %
      are short-circuit right-hand sides the corpus does not exercise in both
      directions, not unexecuted code: no line in `classifier.rs` is uncovered.
      Getting `ui/prompt.rs` above 80 % required making its input and editor
      injectable — see the note on 3.6

### 1.9 Executor — `src/exec/runner.rs`

Added after the plan was written: the architecture drew the stage, the roadmap
never listed it, and until now the gate always reported `executed: false`
([ADR 0023](adr/0023-executor-inherits-stdio.md)).

- [x] `Executor` trait and `ShellExecutor`: `sh -c`, inherited stdio, exit status
      (`128 + signal` when killed)
- [x] The gate runs a command only with a `Runner`, after the block, `--dry-run`,
      and consent, and re-checks the level of the final command before the call
- [x] `main` builds a `Runner` only for an interactive terminal without `--json`
- [x] An executed command is appended to history with its status; a command that
      did not run is not
- [x] `exit_code` in `--json` (`null` when nothing ran)
- [x] **Test:** `CRITICAL` never reaches the executor under `--yes`, `-y`,
      `--dry-run`, or an edit; nor does a blocklisted command or a denied one
- [x] **Test:** `--dry-run`, `-n`, `--complete`, and no `Runner` never execute
- [x] **Test:** the edited command is what runs; a status is not an error
- [x] **Test:** the real `ShellExecutor` through the gate (`exit 5` → `Some(5)`)
- [x] Measured: `exec/runner.rs` 88.10 % of lines (`cargo llvm-cov`)
- [ ] End to end in the real binary — **⛔ needs the engine (1.5)**: every mode
      that reaches the gate needs a model, so `main`'s wiring is compiled and
      reviewed but not exercised by a run
- [ ] Output capture and a timeout — not built, by decision (ADR 0023)

---

## Phase 2 — Shell integration & history

**Status: 🟢 2.1–2.6 shipped; two acceptance criteria wait on CI billing and the Phase 1.5 engine.**
**Goal: `gcode --fix` works because the tool knows what just failed.**

### 2.1 History storage — `src/context/history.rs`

One JSON object per line, append-only, `~/.gcode/history.jsonl`, mode `0600`.

```jsonl
{"ts":1699999999,"cmd":"ls -la","exit":0,"cwd":"/home/u","out":""}
{"ts":1700000005,"cmd":"pytest -q","exit":1,"cwd":"/home/u/p","out":"ModuleNotFoundError: foo"}
```

- [x] `HistoryEntry { ts, cmd, exit, cwd, out }` with serde
- [x] `read_last(n)` reads **backwards** and stops — a 2 GB history file must not
      be fully parsed to get 15 entries. It seeks backwards a chunk at a time and
      stops at the `n`-th newline, so the cost is the size of the tail, not of the
      file
- [x] A malformed line is skipped, not fatal; warn once on stderr
- [x] A torn final line (from a Ctrl-C) is silently discarded
- [x] `append()` is one `write` + `fsync` of a single line
- [x] Rotation at 10 MB: keep the newest 5 MB, suffix `.1`
- [x] Create the data dir with `umask 0077` — **done differently.** The store sets
      the mode explicitly (`0700` on the directory, `0600` on the file) rather than
      setting a process-global umask. A library that calls `umask` changes the mode
      of every file the *host* process creates afterwards, which is not a decision a
      history store gets to make on its own. Recorded in
      [ADR 0017](adr/0017-explicit-file-modes-over-umask.md), which amends rule 6
      of ADR 0005. It is stricter in one way: `umask` only affects files being
      *created*, so a pre-existing `0644` file — from an older version, `touch`, or
      a restored backup — stayed loose. The store tightens it on every append
- [x] **Test:** read last 3 of 100 entries
- [x] **Test:** a corrupt line at position 50 does not stop the read
- [x] **Test:** a 10 MB fixture rotates and stays under the cap
- [x] **Test:** the file mode is `0600` after the first append
- [x] **Test, beyond the list above:** 100 concurrent appends produce intact lines;
      a nested path creates its parents; an entry longer than the read chunk is
      still found; and six failure-injection cases (a path whose parent is a regular
      file, and a directory where the file should be) report `Error::History` rather
      than a silent empty history. 29 tests in `src/context/history/tests.rs`

**Not done, and deliberately so:** nothing reads or writes this store yet. The
prompt does not consume history and no executor appends to it, because both are 2.3
and Phase 4. Building the store first is what lets the shell hook be written against
a tested format.

### 2.2 Environment context — `src/context/env.rs`

- [x] `cwd`, `os`, `arch`, `shell`, `$SHELL` version
- [x] Git: `branch`, `dirty`, `last commit subject` — **done differently.** The
      budget is 250 ms, not the 30 ms written here first, and that change is
      measured rather than preferred. Two `git` calls are issued together, so the
      cost is one spawn rather than two, but even then the pair costs 20–30 ms on an
      idle machine and ~40 ms under a 16-way CPU load. A budget below the cost of
      the work is a budget that always expires, and an always-expired probe is a
      feature that never runs. The deadline still exists and still kills the child;
      250 ms is roughly ten times the worst observed cost and still far below where a
      person notices. Two of the three facts are read from one
      `git status --porcelain=v2 --branch`, and a repository with no commits — every
      new project — reports its branch and no subject rather than nothing
- [x] Silent failure outside a repo. `git` exits 128 and prints nothing there, so the
      probe reads the **exit status**, not the emptiness of stdout: an empty stdout
      is also the correct answer to "which files changed?" in a clean repository, and
      treating it as failure — or, worse, as success — is a wrong answer in both
      directions
- [x] `GCODE_NO_HISTORY`, `GCODE_NO_GIT`, `GCODE_NO_ENV` respected
- [x] **Test:** a fixture repo yields the right branch and dirty flag
- [x] **Test:** outside a repo, no error and no output
- [x] **Test, beyond the list above:** 20 tests in `src/context/env/tests.rs`,
      including a detached HEAD reported as no branch rather than a branch named
      `detached`; a repository with no commits; a probe that overruns its deadline
      returning rather than blocking; `$SHELL` reduced to a program name; a
      four-line `bash --version` reduced to one line; `GCODE_NO_GIT` suppressing only
      git while leaving cwd and platform intact; and a commit subject containing
      both an XML attribute break and a secret-shaped value arriving redacted and
      escaped

**Partly wired.** `build_prompt` is now called by `--generate`, `--fix`, and
`--complete`, and `--fix` reads the history store, so the request, a failed
command, and its output are all redacted before the model sees them. The
*environment* half is still ahead: the run loop builds no environment `Context`,
so prompts are assembled with `Context::default()` and nothing calls
`EnvSnapshot::collect` at runtime until Phase 1.5 wires the engine and its context.
Measured coverage: `context/env.rs` 96.34 % of lines, and the workspace total is
above 92 %.

### 2.3 Bash hook — `shell/gcode.bash`

- [x] Captures the last command, `$?`, `cwd`, and `ts`
- [x] **Preserves the user's `$?`** — `local status=$?` on one statement, and the
      status is returned before the hook returns. A dedicated test asserts a
      failing command is recorded with its real code and that the user still sees
      their own `$?`
- [x] Pre-existing `PROMPT_COMMAND` is chained, never replaced. Both the string
      form and the array form (bash 5.1+) are handled
- [x] Skips commands starting with `gcode` or `_gcode`, and skips a repeat of the
      previous entry so an empty prompt does not duplicate it
- [x] Respects `GCODE_NO_HISTORY=1`, read per prompt rather than at load time
- [x] Under 5 ms; no `jq` dependency at all. Measured 2.2 ms per prompt on macOS
      system bash 3.2 (best of 5 trials)
- [x] Idempotent install: the `_GCODE_HOOK_LOADED` guard makes a second `source` a
      no-op
- [x] **Test:** sourced in a real bash, a failing command is recorded with the
      right exit code and the user still sees their own `$?` — 27 tests in
      `tests/hook_bash.rs`, all against `/bin/bash` 3.2

**Deviation: output is not captured.** The acceptance criterion above says "the
last 2 KB of output". Capturing output means redirecting the command's stdout, which
makes `[ -t 1 ]` false for everything the user runs — `ls` drops its colour, `less`
stops paging, and an editor refuses to start. That was measured, not assumed. A
history hook that degrades every command in the terminal is a worse trade than a
missing error message, so `out` is written empty and `--fix` will work from the
command and its exit status. Revising this is a decision, not an implementation
detail; it would need either a PTY-based capture or an ADR.

**Correction: the hook is installed *before* the user's entries.** Appending it looks
politer, but `PROMPT_COMMAND` entries run in sequence and each sees the previous
one's status, so an appended capture records whatever the user's own hook last
returned. With a `PROMPT_COMMAND` returning 7, every command in the store read
`"exit":7`. That is caught by a test.

### 2.4 Zsh hook — `shell/gcode.zsh`

- [x] `precmd_functions` integration, array append, not a bare `precmd` override
- [x] Same guarantees as bash. Measured 1.3 ms per prompt against the same 5 ms
      budget
- [x] **Test:** the same suite, sourced in a real zsh — 24 tests in
      `tests/hook_zsh.rs`. Every test clears `HISTFILE` and passes `--no-rcs`, so
      nothing reads the developer's `.zshrc` or their real history

**zsh requires one different variable name.** `status` is a read-only special
variable in zsh aliased to `?`, so `local status=$?` fails on the first line of the
hook: the hook errors on every prompt and records nothing. The exit code is captured
into `ret`. There is a test that fails if a `read-only variable` diagnostic ever
appears in a prompt again.

**Appending to `precmd_functions` is safe here, unlike bash.** zsh restores `$?` for
each `precmd` hook, so a user's `precmd` that returns non-zero cannot corrupt the
recorded status. Asserted rather than assumed, by a test with a `precmd` hook that
returns 9.

### 2.5 `--init`

Implemented as three mutually exclusive modes — `--init`, `--check`, `--remove` —
plus `--shell <bash|zsh>` to override the shell detected from `$SHELL`.

- [x] Detect the shell from `$SHELL`; support `--shell` to override. An
      unrecognised `$SHELL` is refused with `Error::UnsupportedShell` naming the
      shell rather than guessing; `--shell fish` is refused at the argument layer
      with the two accepted values
- [x] Append a marked block, idempotently. The block is four lines, because a
      version line is needed for `--check` to report *current* versus *out of date*:

```
# >>> gcode init >>>
# gcode hook 0.1.0
source '/home/user/.gcode/shell/gcode.bash'
# <<< gcode init <<<
```

- [x] The `source` line points at the gcode-owned copy under `~/.gcode/shell/`,
      not at wherever the binary happens to live. The hook text is embedded in the
      binary at compile time and written there on install, so moving or
      repackaging the binary cannot break a user's prompt. `--remove` deletes the
      block and then that file
- [x] `--check` reports installed/version/missing without changing anything:
      `not installed`, or `installed (current, version 0.1.0)` /
      `installed (out of date, version X)`
- [x] `--remove` deletes exactly the marked block, byte-precise. Removing the
      block also removes exactly one preceding newline — the separator install
      added — so install followed by remove restores the file byte-for-byte in
      every case tested: empty file, a lone newline, a file with no trailing
      newline, and a file with trailing blank lines
- [x] Refuses to write a file it cannot read: an rc file that exists but cannot be
      opened is `Error::ShellRcUnreadable`, and nothing is written
- [x] A begin marker with no end marker is refused with
      `Error::ShellBlockMalformed`, never repaired. Marker matching is line-exact,
      so a hand-edited lookalike is not treated as the block
- [x] Prints the exact lines it added, and only those; in `--json` the result
      carries `"explanation"` alongside the otherwise-empty command shape
- [x] **Test:** install twice → file is byte-identical to install once — plus the
      reverse order, remove from a file that never had the block (a no-op), and a
      round trip over a file whose only content is the block
- [x] **Test:** remove after install → file is byte-identical to before
- [x] **Test:** a user's own `PROMPT_COMMAND` survives install and remove. The
      installer only ever appends and removes its own four lines; it never parses
      or rewrites another line
- [x] **Test, beyond the list above:** 45 tests in `src/shell/tests.rs` and the
      hook-mode cases in `tests/runtime.rs`, driven through the public
      `runtime::shell(mode, &Installer)` seam against temporary directories. These
      cover quoting of a hook path containing a space or quote, an unknown shell,
      an unreadable rc file, an install onto a file with no trailing newline, and
      marker matching that must not fire on a lookalike line.

**Deviation from ADR 0007: `--init --remove` together is refused.** ADR 0007 shows
that spelling, but the three flags are implemented as modes and any two of them
conflict, so `--init --remove` is a usage error (`tests/runtime.rs`,
`a_hook_mode_reaches_the_conflict_error`). Treating two contradictory verbs as
"remove wins" or "init wins" would silently do the opposite of what half the
command line asks. The ADR is not edited; this is recorded here and in the
CHANGELOG.

**Not verified by a test, and reported honestly:** the integration test that would
have driven the hook modes through `runtime::run` was removed after it was found to
resolve the real `$SHELL` and the real home directory and install the hook into the
developer's actual `~/.bashrc`. The hook-mode tests call `runtime::shell` directly
with an injected `Installer` instead. The accidental install was removed
byte-precisely by running `--remove --shell bash`; nothing else was touched. There
is deliberately no test that runs the whole binary against the real home.

### 2.6 `--fix` and `--complete`

- [x] `--fix` picks the most recent entry with `exit != 0`, feeds the error
      output to the model, prompts with a diff against the original. Implemented
      in `runtime::fix`: it reads through `History::read_last(200)`, assembles the
      prompt with `context::prompt::build_prompt` (so ADR 0006 redaction and
      escaping apply to the failure output like any other context), and passes the
      model's answer through the same `gate` as any other generation. The
      explanation is a `- failed` / `+ repaired` diff
- [x] No failed entry in history → clear message, exit 0
- [x] `--complete` sends the partial command and returns the completed command.
      **Resolved:** the literal "returns only the continuation" wording was set
      against the full-command output documented in [USAGE.md](USAGE.md),
      `docs/gcode.1`, and `plan.md`, and against safety invariant 1 in
      [TESTING.md](TESTING.md): emitting a bare suffix would mean the classified
      string and the emitted string differ. The decision is to emit and classify
      the full completed command, and to feed the partial to the model through
      `context::prompt::build_prompt` so it is redacted before the prompt like any
      other shell text (ADR 0006)
- [x] **Test:** `--fix` with a fixture failed entry produces a command that
      differs from the original
      (`fix_picks_the_most_recent_failure_and_asks_for_a_repair`)
- [x] **Test:** `--fix` with clean history is a no-op with a clear message
      (`fix_with_clean_history_is_a_no_op_with_a_message`)
- [x] **Test:** the failure output is redacted before it reaches the prompt
      (`fix_redacts_a_secret_in_the_failure_output_before_the_prompt`)
- [x] **Test:** a repaired command that classifies `CRITICAL` is refused
      (`a_fix_that_returns_critical_is_refused`)

### Acceptance criteria

- [ ] `gcode --init` then a failing command then `gcode --fix` produces a
      corrected command — the 2.6 code path is done and unit-tested against a
      `FakeEngine`; the end-to-end run needs the real engine (Phase 1.5)
- [x] The user's shell prompt and `$?` are unaffected — asserted against real
      `bash` and `zsh`
- [x] Install is idempotent; remove restores the file exactly
- [x] History file is `0600` and rotates at 10 MB
- [x] Works in both bash and zsh, tested by sourcing real shells — run on Linux
      x86_64 with bash 5 and zsh 5.9: 27 bash and 24 zsh hook tests pass with
      `GCODE_REQUIRE_SHELLS=1`. Run through `scripts/ci.sh`, not a hosted
      workflow ([ADR 0019](adr/0019-local-ci-not-github-actions.md)).

---

## Phase 3 — Safety layer

**Status: 🟡 3.1–3.5, 3.7, 3.8 shipped. 3.6 mostly; one item open (the `MEDIUM+`
cost estimate, which needs a model).**
**Goal: a CRITICAL command is unrunnable, and editing cannot bypass it.**

See [SAFETY.md](SAFETY.md) for the full model. This is the build list.

### 3.1 Risk types — `src/safety/mod.rs`

- [x] `enum Risk { Safe, Low, Medium, High, Critical }`
- [x] `Ord` so levels compare; `Display`; `as_str()` gives `SAFE`…`CRITICAL`.
      Serde is deferred to the `--json` layer, which hand-builds its object like
      `context/prompt.rs` does, to keep the dependency count down at MSRV 1.75
- [x] `struct Verdict { level, reasons: Vec<Reason>, segments: Vec<Segment> }`
- [x] `struct Reason { pattern_id, level, message, segment }`; `Segment` keeps its
      own level so the UI can point at the offending half of a command
- [x] `Risk::is_runnable()` — the single place that answers "may this run". No
      flag, config key, or env var reaches it

### 3.2 Normalisation — `src/safety/classifier.rs`

- [x] Strip comments: `#` at a token start, outside quotes. A `#` inside a word
      (`foo#bar`) or inside quotes (`grep '#1' .`) survives
- [x] Collapse whitespace. `~` and `$HOME` are treated as one home path by the
      matcher rather than textually expanded to a guessed directory; `$VAR` and
      `${VAR}` are resolved from an injected map when set
- [x] Join line continuations (`\` at EOL)
- [x] **Split on `;`, `&&`, `||`, `|`, `&`, newline, and classify every segment**
- [x] Respect quotes: do not split on a `;` inside `'…'` or `"…"`
- [x] Track brace depth, so a function body stays one segment. A splitter that
      ignores braces would break the fork bomb into harmless pieces
- [x] **An unset `$VAR` stays literal.** Blanking it would turn `rm -rf $TARGET`
      into `rm -rf`, which reads as harmless
- [x] `2>&1` is not a split point

### 3.3 Blocklist

- [x] Built-in set: `rm -rf /`, `mkfs*`, `dd of=/dev/sd*`, `> /dev/sd*`, fork bomb
      `:(){ :|:& };:`, `chmod -R 777 /`, `rm -rf ~`, `tee`/`cat` onto a raw device.
      `/dev/null`, `/dev/zero`, and `/dev/std*` are excluded on purpose: a
      blocklist that fires on `echo x > /dev/null` is one people turn off
- [x] User blocklist merged from `[safety.blocklist]`; it can only raise a level
      to CRITICAL, never lower one
- [x] Matched **before** level assignment; result is CRITICAL
- [x] The fork bomb is matched against the **whole command**, not one segment.
      Bash lexes the trailing `;` as a separator, so a per-segment pass sees
      `:(){ :|:& }` and `:` and misses it
- [x] **Test:** each built-in pattern, with a segment-splitting variant
- [x] **Test:** `ls; rm -rf /` → CRITICAL (the case most implementations miss),
      and the same for `&&`, `||`, `|`, `&`, and a newline
- [x] **Test:** a dangerous command quoted as text (`echo 'rm -rf /'`) is **not**
      a command

### 3.4 Pattern table — `src/safety/patterns.rs`

- [x] The full table from [SAFETY.md § classifier](SAFETY.md#stage-3--patterns-and-structure),
      39 rows across CRITICAL, HIGH, MEDIUM, and LOW
- [x] Every entry carries a user-facing reason string that says what the command
      does, never just that it is risky
- [x] Patterns are data, not code: a new row is a `Pattern` literal plus a test,
      with no logic change
- [x] Matched by an ordered token matcher rather than a regex, for three reasons
      documented at the top of the module: no dependency in the safety-critical
      path, near misses decided by exact tokens rather than by anchoring, and no
      regex engine to audit. Command names are anchored to the head token, so
      `echo rm -rf /` does not match a root-delete rule
- [x] Each pattern has at least one positive test and one near-miss negative
      test, and a test asserts the declared test list matches the table exactly,
      so a new row without tests fails the build
- [x] An unknown-command floor: a command whose head is neither in the table nor
      in `KNOWN_SAFE` is `LOW`, never `SAFE`. This is an addition beyond SAFETY.md
      — a table that calls unknown commands SAFE fails open, which is the wrong
      direction for the module whose job is not failing open

### 3.5 Structural checks

- [x] **Taint:** a write to path P followed by a delete of P → escalate one level.
      Handles `cp`/`mv`/`install`/`ln`/`rsync` destinations, `tee` arguments,
      `dd of=`, and redirect targets; compares paths with quotes stripped
- [x] **Privilege:** `sudo`/`doas`/`pkexec`/`runuser` anywhere → at least MEDIUM.
      `su` only as the head command, because `grep su notes.txt` is not an
      escalation
- [x] **Network + execute:** fetch chained with execute → at least HIGH, checked
      **across segments** (`curl x | bash` is two segments, so no per-segment rule
      can see it) and via `xargs` in one segment

### 3.6 Confirmation UI — `src/ui/prompt.rs`

- [x] Keys: `y n e c ? r`. All are handled; `c` copies to the clipboard by
      shelling out to the platform tool (ADR 0020)
- [x] `Ctrl+C` cancels — resolved as the **operating-system default**, with no
      handler installed (ADR 0022). The crate forbids the `unsafe` a `libc`
      handler needs, a `ctrlc` handler is process-wide and would swallow Ctrl+C at
      the prompt (where `read_line` retries on `EINTR`, so a flag cannot unblock
      it), and the default already terminates with `128 + SIGINT` and leaves the
      resumable `.part` intact. A custom message would not be worth that
      regression
- [x] Shows the command, the level, and every reason. The reasons are not
      decoration: "MEDIUM" alone does not tell a user whether it is their own
      project directory or someone else's home
- [ ] A cost estimate for `MEDIUM+` — **not implemented.** There is no model, so
      there is nothing to estimate, and inventing a unit would put a fabricated
      number in front of a user about to make a safety decision
- [x] `?` prints a plain-language explanation of each matched pattern, and the
      prompt loops rather than exiting
- [x] `e` opens `$EDITOR`, and the core **re-classifies** the result from scratch
      before anything looks at it. The edited string travels back as
      `Decision::Granted(String)`, not as a bare yes, so the core cannot run the
      pre-edit command while believing it reviewed the post-edit one. Rejection
      logic lives in `runtime.rs`, never in `ui/`, because a rule a UI can bypass
      is not a safety rule (ADR 0004)
- [x] …and loops back to the prompt, so `e` can be repeated. The edited text is
      re-classified *for display* and re-rendered before the question is asked
      again, so the user cannot answer `y` to a command that is no longer the one on
      screen. The core still classifies independently before acting. An edit that
      fails, or produces an empty command, denies rather than falling back to the
      original. 18 tests in `src/ui/prompt.rs`, reachable only after the prompt's
      input and editor were made injectable — reaching them through a real terminal
      or `std::env::set_var` was not an option
- [x] `c` copies to the clipboard by shelling out to the platform tool
      (`pbcopy`, `wl-copy`, `xclip`, `xsel`, in that order), the same pattern as
      `e` and `$EDITOR`, so no dependency is added (ADR 0020). The copy is a
      convenience and never consent: it does not grant, and a failed copy is
      reported and re-asked rather than swallowed. Tests inject the copy function
      and never start a real clipboard process
- [x] Default is `N` at every level. There is no branch in `ui/prompt.rs` that
      turns "I could not ask" into permission: end of input, an empty line, an
      unrecognised key, a closed stdin, a read error, and an editor that fails all
      return `Decision::Denied`
- [x] Non-interactive stdin fails safe. `Prompt::ask` checks `is_terminal()` before
      printing anything, and `main` passes `DenyAll` for a pipe or `--json`, so a
      machine-readable invocation cannot block on a human
- [x] **Test:** `e` that turns a HIGH command into a CRITICAL one is re-blocked
      (`tests/runtime.rs::an_edit_that_turns_high_into_critical_is_re_blocked`)
- [x] **Test:** piped stdin does not execute
      (`tests/safety.rs`, `tests/runtime.rs::a_pipe_cannot_run_a_command_that_needs_consent`)

### 3.7 `--yes` semantics

- [x] Suppresses the prompt only. `needs_confirmation` is `!yes && level >= always_confirm`
      and nothing else reads `yes`
- [x] Still classifies, still reports the level and reasons. Logging to the
      history store is Phase 2; the level and reasons are in `--json` and on stderr
- [x] CRITICAL still refuses, and the block happens *before* the prompt is reached,
      so there is no order in which `--yes` sees it first
- [x] **Test:** `--yes` on a CRITICAL command still refuses, exit code non-zero
      (`no_flag_combination_unblocks_critical`, and the CLI exits 1)
- [x] **Test:** `--yes` on a HIGH command runs
      (`yes_replaces_the_prompt_but_not_the_classification`). The history half is
      Phase 2, so the level cannot yet be recorded

### 3.8 `--explain`

- [x] Given a command, prints what it does, what it touches, its level, its
      per-segment levels, and every reason with the pattern id that produced it.
      `Verdict::touched_paths` reads the *normalised* segments, so a path in a
      stripped comment is not reported as touched and one inside a quoted argument
      still is
- [x] Never executes, never suggests. `executed` is always false in this mode and a
      test greps the output for suggestion phrasing, because an `--explain` that
      proposed a different command would be a second generator whose output nothing
      has classified
- [x] **Snapshot:** explanations for 10 canonical commands
      (`tests/runtime.rs::the_ten_canonical_explanations_are_stable`) — asserted
      stable across runs and complete in structure. A committed golden file is not
      used: ten explanations that only change when someone edits them are ten
      expected strings to keep in sync by hand, and the stability check catches the
      regression that matters, which is non-determinism
- [x] `--explain` needs no model, so it is the one mode that works before 1.5

### Acceptance criteria

- [x] `safety/classifier.rs` has 100 % statement coverage — measured with
      `cargo llvm-cov`: **no line in the file is uncovered**, and every function is
      called. The 99.47 % region figure is seven short-circuit right-hand sides the
      corpus never drives in both directions, not dead code. Getting here also
      removed an `escalate` helper that had been left with no production caller
      after an earlier taint branch was deleted — a function kept alive only by its
      own unit test is unreferenced code, not covered code
- [x] Every built-in blocklist pattern has a test; every pattern has a
      near-miss negative test (`tests/safety.rs`, plus a table/list equality test
      that fails the build if a row is added without one)
- [x] `gcode -c "delete everything from root" -y` exits non-zero and runs nothing
      (`no_flag_combination_unblocks_critical` sweeps nine flag combinations; the
      exit code is checked in `main`: 1 for a refusal, 2 for a usage error)
- [x] Editing a generated command re-classifies it — proven by purity in
      `tests/safety.rs::editing_a_command_reclassifies_it`. The run loop that
      calls it on every keystroke is Phase 1.8/3.6, and is still open
- [x] Piped stdin never executes anything — `main` hands the run loop a
      `DenyAll` consenter unless a terminal is attached, so a pipe can neither
      reach a prompt nor consent to anything
- [x] Fuzzing 10 000 mutated commands produces zero panics
      (`tests/safety.rs::ten_thousand_mutations_never_panic`, deterministic
      xorshift so a failure reproduces)

---

## Phase 4 — Packaging

**Status: ⬜ not started**
**Goal: `apt install gcode` works, and the one-line installer installs it.**

### 4.1 `cargo-dist`

- [ ] `dist-workspace.toml` with targets: linux x86_64/aarch64 (musl, gnu), macOS
      x86_64/aarch64, and archives + installers
- [ ] Shell completions embedded via `clap_complete`
- [ ] Man page embedded
- [ ] `cargo dist` produces checksums and signatures
- [ ] **Test:** `./scripts/verify-release.sh` validates a built artifact

### 4.2 Debian / Ubuntu

- [ ] `packaging/debian/control`, `rules`, `changelog`, `copyright`
- [ ] `postinst` runs `gcode --init` non-interactively and safely
- [ ] `prerm` removes it
- [ ] A `gcode-model-kitty` package holding the model, as a separate `Recommends`
- [ ] Lint with `lintian` clean

### 4.3 Fedora / COPR

- [ ] `.spec` with `%license`, `%doc`, correct `Requires`
- [ ] A `.copr` file for the COPR project 🔒 *(needs your COPR account)*

### 4.4 Arch AUR

- [ ] `PKGBUILD` with pinned `pkgver`/`pkgsha256sum`
- [ ] `.SRCINFO` generated
- [ ] 🔒 *(needs your AUR account to publish)*

### 4.5 Homebrew

- [ ] `Formula/gcode.rb`, versioned and sha256-pinned
- [ ] A test block that runs `gcode --version`
- [ ] 🔒 *(needs your Homebrew tap)*

### 4.6 AppImage

- [ ] Runtime bundled, FUSE2 fallback documented
- [ ] `AppRun` that sets the model path

### 4.7 One-line installer — `scripts/install.sh`

- [ ] POSIX `sh`, `set -euo pipefail`, no bashisms
- [ ] Detect OS and arch; refuse unknown combinations with a clear message
- [ ] **Verify the SHA256 before extracting anything**
- [ ] `/usr/local/bin` if writable, else `~/.local/bin`, else `sudo`
- [ ] Model download detached so the prompt returns immediately
- [ ] `GCODE_NO_MODEL`, `GCODE_NO_INIT`, `GCODE_INSTALL_DIR` honoured
- [ ] Self-test at the end, with a non-zero exit if it fails
- [ ] **Test:** shellcheck clean; runs in a Docker matrix
- [ ] **Test:** checksum mismatch aborts before writing anything

### 4.8 Docker

- [ ] Multi-stage `Dockerfile`: build, then a slim runtime with the model baked in
- [ ] Non-root `USER`
- [ ] `docker run --rm -it -v ~/.gcode:/root/.gcode …`
- [ ] Image < 700 MB

### Acceptance criteria

- [ ] `.deb` installs and `gcode --version` works on Ubuntu 22.04 and 24.04
- [ ] `.rpm` installs on Fedora 40
- [ ] `PKGBUILD` builds on Arch
- [ ] `brew install` works from the tap
- [ ] `curl … | sh` completes on all 6 CI platforms
- [ ] A checksum mismatch in the installer aborts before any file is written

---

## Phase 5 — Distribution & CI/CD

**Status: ⬜ not started**
**Goal: releases are automatic, signed, and published without a human in the loop.**

### 5.1 Release workflow

- [ ] Tag `v*.*.*` → build matrix → `cargo dist` → sign → checksum → GitHub Release
- [ ] SLSA provenance attestation
- [ ] `--locked` everywhere, so CI cannot resolve a different dependency tree
- [ ] Draft the release notes from `CHANGELOG.md` automatically

### 5.2 Supply chain

- [ ] `cargo-deny` — licence and advisory policy, checked in CI
- [ ] `cargo-audit` — RustSec advisories
- [ ] `cargo-vet` — audited dependency list
- [ ] Gitleaks or equivalent secret scan on every push and PR 🔒 *(needs your
      GitHub org's secret-scanning enabled for full coverage)*
- [ ] Model checksums pinned in a signed file

### 5.3 APT repository

- [ ] GitHub Pages tree: `dists/stable/{Release,Release.gpg}`, `pool/`
- [ ] Signing key 🔒 *(the private key is yours; never in this repo)*
- [ ] Installation instructions verified on a clean Ubuntu VM

### 5.4 Continuous quality

- [ ] Coverage report uploaded, with a diff check that fails on a decrease
- [ ] `cargo audit` gate
- [ ] Test matrix: Ubuntu 20.04/22.04/24.04, Debian 12, Fedora 40, Arch, Alpine
      3.20, macOS 12/13/14 — via Docker for the Linux spread
- [ ] Nightly job: full test suite plus benchmarks, alerts on regression

### Acceptance criteria

- [ ] Pushing a tag produces a signed release with checksums
- [ ] `apt install gcode` works on a clean VM from the APT repo
- [ ] `cargo audit` reports zero unpatched advisories
- [ ] Secret scanning is on and has never fired on a real secret
- [ ] The full matrix passes

---

## Phase 6 — Polish & launch

**Status: ⬜ not started**
**Goal: someone who has never seen gcode can install it and succeed.**

### 6.1 Documentation

- [x] Documentation set written (`docs/**`)
- [ ] Proofread against the built binary, not against the design
- [ ] `docs/gcode.1` man page matches the real `--help` output
- [ ] GitHub Pages site: docs → static site, no build step
- [ ] A `docs/README.md` link check passes in CI

### 6.2 Completions and man page

- [ ] `gcode --generate-completions bash|zsh|fish` from `clap_complete`
- [ ] Man page generated from the same source of truth
- [ ] **Test:** a generated completion file parses (`bash -n`)

### 6.3 README

- [ ] Hero GIF: install → one command → a result, under 15 seconds
- [ ] The one-line install above the fold
- [ ] Three real examples, real output
- [ ] An honest limitations section
- [ ] Comparison table against similar tools, factual and dated

### 6.4 Launch assets

- [ ] Blog post: the design, the safety model, the latency budget
- [ ] `CHANGELOG.md` complete for 1.0.0
- [ ] Short demo script for the HN post
- [ ] 🔒 Posting accounts are yours. The agent prepares the text; you post.

### 6.5 Pre-launch checklist

- [ ] Every acceptance criterion in phases 0–5 is ✅
- [ ] Test coverage > 85 %
- [ ] Benchmarks within the targets in [TESTING.md](TESTING.md)
- [ ] Man page written, completions generated
- [ ] Demo GIF recorded
- [ ] Packages built and signed
- [ ] APT and Homebrew repos live
- [ ] `README`, `CHANGELOG`, `LICENSE`, `SECURITY`, `CONTRIBUTING` all present
- [ ] A release is published and installable from a clean machine
- [ ] **Zero secrets in the tracked tree** — verified by the secret scanner

---

## Phase 7 — Reliability hardening ⛔ planned

- [ ] Crash reporting with **opt-in** local storage only — no network
- [ ] A corpus of 1000 real-world requests with expected behaviour, as a
      regression suite
- [ ] Degradation ladder: model fails → clearly degraded but still safe
- [ ] `gcode bench --compare <model>` to pick a model empirically
- [ ] Cross-shell CI on real images, not just hosted runners

## Phase 8 — Shell coverage ⛔ planned

- [ ] `shell/gcode.fish` using `fish_prompt` events
- [ ] `shell/gcode.nu` using a custom command hook
- [ ] Per-shell history format differences isolated in a `Shell` trait
- [ ] Completion files for both

## Phase 9 — Agentic mode ⛔ planned

- [ ] Plan generation: request → an ordered, inspectable step list
- [ ] Per-step execution with a result check between steps
- [ ] `--rollback` for reversible steps
- [ ] The whole plan is classified once, as a unit, before step one runs
- [ ] **Non-negotiable:** a multi-step plan is never more permissive than the sum
      of its steps. This is the test that must exist.

## Phase 10 — WASM plugin system ⛔ planned

Deliberately last. Do not start this before there are ten real plugins and a
clear, stable core API. Design it then, not now.

---

## Definition of done for v1.0

```
cargo build --release        clean on linux + macos, both arches
cargo test                   green, > 85% coverage
cargo fmt --check            clean
cargo clippy -D warnings     clean
cargo audit                  zero advisories
secret scan                  zero findings
gcode -c "<request>"         produces a valid command in < 1s
gcode --fix                  works from real history
critical commands            unrunnable
install                      one command, 6 platforms
docs                         complete, link-checked, matches reality
```
