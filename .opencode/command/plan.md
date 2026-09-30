---
description: Plan the next phase or a specific task, and write the spec
agent: gcode-architect
---

Plan gcode development. Produce a specification; do not write implementation code.

## Arguments

`$ARGUMENTS` may name a phase, a task, or a feature. If empty, plan the first
incomplete phase in `docs/ROADMAP.md`.

## Do this

1. Read `AGENTS.md`, then `docs/ROADMAP.md`, then `docs/ARCHITECTURE.md`.
2. Find the target phase — the first one that is not ✅. If a phase is 🔒, stop
   and report it.
3. Read every ADR in `docs/adr/` that the work touches. If the work would
   reverse one, **stop and say so before planning anything else.** That is an
   ADR conversation with the human, not a coding task.
4. Consult `.opencode/STATE.md` for the current pointer.
5. Read the existing code, if any. The code is the source of truth. If it
   contradicts a document, say so.

## Produce

For each task in the plan:

- **Task number and name**, matching `docs/ROADMAP.md`
- **Files** created and modified, with the shape of each: struct names, enum
  variants, function signatures, trait definitions
- **The `InferenceEngine` trait shape** if inference is involved. Preserve it
  exactly — the whole test strategy depends on it
- **Acceptance criteria** that are runnable commands. A criterion you cannot
  run is not a criterion
- **Tests** that will be written: positive, and near-miss negative for anything
  in `src/safety/`
- **Dependencies** — what must exist first, and what this forecloses
- **Risks** — the three ways this goes wrong

Then update, if the plan changes the contract:

- Task checkboxes in `docs/ROADMAP.md`
- `.opencode/STATE.md`

## Rules

- **Never weaken a safety invariant.** The seven invariants in `AGENTS.md` § 3
  are not negotiable. If a task seems to need one weakened, say that first and
  stop.
- **Never invent a value for anything 🔒.** Mark it and list it under
  `Needs you`.
- **Never write a secret.** Rule 0.1. Examples use `/home/user`.
- **Do not tick a criterion you have not verified.** If you have not run it, it
  stays unticked.
- **Prefer the smallest coherent slice.** A task that touches eleven files is
  usually eleven tasks.

## Report

```
Planned:  <phase and tasks, with file paths>
Blocked:  <ADRs touched, or "none">
Needs you: <decisions, keys, accounts — or "nothing">
```

Then, if the plan is unambiguous enough to execute, say so in one line. Do not
execute it here — `/build` does that.
