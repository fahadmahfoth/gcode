---
description: Show where the build stands and what to do next
agent: build
---

Report the current state of the gcode build. Do not modify anything.

## Do this

1. Read `.opencode/STATE.md`.
2. Read the phase table in `docs/ROADMAP.md`.
3. Check `CHANGELOG.md` for anything under `Unreleased`.
4. If `src/` exists, run a quick reality check:

   ```bash
   ls src/ 2>/dev/null && cargo test --quiet 2>&1 | tail -20
   ```

5. Check for credential-shaped strings, because this repository is public:

   ```bash
   grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"']{8,}' \
     --exclude-dir=target --exclude-dir=.git . || echo "clean"
   ```

6. Check for uncommitted work: `git status --short`.

## Report this shape

```
Project:     gcode — local-first, offline NL → shell command generator
Phase:       1 — Core inference pipeline   [status]
Next task:   1.1 CLI layer — src/cli.rs
Code:        [written / not written]
Tests:       [count, or "none yet"]
Coverage:    [measured number, or "not measured"]
Last verify: [command and its real result]
Needs you:   [🔒 items, or "nothing"]

Blocked on:  [nothing, or the specific blocker]
```

Then, if anything is out of sync — STATE.md disagrees with ROADMAP.md, a ticked
criterion has no verification behind it, or a secret scan fired — say so
explicitly and name the file. Do not silently reconcile them.

Do not start work. Do not edit files. Report only.
