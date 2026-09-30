# 0007. Shell hooks over a wrapper script

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

`gcode --fix` and context-aware generation need the last command, its exit code,
its working directory, and a tail of its output. The shell knows all of this at
the moment the command finishes. Everything else has to guess.

Two ways to get it:

1. **Shell hook.** gcode installs a `PROMPT_COMMAND` (bash) or `precmd`
   function (zsh) that runs before each prompt and writes one history line.
2. **Wrapper script.** The user's `alias` or `PATH` shim runs every command
   through gcode, which records the result.

The wrapper is dramatically simpler to build and to reason about. It is also the
wrong design, and the reason is worth writing down.

## Decision

Install a shell hook, integrated additively with whatever the user already has.

1. `shell/gcode.bash` defines `_gcode_capture` and chains it onto
   `PROMPT_COMMAND` **without overwriting it**.
2. `shell/gcode.zsh` appends to `precmd_functions`, which is additive by design
   in zsh.
3. Install is idempotent, delimited by `# >>> gcode init >>>` markers, and
   reversible byte-precisely with `--init --remove`.
4. gcode never writes to `~/.bashrc` or `~/.zshrc` outside those markers.
5. The hook preserves the user's `$?` as its first statement. This is the single
   most important line in the file.
6. `GCODE_NO_HISTORY=1` disables recording entirely.
7. Under 5 ms per prompt, and it skips gcode's own commands.

## Alternatives considered

**Wrapper script via `alias`.** Cannot capture output without breaking
interactive programs. `alias ls='gcode exec ls'` means `ls` stops being `ls`:
no completion, no colour handling, no `ls | less`, and a recursive mess once the
user has twenty aliases. Rejected: it damages the shell it is meant to observe.
The "wrap every command" variant is worse — it intercepts `cd`, breaks pipelines,
and adds latency to everything for the benefit of one feature.

**Ask the user to run `gcode --record` manually.** Zero intrusion. Rejected: it
would be forgotten, so history would be empty, so `--fix` would not work. The
feature dies from its own ergonomics.

**Parse the shell's own history file after the fact.** No shell modification at
all. Rejected: bash and zsh history formats carry no exit code and no output, and
the formats differ between shells, versions, and `HISTFILE` settings.

**Require `script` or a pty wrapper.** Captures everything. Rejected: it
requires root, changes terminal behaviour, and is a much larger intrusion than
a hook.

**A gcode-managed shell session** (`gcode shell`). Full control, no intrusion
into the user's shell. Rejected as the primary path for a different reason than
the wrapper: the user has to adopt a different shell to use the tool. It remains
a reasonable escape hatch and is documented in
[../TROUBLESHOOTING.md](../TROUBLESHOOTING.md) for users who refuse hooks.

## Consequences

**Easier**

- Output, exit code, and cwd are captured at the source, accurately.
- The user's shell is otherwise untouched. `ls` is `ls`.
- Fish and nushell can be supported later by adding a file, because the
  differences are isolated to the hook, not the tool.
- `GCODE_NO_HISTORY=1` is a complete opt-out for users who will not accept any
  modification.

**Harder**

- Shell hooks are notoriously easy to get wrong. Clobbering `PROMPT_COMMAND`,
  eating `$?`, and breaking `set -u` are the classic failures, and all three
  happen to people.
- The hook runs on **every prompt**, so a 50 ms bug is felt constantly. The 5 ms
  budget is a real engineering constraint, not a nicety.
- Testing requires real shells in real containers, not unit tests.
- Cross-shell and cross-version differences are a permanent maintenance cost:
  `history 1` output format, `PROMPT_COMMAND` semantics under `set -u`, and
  `precmd` behaviour all vary.
- A user with an exotic shell config may hit a conflict we cannot reproduce.
  Hence `--init --remove` and clear recovery documentation.

**Forecloses**

- Nothing about the tool. This is an integration detail behind
  `context::history`, which can be fed by other means later.

## Validation

- Test in a real bash container: a failing command is recorded with the correct
  exit code, and `$?` is still visible to the user's own subsequent code.
- Test in a real zsh container: same guarantees via `precmd_functions`.
- Test: a user's pre-existing `PROMPT_COMMAND` survives install and remove,
  byte-for-byte.
- Test: install twice produces a file identical to install once.
- Test: the hook adds under 5 ms to a prompt, measured in CI and in the nightly
  benchmark.
- Test: sourcing the hook under `set -euo pipefail` does not break the shell.
- A user who reports a broken prompt gets a fix within one release. This is
  treated as a P0 correctness bug, not a preference.

