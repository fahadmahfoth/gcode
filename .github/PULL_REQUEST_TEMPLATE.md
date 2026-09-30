## What this changes

<!-- One or two sentences. If it reverses a recorded decision, say so and link
     the ADR. -->

## Phase

<!-- Which task or phase in docs/ROADMAP.md does this move? -->

- Phase: ____
- Task: ____

## Type

- [ ] `feat` — new capability
- [ ] `fix` — bug fix
- [ ] `perf` — measurable performance change
- [ ] `test` — tests only
- [ ] `docs` — documentation only
- [ ] `refactor` — no behaviour change
- [ ] `build` / `ci` — dependencies, packaging, workflows

## Verification — required

Paste the actual output. A green tick nobody ran is worse than a failing test
that got reported.

```
$ cargo fmt --all -- --check

$ cargo clippy --all-targets --all-features -- -D warnings

$ cargo test
```

## Safety — required if `src/safety/` is touched

- [ ] The change is covered in the safety test matrix in `docs/TESTING.md`
- [ ] Every new pattern has a **positive** test and a **near-miss negative** test
- [ ] Compound commands are still split, and the maximum level still wins
- [ ] `--edit` still re-runs classification
- [ ] Non-interactive stdin still fails closed
- [ ] `--yes` still suppresses the prompt only
- [ ] There is no override path for `CRITICAL`
- [ ] The `gcode-safety-auditor` agent was run, and there are no HIGH findings

## Tests

- [ ] New behaviour has a test
- [ ] A bug fix has a regression test that fails without the fix
- [ ] No test loads a real model
- [ ] Coverage has not decreased

## Docs

- [ ] `CHANGELOG.md` has an entry under `Unreleased`
- [ ] `docs/USAGE.md` has a row for every new flag
- [ ] `docs/gcode.1` matches the real `--help` output
- [ ] An ADR exists, if this changes an architectural choice
- [ ] No document claims a feature that is not in this build

## Secrets — required, this repository is public

- [ ] No credential, token, key, or connection string anywhere in the diff
- [ ] Test fixtures use synthetic secret-shaped values (`sk-` + fake characters)
- [ ] Examples use `/home/user`, not a real home directory or hostname
- [ ] No signing key, and no long-lived secret
- [ ] No model file, and no history file

## ADRs

- [ ] This does not reverse a recorded decision, or a new ADR is included
- [ ] Any ADR is in `docs/adr/`, numbered sequentially, never renumbered

## Notes for reviewers

<!--
Anything a reviewer should know: a tricky part, a deliberate trade-off, something
you are unsure about. Flagging your own uncertainty is worth more than a confident
change that turns out to be wrong.
-->
