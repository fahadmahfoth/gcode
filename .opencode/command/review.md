---
description: Audit the safety layer or review a diff against the project rules
agent: gcode-reviewer
---

Run a read-only safety audit or a rules review. Nothing is edited.

## Arguments

`$ARGUMENTS` selects the mode:

- `safety` — audit `src/safety/` for holes, false positives, and bypass paths
- `diff` — review the working diff against `AGENTS.md`
- `branch` — review the branch against `master`
- `adr` — check whether a change reverses a recorded decision
- empty — do `safety` if `src/safety/` has uncommitted changes, otherwise `diff`

## Do this

### For `safety`

1. Read `docs/SAFETY.md` and the safety matrix in `docs/TESTING.md`.
2. Check all seven invariants from `AGENTS.md` § 3. All seven, or say which one
   you could not check and why.
3. Trace every path to `exec/runner.rs` backwards. For each one, ask where the
   string was classified and whether that call can be skipped.
4. Attack the normaliser. Try: `ls; rm -rf /`, `rm --recursive --force /`,
   `rm -fr /`, `rm -rf "${HOME}"`, `ls $(rm -rf /)`, backticks, line
   continuations, a redirect that turns a read into a write, homoglyphs.
5. Attack the blocklist. Can each entry be evaded by reordering flags or by
   changing quoting? An entry that can be evaded is not an entry.
6. Check every pattern for a positive **and** a near-miss negative test. A
   pattern without a negative test is an untested pattern.
7. Check false positives too. Over-broad patterns train users to approve
   reflexively, which destroys the gate as surely as a false negative.

### For `diff`, `branch`, `adr`

1. `git status`, `git diff`, `git log --oneline -10`
2. Read `AGENTS.md` and review in this order: **secrets → safety invariants →
   ADR conflicts → test adequacy → error handling → conventions → docs →
   honesty.**
3. For every doc claim, ask whether it is true. Is a criterion ticked with no
   verification behind it? Does a benchmark number exist that nobody measured?
4. For every snapshot change, ask whether a human looked at it.

## Report

```
Mode:        safety | diff | branch | adr
Scope:       <files or phases examined>

Blocker   <file:line> — issue / consequence / fix / verify
Major     <file:line> — ...
Minor     <file:line> — ...

Invariants:   <all seven checked, or which and why>
Patterns:     <N of M audited>
False pos:    <N found>
Bypasses:     <N found>

Verdict:      APPROVE | APPROVE WITH COMMENTS | REQUEST CHANGES
Not verified: <what you did not run — "I did not run cargo test" is required
               if true>
```

Findings lead with the worst thing. A review that buries one blocker in thirty
nits has failed.

Rules for you:

- Do not soften a finding to be agreeable. A security finding softened is a
  vulnerability shipped.
- Do not report a finding you cannot state a consequence for.
- Do not approve what you did not verify.
- Edit nothing. This command reviews.
