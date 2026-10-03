# 0023. The executor runs `sh -c` with inherited stdio, captures nothing, and has no timeout

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Relates to**: [0004](0004-independent-risk-classifier.md), [0008](0008-critically-unrunnable.md), [0022](0022-no-signal-handler.md)

## Context

The architecture names a final stage, `src/exec/runner.rs`, described as "spawn,
stream, capture, timeout". Until now the run loop stopped at the verdict and
reported `executed: false`. Building the stage means choosing what it does with
the child's streams, how long it waits, and what status gcode reports.

## Decision

1. **`sh -c <command>`**, one string, so pipes, lists, and redirections mean what
   the classifier assumed when it split and classified the same string.
2. **Inherited stdin, stdout, and stderr.** The user sees the command's own output
   live, and an interactive command (`less`, `ssh`, a prompt) works.
3. **No output capture.** The history record has an empty `out`. Capturing needs a
   pseudo-terminal, a new dependency or `unsafe`, and changes what an interactive
   command can do. It is a separate decision and needs its own ADR.
4. **No timeout.** A command the user approved is theirs to interrupt; Ctrl+C
   reaches the child and gcode together (ADR 0022).
5. **gcode exits with the command's status** (`128 + signal` when killed). A
   command that fails is a status, not an error of gcode's.
6. **The executor is optional and injected.** `run_configured` takes
   `Option<Runner>`; `main` builds one only for an interactive terminal without
   `--json`. With none, nothing in the run loop can start a process.
7. **Only generate and fix execute.** `--complete` produces text for the shell to
   place on the line, and `--dry-run`, `-n`, and `--explain` never run anything.

The gate re-checks that the final command is runnable immediately before the call,
so reordering the code above it cannot deliver a refused command to the executor
(ADR 0008).

## Consequences

- A `SAFE` or `LOW` command now runs without a prompt when a terminal is attached,
  as the confirmation threshold already implied.
- A piped invocation, `--json`, and every test still run nothing.
- gcode's own exit codes (1 refusal, 2 usage) overlap with a command's. A script
  that needs to tell them apart should use `--json --dry-run`.
- The shell hook records the `gcode` invocation; gcode records the command it ran.
  They are different lines.
