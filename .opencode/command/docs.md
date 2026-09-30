---
description: Update the documentation, man page, changelog, and roadmap
agent: gcode-docs
---

Keep gcode's documentation true. This command edits docs; it does not write
code.

## Arguments

`$ARGUMENTS` may name what changed, a document, or a task. If empty, reconcile
all documentation with the current state of the code.

## Do this

1. Read `AGENTS.md` § 5. Documentation conventions.
2. Read `CHANGELOG.md` and `docs/ROADMAP.md`.
3. Check what actually changed:

   ```bash
   git status --short
   git diff --stat
   git log --oneline -10
   ```

4. **Run the binary** before documenting any behaviour:

   ```bash
   cargo run -- --help
   cargo run -- --version
   cargo run -- --json -n -c "list all files here" 2>&1 | head -20
   ```

## Update

- `docs/USAGE.md` — **a row for every flag in `--help`.** Diff the two.
- `docs/gcode.1` — the man page must match the real `--help` output
- `README.md` — only what is true today. This project is pre-1.0; the status
  table and the "planned" markers must be accurate
- `CHANGELOG.md` — under `Unreleased`, only user-visible changes
- `docs/ROADMAP.md` — task checkboxes and acceptance criteria, **only for what
  was actually verified**
- `.opencode/STATE.md` — `last_verified`, `last_command`, phase pointer
- `docs/adr/` — a new ADR, if this change met all three tests in
  `docs/DECISIONS.md`
- `docs/TROUBLESHOOTING.md` — if a new failure mode is now reachable

## The two rules

### Never document a lie

A feature is not shipped until it is in `CHANGELOG.md` under a **released**
version. Anything unimplemented is marked `⛔ planned (Phase N)`. A criterion is
ticked only when the verification has actually run.

Do not write an example output you invented and present it as real. If the
binary does not build yet, say the command is targeted behaviour and mark it.

### Never write a secret

No keys, tokens, passwords, private keys, or connection strings — not in an
example, not in a fixture, not in a comment. Examples use `/home/user`. Signing
key *handling* is documented; key *material* is not.

## Style

- English docs. Arabic user-facing strings are fine in code.
- Direct and imperative. No marketing adjectives.
- Every claim checkable. No aspirational language in a factual document.
- Never remove an attribution note another author added. Adding one is optional.

## Verify before reporting

- Every flag in `--help` has a `docs/USAGE.md` row
- Every internal link resolves
- Every file path referenced exists
- Every `⛔` marker still matches reality
- The secret scan is clean:

  ```bash
  grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"']{8,}' \
    --exclude-dir=target --exclude-dir=.git . || echo "clean"
  ```

## Report

```
Did:        <documents changed>
Verified:   <flags checked, links checked, examples run — and the result>
Not done:   <what remains, and why>
Needs you:  <or "nothing">
```

If a document would need a real secret, hostname, or account to be complete,
stop and say so. Do not invent a value.
