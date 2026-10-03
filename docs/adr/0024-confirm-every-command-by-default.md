# 0024. Confirm every command by default

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Relates to**: [0008](0008-critically-unrunnable.md), [0023](0023-executor-inherits-stdio.md)

## Context

[ADR 0023](0023-executor-inherits-stdio.md) made the executor real. With the
default `always_confirm = MEDIUM`, a `SAFE` or `LOW` command then ran the moment
it was generated, with no question. The classifier is hand-written and can be
wrong about what is harmless, and the model chooses the command. The maintainer
wants a human answer before anything runs.

## Decision

The default `safety.always_confirm` is `SAFE`, so every runnable command is asked
about. `DEFAULT_CONFIRM_AT` and the config default change together, and a test
keeps them equal.

- `--yes` still skips the question and nothing else: classification and the
  `CRITICAL` refusal are unchanged (ADR 0008).
- `safety.always_confirm` remains configurable, `SAFE` to `HIGH`. A user who sets
  `MEDIUM` gets the earlier behaviour on purpose.
- `--complete` prints text for the shell to place on the line and runs nothing, so
  it asks only from `MEDIUM` upward, whatever the threshold.
- Without a terminal nobody can be asked, so a pipe or `--json` refuses a `SAFE`
  command with `ConsentDenied` unless `--yes` or `--dry-run` is given. This is the
  existing fail-closed rule, now covering every level.

This amends one consequence of ADR 0023 ("`SAFE` and `LOW` run without a
prompt"). 0023 is not edited.

## Consequences

- One extra keypress for read-only commands such as `ls`.
- Scripts that relied on a `SAFE` command passing in a pipe must add `--yes`.
