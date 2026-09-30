# AGENTS.md — Operating rules for this repository

> This file is the single source of truth for how any agent works in this
> repository. It is loaded automatically at session start.
>
> The repository maintains itself with these rules. You review and decide; the
> agents implement, test, and document.

---

## 0. Absolute rules

These are not preferences. Violating any of them is a failed task.

### 0.1 Never write a secret

- **Never** write an API key, token, password, private key, connection string,
  `.env` value, or credential of any kind into any file — not in code, not in
  docs, not in examples, not in test fixtures, not in comments, not in a commit
  message.
- Test fixtures use **synthetic secret-shaped values**: `sk-` followed by
  obviously fake characters. Never a real or expired credential.
- Examples use `/home/user`, never a real home directory or hostname.
- Anything requiring a key, an account, a domain, or a signing key is marked
  **🔒** in [docs/ROADMAP.md](docs/ROADMAP.md) and left for the human.
- `.env` is gitignored. `.env.example` contains names and empty values only.
- **If you find a secret already committed: stop and report it. Do not just fix
  it.** Rotation is the human's decision, and it comes first.

### 0.2 Stay inside the repository

- Read and write only within the project directory, except when the user
  explicitly names another path.
- Never read `~/.ssh`, `~/.aws`, `~/.config/gh`, browser profiles, keychains, or
  any credential store. Never, even to "check the format".
- Never run a command that reads a secret store, even read-only.

### 0.3 Never commit without permission

- Stage and commit **only** when explicitly asked.
- Never force-push, never rewrite history, never change git config, never
  bypass hooks.
- Before any commit: `git status`, `git diff`, `git log --oneline -10`.
- Never commit a file matching `.gitignore`. If something important is ignored,
  say so rather than forcing it in.

### 0.4 Never write a lie into the docs

- A feature is **not** documented as shipped until it is in `CHANGELOG.md`
  under a released version.
- Anything not implemented is marked `⛔ planned (Phase N)`.
- Acceptance criteria in [docs/ROADMAP.md](docs/ROADMAP.md) are ticked only
  when you have **run the verification**, not when you believe it passes.

### 0.5 No comments unless asked

Match the surrounding style. The instruction to add an authorship header or an
explanatory block is a request; do not add commentary to code otherwise.

---

## 1. What this project is

**gcode** — a local-first, offline CLI that turns natural language into shell
commands, classifies the risk, and asks for confirmation.

Read these before doing anything:

| Document | What it gives you |
|---|---|
| [docs/ROADMAP.md](docs/ROADMAP.md) | **The build contract.** Phases, tasks, acceptance criteria. Start here. |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Module map, data flow, testing seams |
| [docs/SAFETY.md](docs/SAFETY.md) | The security model you must not break |
| [docs/TESTING.md](docs/TESTING.md) | Test strategy and the safety matrix |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Why it is built this way, and what not to change |
| [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) | Branch, commit, and PR rules |
| [plan.md](plan.md) | The long-range vision. Aspiration, not a status report. |

The three truths:

1. `plan.md` is vision.
2. `docs/ROADMAP.md` is the contract.
3. `CHANGELOG.md` is the only record of what shipped.

---

## 2. The working loop

1. **Find the phase.** First phase in [docs/ROADMAP.md](docs/ROADMAP.md) that is
   not ✅. If a phase is 🔒, stop and ask.
2. **Read the phase.** Every task, every acceptance criterion.
3. **Check the ADRs.** Does this task touch a recorded decision? If it would
   reverse one, stop and ask. If it is a natural extension, note it.
4. **Implement** the smallest coherent slice that moves an acceptance criterion.
5. **Test it.** New behaviour gets a test. A bug fix gets a regression test that
   fails without the fix.
6. **Verify**, by running it:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   cargo llvm-cov --fail-under-lines 85    # when coverage tooling is available
   ```

7. **Update the state**, mechanically:
   - Task status in [docs/ROADMAP.md](docs/ROADMAP.md)
   - `.opencode/STATE.md`
   - `CHANGELOG.md` under `Unreleased`
   - Docs, if behaviour or a flag changed
8. **Report.** What changed, what was verified, what is left, what needs a human.

Do not start a phase before the previous one is ✅. Do not mark a criterion met
without having run the command that proves it.

---

## 3. Non-negotiable technical rules

These come from the ADRs. Breaking one requires a **new ADR**, not a clever
implementation.

| Rule | ADR |
|---|---|
| Inference is local. No network except the model downloader. | [0001](docs/adr/0001-local-first-offline-inference.md) |
| Rust, one static binary, no runtime dependency. | [0002](docs/adr/0002-rust-single-binary.md) |
| Decoding is grammar-constrained. Never remove it. | [0003](docs/adr/0003-grammar-constrained-decoding.md) |
| `src/safety/` never imports the model. It is a pure function. | [0004](docs/adr/0004-independent-risk-classifier.md) |
| History is JSONL, `0600`, rotated at 10 MB, no database. | [0005](docs/adr/0005-jsonl-history-store.md) |
| Redaction happens **before** prompt assembly, in one function. | [0006](docs/adr/0006-redact-before-prompt-assembly.md) |
| Shell hooks chain onto existing hooks; they never clobber. `$?` is preserved. | [0007](docs/adr/0007-shell-hook-integration.md) |
| `CRITICAL` is unrunnable. No flag, config key, or env var overrides it. | [0008](docs/adr/0008-critically-unrunnable.md) |
| `--locked` everywhere in CI. `cargo audit` and `cargo deny` gate. | [0009](docs/adr/0009-crates-io-dependencies.md) |
| The model registry is embedded at compile time and validated at build. | [0010](docs/adr/0010-embed-the-model-registry.md) |
| No plugin system before v1.0. | [0011](docs/adr/0011-defer-the-plugin-system.md) |
| `curl | sh` verifies checksums before extracting. | [0012](docs/adr/0012-one-liner-install.md) |
| No native Windows in v1.x. | [0013](docs/adr/0013-deny-windows-native.md) |
| `Q2_K` is refused, not warned about. | [0014](docs/adr/0014-default-model-kitty-bash-llm.md) |
| `cargo-dist` builds releases. `verify-release.sh` gates them. | [0015](docs/adr/0015-cargo-dist-over-distro-packaging.md) |
| Keyless signing. No long-lived signing secret in this project. | [0016](docs/adr/0016-keyless-signing.md) |

### The safety invariants

If your change touches any of these, the test matrix in
[docs/TESTING.md](docs/TESTING.md) must gain a row.

1. Classification is derived from the emitted string, never from the model's
   opinion.
2. Compound commands are split; every segment is classified; the maximum wins.
3. Editing a command re-runs classification.
4. Non-interactive stdin fails closed — no prompt, no execution.
5. `--yes` suppresses the prompt only. Never classification, never the blocklist.
6. `CRITICAL` is unreachable. There is no override, and adding one is a new ADR.
7. Every new pattern gets a positive test **and** a near-miss negative test.

---

## 4. Code conventions

| Rule | Why |
|---|---|
| No `unwrap()`/`expect()` outside tests | A CLI prints an error; it does not panic |
| `thiserror` for library errors, `anyhow` for context | Distinguishes the two layers |
| No `println!` in `src/` except `main.rs` and `ui/` | Keeps `--json` honest |
| `unsafe` requires a `// SAFETY:` comment | There is almost none. Keep it that way |
| Comments explain *why* | The code says *what* |
| Every public item has a doc comment | Generates the man page |
| Paths go through `utils::paths` | One place to change per platform |
| Tests use `FakeEngine`, never a real model | The suite must finish in seconds |

Conventional commits: `feat(safety):`, `fix(shell):`, `docs(models):`,
`test(safety):`. Scopes match the module map.

---

## 5. Documentation conventions

- English. User-facing strings may be bilingual; the tool is used in both
  Arabic and English.
- Direct and imperative. No marketing adjectives.
- Every claim is checkable. Do not document an aspiration as a fact.
- New flag → a row in [docs/USAGE.md](docs/USAGE.md).
- New architectural choice → an ADR in `docs/adr/`, immutable once merged.
  Supersede, never edit.
- `docs/gcode.1` must match the real `--help` output.

---

## 6. What needs a human

Stop and ask. Do not invent a value.

- Anything marked **🔒** in [docs/ROADMAP.md](docs/ROADMAP.md): a key, an
  account, a domain, a signing key, a package repository, a posting account
- A new third-party dependency with a licence or maintenance question
- Reversing or amending an ADR
- Anything that weakens a safety invariant
- Publishing a release, posting publicly, or merging to `main`
- Removing or renaming a published artefact
- Anything touching a user's real home directory or shell config

When you stop, say exactly what you need and what you have already done.

---

## 7. The agent set

| Agent | Role |
|---|---|
| `.opencode/agent/gcode-architect.md` | Plans phases, writes specs and ADRs |
| `.opencode/agent/gcode-rust.md` | Writes Rust in `src/` |
| `.opencode/agent/gcode-safety-auditor.md` | Reviews `src/safety/`; read-only |
| `.opencode/agent/gcode-test-engineer.md` | Writes tests, fixtures, benchmarks |
| `.opencode/agent/gcode-docs.md` | Maintains `docs/`, man page, README |
| `.opencode/agent/gcode-release.md` | Packaging, CI, releases |
| `.opencode/agent/gcode-reviewer.md` | Reviews diffs against the rules; read-only |

Commands: `/status`, `/plan`, `/build`, `/test`, `/review`, `/docs`, `/release`,
`/ship`.

The end-to-end workflow is in
`.opencode/skill/gcode-delivery/SKILL.md`. Load it with the `skill` tool when
asked to "build the project", "do the next phase", or "ship it".

---

## 8. Pre-commit checklist

Run before every commit. Report the actual output; do not assume.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo audit                                        # when available
grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)\s*[:=]\s*["'"'"'][^"'"'"']{8,}' \
  --exclude-dir=target --exclude-dir=.git . || echo "no credential-shaped assignments"
git status --short
```

The last check is a backstop, not a substitute for rule 0.1.

---

## 9. Reporting

When you finish a unit of work, report in this shape:

```
Did:        <what changed, with file paths>
Verified:   <the commands you ran, and their result>
Not done:   <what remains, and why>
Needs you:  <decisions, keys, accounts — or "nothing">
```

Be honest about failures. A test that fails and is reported is worth more than a
green tick that was never run.
