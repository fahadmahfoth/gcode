---
description: Run the whole delivery workflow end to end, from the current phase to a release
agent: build
---

Take gcode from wherever it is to a releasable state. This is the long one.
Work in slices, verify each, and report as you go.

## Arguments

`$ARGUMENTS` may be a phase (`Phase 1`), a target (`1.0.0`), or empty, meaning
"everything remaining in the current phase".

## Do this

Load the workflow skill first — it has the full slice-by-slice procedure:

> Use the `skill` tool to load `gcode-delivery`.

Then follow it. In summary:

### 1. Assess

Read `AGENTS.md`, `.opencode/STATE.md`, and `docs/ROADMAP.md`. Report where the
build stands and what is blocking. If anything 🔒 is in the way, stop and list
it.

### 2. Plan the slice

The **smallest coherent slice that moves an acceptance criterion.** One task, or
one tightly-coupled group. Not a whole phase.

Confirm the work does not reverse an ADR. If it would, stop: that is a decision
for the human, not a coding task.

### 3. Implement

Code, tests, and docs for the slice together. Follow `AGENTS.md` § 4.

- New behaviour gets a test
- A bug fix gets a regression test that fails without the fix
- Every safety pattern gets a positive **and** a near-miss negative test
- No test loads a real model

### 4. Verify — by running it

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo llvm-cov --fail-under-lines 85    # when available
```

If something fails, fix it or report the failure. Never report a green tick you
did not observe.

### 5. Safety audit, when `src/safety/` is involved

Run the `gcode-safety-auditor` agent over the change. Any `CRITICAL` or `HIGH`
finding is a blocker. Do not proceed past one.

### 6. Update the state

- `docs/ROADMAP.md` — checkboxes and criteria, only for what you verified
- `.opencode/STATE.md` — the pointer, `last_verified`, `last_command`
- `CHANGELOG.md` — under `Unreleased`
- `docs/USAGE.md`, `docs/gcode.1` — if a flag changed

### 7. Commit

Only when the human explicitly asked for commits. When asked:

```bash
git status
git diff
git log --oneline -10
```

Stage only intended files. Never force-push, never rewrite history, never
bypass hooks, never commit a file matching `.gitignore`.

Conventional commit: `feat(safety):`, `fix(shell):`, `docs(models):`,
`test(safety):`.

### 8. Repeat

Go back to step 2 for the next slice, until the target is met or you hit a 🔒.

## Never

- Write a credential, or touch `~/.ssh`, `~/.aws`, or any credential store
- Mark an acceptance criterion met without having run the verification
- Weaken a safety invariant
- Add an override for `CRITICAL`
- Publish a release, tag, or post publicly
- Continue past a 🔒 without asking

## Report — at the end, and at every 🔒

```
Did:        <slices completed, with file paths>
Verified:   <every command run, with real output>
Not done:   <what remains, and why>
Needs you:  <decisions, keys, accounts — or "nothing">
```

If you stop early, say what you completed and what is left. A partial slice
reported honestly is a good outcome. A silently truncated one is not.
