# 0020. Copy to the clipboard by shelling out, not by linking a crate

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Relates to**: [0001](0001-local-first-offline-inference.md), [0002](0002-rust-single-binary.md)

## Context

The confirmation prompt offers `c` to copy the command, so a user can paste it
somewhere without selecting it by hand. The key was previously recognised and
declined: the roadmap recorded that every clipboard path meant either a new crate
or a platform tool, and that was an unmade decision.

There is no clipboard API in the standard library and no single cross-platform
one that does not pull a dependency. The alternatives were:

1. a crate such as `arboard` or `copypasta`, which links the platform clipboard;
2. shelling out to whatever tool the platform already has;
3. leaving `c` declined.

## Decision

**Shell out to the platform tool.** On macOS, `pbcopy`. On Linux, try `wl-copy`,
then `xclip -selection clipboard`, then `xsel --clipboard --input`, in that
order. If none is present, report that the copy failed and ask again; never treat
a failed copy as an answer.

The mechanism lives in `src/ui/prompt.rs`, is injectable for tests (`copying`),
and is exercised only through that seam. A test never starts a real clipboard
process.

## Alternatives considered

**Link a clipboard crate.** One call, no `PATH` dependency. Rejected: it is a new
dependency with its own platform backends and maintenance surface, for a
convenience key. The project already shells out to `$EDITOR` for `e`, so the
pattern and the failure handling are established.

**Leave `c` declined.** Honest, and what the roadmap said before. Rejected now
that the decision to shell out is recorded: the key is cheap to implement
correctly and the failure mode (no tool installed) is reported, not hidden.

## Consequences

**Easier**

- No new dependency to audit, license-check, or carry in the MSRV matrix.
- The behaviour on a headless Linux box is a clear "could not copy", not a panic.

**Harder**

- Copying depends on `PATH` having a clipboard tool. On a minimal Linux install
  it may not, and the user sees a failure rather than a silent no-op. That is the
  intended trade.

## Validation

- `c` copies the command currently on screen, including one produced by a
  preceding `e`, and never grants consent on its own.
- A failed copy reports it and re-asks; a following `y` still works.
- Tests inject the copy function, so the suite spawns no clipboard process.
