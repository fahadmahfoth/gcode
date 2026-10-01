# 0017. Explicit file modes, not a process-global umask

- **Status**: Accepted
- **Date**: 2026-10-01
- **Deciders**: gcode maintainers
- **Amends**: [0005](0005-jsonl-history-store.md), rule 6

## Context

ADR 0005 rule 6 says the history data directory is created with `umask 0077`.
That rule was written before there was code, and it names the wrong mechanism for
the outcome it wants.

`umask` is process-global state. A library that calls it changes the file mode of
every file the *host* process creates afterwards — not just its own. `gcode` is a
library crate (`src/context/history.rs`) that a shell hook, a test, or a future
embedding application will call. A call to `umask(0077)` from inside a history
append would silently tighten, or in the other direction loosen, unrelated files
that the embedding application is responsible for. No test in this repository can
detect that, because the effect is on somebody else's files.

The outcome rule 6 actually wants is narrow and worth stating on its own: the
history directory is `0700` and the history file is `0600`, and nothing wider. That
is a statement about two paths, not about the process.

There is a second gap. `umask` only affects files that are *created*. It does
nothing about a file that already exists — a `0644` history file left by an older
version, or by `touch`, or copied in from a backup, stays `0644`. Rule 6's intent
is not met by a fresh-install-only guarantee.

## Decision

Set modes explicitly on the two paths, and never call `umask`.

1. The history directory is created `0700`.
2. The history file is created `0600`, and its mode is set to `0600` again on every
   append, not only at creation. A pre-existing `0644` file is tightened.
3. Nothing in this repository calls `umask`.

The rotation sidecar `history.jsonl.1` is created `0600` by the same path.

## Consequences

- The guarantee holds for an existing file as well as a new one, which rule 6 did
  not deliver.
- An embedding application keeps whatever umask it chose.
- Tests must assert the mode directly rather than inferring it from the process's
  umask, which is what `src/context/history/tests.rs` does: it creates a
  pre-existing `0644` file and asserts the mode after the first append.
- Modes are Unix concepts. On platforms without them the calls are compiled out, so
  "the file is private" is not a claim this repository makes on those platforms. ADR
  0013 already denies native Windows for v1.x.
- `std::fs::Permissions::set_mode` follows symlinks. A history path that is a
  symlink to another file is therefore tightened at its target. This is the
  documented behaviour of `std`, it is not a new hole, and the history path is chosen
  by the user via `GCODE_HISTORY_FILE`.

## Alternatives considered

**Keep `umask 0077`.** Rejected: it makes a library mutate global process state for
an effect on two paths, and it cannot fix a file that already exists.

**Only set the mode at creation, as `OpenOptions::mode` does.** Rejected: it leaves
a pre-existing loose file loose, which is the case a user who has run an older
version is actually in.

**Refuse to start if the existing mode is too wide.** Rejected: it turns a
recoverable permissions problem into a refusal to run, for a file we can fix in one
syscall.