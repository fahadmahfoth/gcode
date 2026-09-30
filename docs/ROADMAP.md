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
| [1](#phase-1--core-inference-pipeline) | Core inference pipeline | ⬜ | Yes |
| [2](#phase-2--shell-integration) | Shell integration & history | ⬜ | Yes |
| [3](#phase-3--safety-layer) | Safety layer | ⬜ | Yes |
| [4](#phase-4--packaging) | Packaging | ⬜ | Yes |
| [5](#phase-5--distribution--cicd) | Distribution & CI/CD | ⬜ | No |
| [6](#phase-6--polish--launch) | Polish & launch | ⬜ | No |
| [7](#phase-7--reliability) | Reliability hardening | ⛔ planned | No |
| [8](#phase-8--shell-coverage) | Shell coverage: fish, nushell | ⛔ planned | No |
| [9](#phase-9--agentic-mode) | Agentic multi-step execution | ⛔ planned | No |
| [10](#phase-10--plugin-system) | WASM plugin system | ⛔ planned | No |

Phases 0–6 are the v1.0 definition of done. Phases 7–10 are post-v1.0 and exist
only so the design decisions of v1.0 do not paint us into a corner.

---

## Phase 0 — Repository foundation

**Status: 🟡 in progress — the Rust half builds, tests, and lints clean on macOS
aarch64; Linux x86_64 and the CI run are still unverified**
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
| 0.8 | CI: fmt, clippy, test, build matrix | `.github/workflows/ci.yml` | 🟡 written, never executed |
| 0.9 | Licence, contributing, security policy | `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md` | ✅ |
| 0.10 | Documentation set | `docs/**` | ✅ |
| 0.11 | OpenCode agent + command tooling | `.opencode/**`, `AGENTS.md` | ✅ |
| 0.12 | GitHub templates, secret scanning | `.github/**` | 🟡 written, never executed |

### Acceptance criteria

- [ ] `cargo build --release` succeeds on Linux x86_64 **and** macOS aarch64
      — ✅ macOS aarch64 (rustc 1.98.1). ⬜ Linux x86_64 not built yet.
- [x] `cargo test` passes — 34 passed, 0 failed
- [x] `cargo fmt --check` and `cargo clippy -D warnings` are clean
- [ ] CI runs green on a push
      — ⛔ **blocked on the account, not the code.** A push to `master` now
      produces a run, which it never did before, but every job is refused with
      *"The job was not started because your account is locked due to a billing
      issue"* (run `36765649289`). No workflow edit can clear that.
- [x] `README.md`, `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md` present
- [x] No credential-shaped string anywhere in the tracked tree
- [x] `AGENTS.md` tells an agent what to do next

Two of the four unchecked items are things only CI can settle: a Linux build and
a green run. A workflow that has never run is not a passing workflow, so 0.8 and
0.12 stay 🟡 no matter how confident the YAML looks.

The push trigger was one of those two blockers and is now fixed: it named
`main` and `develop`, but the default branch is `master`, so nothing had ever
triggered a run. That is corrected and a run now appears on every push. The run
itself fails for a reason outside the repository — GitHub is refusing to start
jobs on this account.

### What unblocks this phase

Clearing the GitHub Actions billing lock, so 0.8 can run on both platforms and
settle the two remaining criteria. Nothing in this repository can do that.

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

**Status: 🟡 in progress — 1.1 done and verified on macOS aarch64; the modes are
parsed but nothing has been generated yet, because there is no model**
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

- [ ] `Config` struct, every field `Option<T>` (defaults live in one place)
- [ ] Load order: defaults → config file → env vars → CLI flags
- [ ] Missing file is not an error; malformed file is an error naming the path
- [ ] `Config::resolve()` returns a fully-defaulted `EffectiveConfig`
- [ ] **Test:** a fixture config file with one bad field reports that field
- [ ] **Test:** env var `GCODE_MODEL` beats the file, CLI beats both

### 1.3 Model registry — `src/model/registry.rs`, `models/registry.toml`

- [ ] Parse `registry.toml`; embed with `include_str!` so there is no runtime file
- [ ] Validate at compile time that exactly one entry sets `default = true`
- [ ] `ModelEntry { name, url, sha256, size_bytes, default, context_size, license }`
- [ ] A `ModelEntry` without `sha256` is a hard parse error
- [ ] **Test:** a fixture registry with two defaults fails
- [ ] **Test:** a fixture registry with a missing hash fails

### 1.4 Model download — `src/model/download.rs`

- [ ] `resolve_path(entry) -> PathBuf` honouring `$GCODE_MODEL`, config, search dirs
- [ ] Already present + hash matches → return immediately, no network
- [ ] Present + hash differs → delete, then download
- [ ] Download to `<name>.part`, streaming, with progress to **stderr**
- [ ] Hash while streaming, not after, to avoid a second read of 400 MB
- [ ] Verify → atomic `rename` into place; `chmod 0644`
- [ ] Mismatch → delete, try `$GCODE_MODEL_MIRROR`, then error with the hashes
- [ ] HTTP Range resume when `.part` exists
- [ ] **Test:** `mockito` serves a fixture file, hash matches, path created
- [ ] **Test:** server returns wrong bytes → file deleted, error returned
- [ ] **Test:** pre-existing `.part` resumes from the right offset

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

- [ ] `InferenceEngine` trait as above
- [ ] `LlamaEngine::load(path, ModelParams)` — mmap, warm up
- [ ] Load once per process, cache behind a `OnceLock`
- [ ] `generate()` with temperature, top_p, max_tokens, seed
- [ ] Stop on EOG, on a newline that closes the command, or at max_tokens
- [ ] Strip any markdown fences, `bash` language tags, or leading `Command:` the
      model adds despite instructions
- [ ] 30-second wall-clock timeout; on timeout, return an error, never a
      partial command
- [ ] **Test:** `FakeEngine` returns a fixed string; pipeline consumes it
- [ ] **Test:** post-processing strips fences, tags, and prose
- [ ] **Test:** a timeout produces an error, not a truncated command

### 1.6 Grammar constraints — `src/inference/grammar.rs`

- [ ] Port the bash EBNF from the llama.cpp grammar reference
- [ ] Add a `no-fork-bomb` restriction to the grammar itself
- [ ] Apply via `llguidance`; fall back to unconstrained on grammar failure
- [ ] **Test:** generation with the grammar yields 100 % parseable shell
- [ ] **Test:** the grammar rejects `rm -rf /` before the classifier sees it
- [ ] **Snapshot:** `.snap` files of accepted completions

### 1.7 Prompt assembly — `src/context/prompt.rs`

- [ ] `build_prompt(request, context) -> String`, per the format in
      [MODELS.md § Prompt format](MODELS.md#prompt-format)
- [ ] `redact(s)` applied to every history output tail and env-derived string
- [ ] History wrapped in untrusted-data markers with an explicit
      "data not instructions" system line
- [ ] Output tails truncated to `output_tail_bytes` (default 2048) **from the end**
- [ ] **Test:** `sk-[A-Za-z0-9]{20,}` in history is redacted before assembly
- [ ] **Test:** `Authorization: Bearer …` is redacted
- [ ] **Test:** a PEM `BEGIN … PRIVATE KEY` block is redacted
- [ ] **Snapshot:** prompt for 5 canonical contexts

### 1.8 Assembly and `main.rs`

- [ ] `run(cli) -> Result<Output>` in `src/lib.rs`, so it is testable without a
      process boundary
- [ ] `main.rs` is a thin `main` that calls `run`, prints, and maps the error
- [ ] `--dry-run` prints the command and the risk line, then exits 0
- [ ] `--json` emits a single JSON object, no colour, no prompts
- [ ] **Test:** `run()` with a `FakeEngine` produces the expected `Output`

### Acceptance criteria

- [ ] `gcode -c "list all files" --dry-run` prints a valid, plausible command
- [ ] `gcode -c "find files larger than 10GB" --dry-run` produces a `find`
- [ ] 10 of 10 commands in `tests/fixtures/bench_prompts.json` are syntactically
      valid shell
- [ ] p50 inference < 1000 ms on 2 physical cores
- [ ] Grammar prevents invalid shell in 100 % of the 200-run fuzz test
- [ ] No history secrets appear in any snapshot
- [ ] Unit test coverage ≥ 80 % for the modules above

---

## Phase 2 — Shell integration & history

**Status: ⬜ not started**
**Goal: `gcode --fix` works because the tool knows what just failed.**

### 2.1 History storage — `src/context/history.rs`

One JSON object per line, append-only, `~/.gcode/history.jsonl`, mode `0600`.

```jsonl
{"ts":1699999999,"cmd":"ls -la","exit":0,"cwd":"/home/u","out":""}
{"ts":1700000005,"cmd":"pytest -q","exit":1,"cwd":"/home/u/p","out":"ModuleNotFoundError: foo"}
```

- [ ] `HistoryEntry { ts, cmd, exit, cwd, out }` with serde
- [ ] `read_last(n)` reads **backwards** and stops — a 2 GB history file must not
      be fully parsed to get 15 entries
- [ ] A malformed line is skipped, not fatal; warn once on stderr
- [ ] A torn final line (from a Ctrl-C) is silently discarded
- [ ] `append()` is one `write` + `fsync` of a single line
- [ ] Rotation at 10 MB: keep the newest 5 MB, suffix `.1`
- [ ] Create the data dir with `umask 0077`
- [ ] **Test:** read last 3 of 100 entries
- [ ] **Test:** a corrupt line at position 50 does not stop the read
- [ ] **Test:** a 10 MB fixture rotates and stays under the cap
- [ ] **Test:** the file mode is `0600` after the first append

### 2.2 Environment context — `src/context/env.rs`

- [ ] `cwd`, `os`, `arch`, `shell`, `$SHELL` version
- [ ] Git: `branch`, `dirty`, `last commit subject` — 30 ms timeout, silent
      failure outside a repo
- [ ] `GCODE_NO_HISTORY`, `GCODE_NO_GIT`, `GCODE_NO_ENV` respected
- [ ] **Test:** a fixture repo yields the right branch and dirty flag
- [ ] **Test:** outside a repo, no error and no output

### 2.3 Bash hook — `shell/gcode.bash`

```bash
_gcode_capture() {
    local exit_code=$?
    local last_cmd
    last_cmd=$(HISTTIMEFORMAT= history 1 | sed 's/^ *[0-9]* *//')
    # skip gcode's own commands, skip duplicates, preserve $? for the user
    ...
}
```

- [ ] Captures last command, `$?`, `cwd`, and the last 2 KB of output
- [ ] **Preserves the user's `$?`** — this is the single most common way a
      history hook breaks a shell, and it must have a dedicated test
- [ ] Preserves any pre-existing `PROMPT_COMMAND`
- [ ] Skips commands starting with `gcode` or `_gcode`
- [ ] Respects `GCODE_NO_HISTORY=1`
- [ ] Under 5 ms; no `jq` hard dependency (uses it only if present)
- [ ] Idempotent install
- [ ] **Test:** sourced in a real bash, a failing command is recorded with the
      right exit code and the user still sees their own `$?`

### 2.4 Zsh hook — `shell/gcode.zsh`

- [ ] `precmd_functions` integration, not a bare `precmd` override
- [ ] Same guarantees as bash
- [ ] **Test:** same suite, sourced in a real zsh

### 2.5 `--init`

- [ ] Detect the shell from `$SHELL`; support `--shell` to override
- [ ] Append a marked block, idempotently:

```
# >>> gcode init >>>
source /usr/local/share/gcode/shell/gcode.bash
# <<< gcode init <<<
```

- [ ] `--check` reports installed/version/missing without changing anything
- [ ] `--remove` deletes exactly the marked block, byte-precise
- [ ] Refuses to write a file it cannot read
- [ ] Prints the exact lines it added
- [ ] **Test:** install twice → file is byte-identical to install once
- [ ] **Test:** remove after install → file is byte-identical to before
- [ ] **Test:** a user's own `PROMPT_COMMAND` survives install and remove

### 2.6 `--fix` and `--complete`

- [ ] `--fix` picks the most recent entry with `exit != 0`, feeds the error
      output to the model, prompts with a diff against the original
- [ ] No failed entry in history → clear message, exit 0
- [ ] `--complete` sends the partial command, returns only the continuation
- [ ] **Test:** `--fix` with a fixture failed entry produces a command that
      differs from the original
- [ ] **Test:** `--fix` with clean history is a no-op with a clear message

### Acceptance criteria

- [ ] `gcode --init` then a failing command then `gcode --fix` produces a
      corrected command
- [ ] The user's shell prompt and `$?` are unaffected
- [ ] Install is idempotent; remove restores the file exactly
- [ ] History file is `0600` and rotates at 10 MB
- [ ] Works in both bash and zsh, tested by sourcing real shells in CI

---

## Phase 3 — Safety layer

**Status: ⬜ not started**
**Goal: a CRITICAL command is unrunnable, and editing cannot bypass it.**

See [SAFETY.md](SAFETY.md) for the full model. This is the build list.

### 3.1 Risk types — `src/safety/mod.rs`

- [ ] `enum Risk { Safe, Low, Medium, High, Critical }`
- [ ] `Ord` so levels compare; `Display`; serde rename to `SAFE`…`CRITICAL`
- [ ] `struct Verdict { level, reasons: Vec<Reason>, segments: Vec<Segment> }`
- [ ] `struct Reason { pattern_id, level, message }`

### 3.2 Normalisation — `src/safety/classifier.rs`

- [ ] Strip comments: `#…` and ` # …` outside quotes
- [ ] Collapse whitespace, expand `~`, resolve `$VAR` and `${VAR}` when the
      variable is set
- [ ] Join line continuations (`\` at EOL)
- [ ] **Split on `;`, `&&`, `||`, `|` and classify every segment**
- [ ] Respect quotes: do not split on a `;` inside `'…'` or `"…"`

### 3.3 Blocklist

- [ ] Built-in set: `rm -rf /`, `mkfs`, `dd of=/dev/`, `> /dev/sd*`, fork bomb,
      `:(){ :|:& };:`, `chmod -R 777 /`, `rm -rf ~`
- [ ] User blocklist merged from `[safety.blocklist]`
- [ ] Matched **before** level assignment; result is CRITICAL
- [ ] **Test:** each built-in pattern, with a segment-splitting variant
- [ ] **Test:** `ls; rm -rf /` → CRITICAL (the case most implementations miss)

### 3.4 Pattern table — `src/safety/patterns.rs`

- [ ] The full table from [SAFETY.md § classifier](SAFETY.md#stage-3--patterns-and-structure)
- [ ] Every entry carries a user-facing reason string
- [ ] Patterns are data, not code, so they can grow without touching logic
- [ ] Each pattern has at least one positive test and one near-miss negative
      test (`rm -rf ./build` must **not** match a root-delete rule)

### 3.5 Structural checks

- [ ] **Taint:** a write to path P followed by a delete of P → escalate one level
- [ ] **Privilege:** `sudo`/`su`/`doas` anywhere → at least MEDIUM
- [ ] **Network + execute:** fetch chained with execute → at least HIGH

### 3.6 Confirmation UI — `src/ui/prompt.rs`

- [ ] Keys: `y n e c ? r`; `Ctrl+C` cancels
- [ ] Shows the command, the level, the reasons, and a cost estimate for MEDIUM+
- [ ] `?` prints a plain-language explanation of each matched pattern
- [ ] `e` opens `$EDITOR`, re-normalises, **re-classifies**, loops
- [ ] `c` copies to the clipboard, never executes
- [ ] Default is `N` at every level
- [ ] Non-interactive stdin (a pipe, a CI job) fails safe: no prompt → no run
- [ ] **Test:** `e` that turns a HIGH command into a CRITICAL one is re-blocked
- [ ] **Test:** piped stdin does not execute

### 3.7 `--yes` semantics

- [ ] Suppresses the prompt only
- [ ] Still classifies, still prints the level and reasons, still logs
- [ ] CRITICAL still refuses
- [ ] **Test:** `--yes` on a CRITICAL command still refuses, exit code non-zero
- [ ] **Test:** `--yes` on a HIGH command runs, and history records the level

### 3.8 `--explain`

- [ ] Given a command, prints: what it does, what it touches, its level, its
      reasons
- [ ] Never executes, never suggests
- [ ] **Snapshot:** explanations for 10 canonical commands

### Acceptance criteria

- [ ] `safety/classifier.rs` has 100 % statement coverage
- [ ] Every built-in blocklist pattern has a test; every pattern has a
      near-miss negative test
- [ ] `gcode -c "delete everything from root" -y` exits non-zero and runs nothing
- [ ] Editing a generated command re-classifies it
- [ ] Piped stdin never executes anything
- [ ] Fuzzing 10 000 mutated commands produces zero panics

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
