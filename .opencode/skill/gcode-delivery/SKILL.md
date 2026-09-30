---
name: gcode-delivery
description: End-to-end workflow for building gcode — find the current phase, slice the work, implement with tests, verify, audit safety, update state, and report. Load this when asked to "build the project", "do the next phase", "ship it", "continue gcode", or "what should we work on". Triggers on gcode, roadmap, phase, slice, ship, release-prep.
license: MIT
metadata:
  project: gcode
  author: OpenCode
---

# gcode delivery workflow

The procedure for taking **gcode** from its current state to a releasable one.
Follow it in order. Do not skip the verification steps, and do not skip the
report.

## The setup

gcode is a local-first, offline CLI that turns natural language into shell
commands, classifies the risk, and asks for confirmation. It is Rust, one static
binary, with a local GGUF model and no network after the model download.

Read before working:

| Document | What it gives you |
|---|---|
| `AGENTS.md` | The operating rules. **Absolute.** |
| `docs/ROADMAP.md` | The build contract: phases, tasks, acceptance criteria |
| `docs/ARCHITECTURE.md` | Module map, dependency direction, testing seams |
| `docs/SAFETY.md` | The security model you must not break |
| `docs/TESTING.md` | Test strategy and the safety matrix |
| `docs/DECISIONS.md` + `docs/adr/` | Why it is built this way, and what not to change |

The three truths:

1. `plan.md` is vision.
2. `docs/ROADMAP.md` is the contract.
3. `CHANGELOG.md` is the only record of what shipped.

## The seven safety invariants

Non-negotiable. A change that weakens one is a blocker, and adding an override is
a new ADR conversation, not a code change.

1. Classification is derived from the emitted string, never from the model's
   opinion.
2. Compound commands are split; every segment is classified; the maximum wins.
3. Editing a command re-runs classification.
4. Non-interactive stdin fails closed — no prompt, no execution.
5. `--yes` suppresses the prompt only. Never classification, never the blocklist.
6. `CRITICAL` is unreachable. No flag, config key, or env var overrides it.
7. Every new pattern gets a positive test **and** a near-miss negative test.

## The loop

### 1. Find the work

Read `.opencode/STATE.md` and `docs/ROADMAP.md`. The target is the first phase
that is not ✅. Never start a phase before the previous one is done.

If a phase is 🔒 — a key, an account, a domain, a signing key, a posting
account — **stop and ask.** Do not invent a value, and do not leave a plausible
placeholder in code.

### 2. Check the ADRs

Read every ADR the work touches. Ask: would this reverse or quietly weaken a
recorded decision?

- **Yes** → stop. Write up the conflict and let the human decide. A new ADR is
  the mechanism for reversing a decision, not a clever implementation.
- **Natural extension** → note it, and write the ADR if it meets all three tests
  in `docs/DECISIONS.md`.

### 3. Slice it

**The smallest coherent slice that moves an acceptance criterion.** One task, or
one tightly-coupled group. Not a whole phase, and not a refactor along the way.

A slice that touches eleven files is usually eleven slices.

State the slice as: the criterion it moves, the files it touches, and the test
that proves it.

### 4. Implement

Code, tests, and docs for the slice, together.

- Follow `AGENTS.md` § 4. No `unwrap()` outside tests, `thiserror` for library
  errors, no `println!` outside `main.rs` and `ui/`.
- Every public item gets a doc comment.
- Every path goes through `utils::paths`.
- Comments explain *why*. The code says *what*.
- `src/safety/` stays pure: no model, no I/O, no randomness, no clock.
- No comments unless the surrounding code has them.

Tests:

- New behaviour gets a test
- A bug fix gets a regression test that **fails without the fix**
- Every safety pattern gets a positive test and a near-miss negative test
- No test loads a real model — use `FakeEngine`
- Shell hooks are tested by sourcing them in real shells in containers

### 5. Verify — by running it

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo llvm-cov --fail-under-lines 85    # when available
cargo audit                             # when available
```

**Never report a green tick you did not observe.** A failing test reported
honestly is worth more than a fake pass. If `cargo llvm-cov` is unavailable, say
so — do not estimate a number.

Secret scan, because this repository is public and publishes a `curl | sh`
installer:

```bash
grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"']{8,}' \
  --exclude-dir=target --exclude-dir=.git . || echo "clean"
```

If this fires on a real credential, **stop**. Report it. Do not fix it silently.
Rotation is the human's decision and it comes first.

### 6. Audit, when safety is involved

If `src/safety/` or the confirmation flow is in the diff, run the
`gcode-safety-auditor` agent. Any `CRITICAL` or `HIGH` finding is a blocker.

Also run the `gcode-reviewer` agent on the diff before committing.

### 7. Update the state — in the same change

- `docs/ROADMAP.md` — task checkboxes and acceptance criteria, **only for what
  was verified by running the command**
- `.opencode/STATE.md` — `phase`, `first_incomplete_task`, `next: true`,
  `last_verified`, `last_command`
- `CHANGELOG.md` — under `Unreleased`, user-visible changes only
- `docs/USAGE.md` — a row for every new flag
- `docs/gcode.1` — must match the real `--help` output
- `.opencode/STATE.md` — exactly one phase has `next: true`

### 8. Commit — only when asked

```bash
git status
git diff
git log --oneline -10
```

Stage only intended files. Never force-push, never rewrite history, never change
git config, never bypass hooks, never commit a file matching `.gitignore`.

Conventional commit, scope matching the module map:
`feat(safety):`, `fix(shell):`, `docs(models):`, `test(safety):`.

### 9. Report

```
Did:        <what changed, with file paths>
Verified:   <the commands you ran, and their result>
Not done:   <what remains, and why>
Needs you:  <decisions, keys, accounts — or "nothing">
```

Be honest about failures. Report the state at every 🔒, not just at the end.

### 10. Repeat

Back to step 1 for the next slice, until the target is met or you hit a 🔒.

## Never

- Write a credential of any kind. Fixtures use `sk-` plus obviously fake
  characters.
- Read `~/.ssh`, `~/.aws`, `~/.config/gh`, a keychain, or any credential store.
- Touch anything outside the repository unless the user named the path.
- Mark a criterion met without having run the verification.
- Weaken a safety invariant, or add an override for `CRITICAL`.
- Publish a release, tag, push to `main`, or post publicly.
- Document a feature as shipped before it is in a released `CHANGELOG.md`.

## Reference — the agent set

| Agent | Use for |
|---|---|
| `gcode-architect` | Phase specs, ADRs, planning |
| `gcode-rust` | Implementation in `src/` |
| `gcode-safety-auditor` | Read-only audit of `src/safety/` |
| `gcode-test-engineer` | Tests, fixtures, fuzz targets, benchmarks |
| `gcode-docs` | `docs/`, man page, README, ADRs, changelog |
| `gcode-release` | Packaging, CI, releases |
| `gcode-reviewer` | Read-only diff review |

Commands: `/status`, `/plan`, `/build`, `/test`, `/review`, `/docs`, `/release`,
`/ship`.
