---
description: Implement the next slice of the roadmap, with tests
agent: gcode-rust
---

Implement gcode work. Code plus tests, in one slice.

## Arguments

`$ARGUMENTS` may name a task (`1.3`), a phase (`Phase 1`), or a feature. If
empty, take the `first_incomplete_task` from `.opencode/STATE.md`.

## Do this

1. Read `AGENTS.md`. These rules are absolute.
2. Read the target phase in `docs/ROADMAP.md`. Every task, every acceptance
   criterion.
3. Read `docs/ARCHITECTURE.md` for the module map and dependency direction.
4. Read the relevant ADRs. If the work would reverse one, **stop and report it
   before writing code.**
5. If anything is 🔒, stop. Report exactly what is needed.
6. If `src/safety/` is involved, read `docs/SAFETY.md` and the safety matrix in
   `docs/TESTING.md` first.

## Implement

**The smallest coherent slice that moves an acceptance criterion.** Not a whole
phase, not a refactor along the way.

- Follow `AGENTS.md` § 4 exactly. No `unwrap()` outside tests, `thiserror` for
  library errors, no `println!` outside `main.rs` and `ui/`.
- Every public item gets a doc comment.
- **New behaviour gets a test. A bug fix gets a regression test that fails
  without the fix.** Write the failing test first if you can.
- **Every safety pattern gets a positive test and a near-miss negative test.**
  `rm -rf ./build` must not match a root-delete rule.
- No test loads a real model. Use `FakeEngine`.
- No comments unless the surrounding code has them and they explain *why*.
- No secrets, ever. Fixtures use `sk-` plus obviously fake characters.

## Verify — by running it

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

If any fails, fix it or report the failure. **Never report a green tick you did
not observe.** A failing test you report honestly is worth more than a fake pass.

## Update the state — in the same change

- Task checkboxes in `docs/ROADMAP.md`, only for what you verified
- Phase acceptance criteria, only for what you verified
- `.opencode/STATE.md`: `phase`, `first_incomplete_task`, `next: true`,
  `last_verified`, `last_command`
- `CHANGELOG.md` under `Unreleased`
- `docs/USAGE.md` if a flag changed; `docs/gcode.1` if it affects the man page

## Never

- Commit, unless explicitly asked. When asked: `git status`, `git diff`,
  `git log --oneline -10` first, and stage only intended files.
- Weaken a safety invariant.
- Add an override for `CRITICAL`.
- Mark a criterion met you have not verified.
- Write a credential, or touch `~/.ssh`, `~/.aws`, or any credential store.
- Touch anything outside the repository.

## Report

```
Did:        <files changed, with paths>
Verified:   <the commands you ran, and their actual output>
Not done:   <what remains, and why>
Needs you:  <decisions, keys, accounts — or "nothing">
```

Be honest about failures. A test that fails and is reported is worth more than a
green tick that was never run.
