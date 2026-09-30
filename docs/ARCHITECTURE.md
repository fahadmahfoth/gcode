# Architecture

This document describes how gcode is built. It is written to be read *before*
contributing, and to stay true as the code changes. When code and this document
disagree, the code is right and this document is a bug.

---

## Design constraints

Four constraints drive every architectural decision. They are not goals, they are
boundaries.

1. **Single binary, no runtime.** A user must go from nothing to a working
   command in 30 seconds. No runtime, no plugin loader that needs a runtime, no
   JIT, no `python` shebang. Static linking where the platform allows it.
2. **Local-first, offline-capable.** After the model is on disk, the tool must
   work with the network cable pulled. This rules out mandatory cloud inference
   and mandatory online risk scoring.
3. **Safe by default.** Anything destructive must be visible and consent-based.
   The dangerous path must be the *hard* path to reach.
4. **Predictable latency.** Under one second for a typical request on a two-core
   CPU. This rules out model sizes above ~2 GB for the default and rules out
   any pipeline that requires a second model call in the common case.

---

## High-level flow

```
 natural language
        │
        ▼
 ┌──────────────┐   clap: parse flags, pick mode
 │  CLI LAYER   │   src/cli.rs
 └──────┬───────┘
        │
        ▼
 ┌──────────────┐   history.jsonl (tail N) + cwd + git + os/shell
 │  CONTEXT     │   src/context/{history,env,prompt}.rs
 │  BUILDER     │   redaction happens HERE, before anything else sees it
 └──────┬───────┘
        │
        ▼
 ┌──────────────┐   GGUF → RAM once, cached across the process lifetime
 │  INFERENCE   │   src/inference/{engine,grammar}.rs
 │  ENGINE      │   grammar-constrained decoding via llguidance
 └──────┬───────┘   returns: one shell command, never prose
        │
        ▼
 ┌──────────────┐   pattern match, then structural analysis
 │  SAFETY      │   src/safety/{patterns,classifier}.rs
 │  CLASSIFIER  │   SAFE | LOW | MEDIUM | HIGH | CRITICAL
 └──────┬───────┘   hard blocklist evaluated FIRST, before level assignment
        │
        ▼
 ┌──────────────┐   y / n / e / c / ? / r
 │ CONFIRMATION │   src/ui/prompt.rs
 └──────┬───────┘   `--edit` re-enters the classifier (no laundering)
        │
        ▼
 ┌──────────────┐   sh -c, inherited stdio, captured exit code
 │   EXECUTOR   │   src/exec/runner.rs
 └──────┬───────┘
        │
        ▼
 ┌──────────────┐   append one JSON line, mode 0600
 │   HISTORY    │   src/context/history.rs
 └──────────────┘
```

The critical property: **redaction happens before the prompt is assembled**, and
**classification happens before the user is ever shown an actionable button**.
There is no code path where an unclassified command reaches the confirmation
prompt.

---

## Module map

| Path | Responsibility | Depends on |
|---|---|---|
| `src/main.rs` | Wiring only. No business logic. | everything |
| `src/cli.rs` | Flag definitions, mode selection, validation | `clap` |
| `src/config.rs` | Layered config load + defaults + validation | `toml`, `serde` |
| `src/context/history.rs` | Read/append `history.jsonl`, rotation, redaction | `serde_json` |
| `src/context/env.rs` | cwd, OS, shell, git branch/status | `std::process` |
| `src/context/prompt.rs` | Assemble the prompt; the one place redaction is called | above two |
| `src/inference/engine.rs` | Model load, tokenize, generate, unload | `llama-cpp-rs` |
| `src/inference/grammar.rs` | Bash grammar for constrained decoding | `llguidance` |
| `src/model/download.rs` | Fetch, checksum, resume, mirror fallback | `reqwest`, `sha2` |
| `src/model/registry.rs` | Known models, names, URLs, hashes | `toml` |
| `src/safety/patterns.rs` | Regex table: pattern → level → reason | `regex` |
| `src/safety/classifier.rs` | Blocklist, then patterns, then structural checks | above |
| `src/exec/runner.rs` | Spawn, stream, capture, timeout | `std::process` |
| `src/utils/paths.rs` | XDG dirs, model cache, config discovery | `dirs` |
| `src/ui/` | Colour, prompts, rendering, `--json` output | `dialoguer`, `colored` |

Dependency rule: **`ui` may depend on anything; nothing may depend on `ui`.**
This keeps the core testable headlessly and keeps `--json` honest — it serialises
data, it does not scrape the human-facing renderer.

---

## The data that crosses a trust boundary

Three things cross from the user's world into the model's world:
`cwd`, `os`/`shell`, and `history` entries including output tails. All three are
attacker-influenced in a hostile-directory scenario — a repository you just
cloned can contain a `.bashrc`-shaped filename or an oddly named directory.

Defences:

- **Redaction before prompt assembly.** `context::prompt::redact()` runs on
  history output tails and on env-derived strings. Patterns: `*_TOKEN`, `*_SECRET`,
  `*_KEY`, `*_PASSWORD`, `AWS_*`, `GITHUB_TOKEN`, `Authorization:`, `Bearer `,
  long base64/hex runs, and PEM headers.
- **History output truncation.** Only the last 2 KB of output is kept, and only
  the last 15 entries by default. A 2 GB `cat` does not become a prompt.
- **Prompt delimiters.** History is wrapped in explicit untrusted-data markers
  with a system-level instruction that content inside is data, not instructions.
  This is a mitigation, not a guarantee; the grammar and the classifier are the
  real defence, because they constrain what the model can *emit*, not merely what
  it can be told.
- **The classifier never trusts the model.** It re-derives risk from the emitted
  string. A model persuaded by a hostile history entry cannot emit something the
  classifier has not seen.

The last point is the load-bearing one. Prompt injection is bounded, not
prevented, because the output space is grammar-constrained and every output
passes an independent structural check.

---

## Concurrency model

Single-threaded by design, with two exceptions.

```
main thread:  parse → context → classify → prompt → execute
model:        loaded once, inference is synchronous and blocking
hook side:    the shell hook is a separate short-lived process (~5 ms)
```

Rationale: a CLI that lives for under two seconds does not benefit from
parallelism, and every thread is a place for a race with a model that holds
`mmap`'d memory. Where parallelism exists:

- The **model download** runs in a background thread (or detached process) so the
  user gets their prompt back immediately. It takes a filesystem lock before
  writing, so two concurrent `gcode` invocations cannot corrupt the cache.
- The **shell hook** is spawned by the shell itself and never overlaps with
  inference in a meaningful way.

If a daemon mode is ever added (ROADMAP, post-v1.0), it will be a separate
process with a Unix socket, not a thread inside the CLI.

---

## Failure handling

Every failure mode has a defined, tested behaviour. The rule: **fail closed**.

| Failure | Behaviour |
|---|---|
| Model missing | Prompt to download; `--no-model` exits with guidance |
| Model checksum mismatch | Delete the file, refuse to load, say why |
| Inference timeout (30 s) | Abort, never execute, log the partial output |
| Malformed history line | Skip that line, keep reading, warn once |
| History file unreadable | Continue with no history, warn on stderr |
| Unknown model name | List valid names, exit 2 |
| Config invalid | Print the parse error and the file path, use defaults |
| Editor returns garbage | Re-classify; if it no longer parses, refuse |
| Network down during model fetch | Retry with backoff, then try the mirror, then explain |
| Grammar-constrained decode fails | Fall back to unconstrained, then to blocklist-only output |

The last row is the important one. If the model cannot produce a valid command,
gcode prints nothing executable rather than printing a guess.

---

## Extension points

Deliberately few. Each one exists because a real use case demanded it, and each
one is a maintenance cost.

| Extension | Mechanism | Stability |
|---|---|---|
| Custom models | `--model` + registry TOML | Stable |
| Custom risk patterns | `[safety.blocklist]` in config | Stable |
| Shell support | A file in `shell/` + an entry in `detect_shell()` | Additive |
| WASM plugins | ⛔ planned (Phase 10) | Not designed yet |

The last row is a trap this project has not yet walked. Do not build the plugin
system before there are ten real plugins.

---

## Testing seams

Every module is unit-testable without a model, a network, or a terminal:

- `cli` — pure parse tests, no I/O
- `config` — layered merge over in-memory fixtures
- `context` — `tempfile`-backed history fixtures
- `safety` — pure functions, no I/O, 100% coverage target
- `prompt` — `insta` snapshots; the prompt is a string, so a snapshot is the
  whole test
- `model` — `mockito` for HTTP, `tempfile` for the cache
- `inference` — a fake engine behind a trait, so tests never load 400 MB
- `exec` — runs real commands in a temp cwd, which is safe because the commands
  are ones the test wrote

The `InferenceEngine` trait exists specifically so the whole pipeline can be
tested end-to-end with a deterministic fake. That trait is load-bearing for the
test strategy described in [TESTING.md](TESTING.md).

---

## Related documents

- [ROADMAP.md](ROADMAP.md) — what gets built, in what order
- [SAFETY.md](SAFETY.md) — the security model in depth
- [TESTING.md](TESTING.md) — test strategy and coverage targets
- [DECISIONS.md](DECISIONS.md) — why each of these choices was made
- [`adr/`](adr/) — the immutable decision records behind them
