# 0005. JSONL append-only history store

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

`gcode --fix` and context-aware generation both need recent shell history with
exit codes and output. That data has to survive across processes, be readable
without the tool running, survive crashes mid-write, and never need migrating.

It is also sensitive: it contains the user's real commands and, occasionally, the
output of commands that printed a token. It lives in a file the user's own
account can read.

Working set: about 15 entries of a few kilobytes each, read on every invocation,
written once per executed command.

## Decision

Append-only JSONL at `~/.gcode/history.jsonl`, mode `0600`, no database.

1. One JSON object per line, one line per executed command.
2. Reads go **backwards** from the end and stop after N entries. A large file
   must not be fully parsed to get the last 15 records.
3. Writes are a single `write` of one line followed by `fsync`. A torn line from
   a Ctrl-C is discarded on read.
4. A malformed line anywhere is skipped, never fatal. One warning on stderr.
5. Rotation at 10 MB: keep the newest 5 MB as `history.jsonl`, previous as
   `history.jsonl.1`.
6. The data directory is created with `umask 0077`.
7. No index, no daemon, no compaction, no background process.

## Alternatives considered

**SQLite.** Queryable, transactional, handles concurrent writers properly.
Rejected: a C dependency in a project that otherwise has none, a schema to
migrate, and a database file where a text file would do. The access pattern is a
ring-buffer tail read, which a text file handles natively. Revisit if we ever
need structured queries over months of history, which we currently do not.

**A binary format.** Faster and smaller. Rejected: it makes the file unreadable
without the tool, which destroys a genuinely useful property — a user debugging
their own history should be able to `tail` it. Greppability is a feature for a
developer-facing tool.

**The shell's own history file.** No new file at all. Rejected: bash and zsh
history formats do not include exit codes or output, both of which are
load-bearing for `--fix` and for context. Reading the user's shell history file
directly would also mean parsing two incompatible formats and depending on
`HISTFILE` conventions that vary widely.

**A daemon holding history in memory.** Faster. Rejected: adds a background
process, a startup dependency, and a new failure mode, for a workload of about
15 entries. A CLI that must not depend on a daemon should not need one.

**Only gcode's own commands, not full shell history.** Narrower privacy exposure.
Rejected: `--fix` needs the *user's* failed command, not gcode's. The tool's
value depends on seeing what actually happened on their shell.

## Consequences

**Easier**

- No dependency, no migration, no daemon, no lock protocol.
- A user can read, grep, truncate, or delete their history with `cat`, `grep`,
  `wc`, and `rm`. That is a privacy feature.
- Crash safety is a property of `O_APPEND` plus single-line atomicity on POSIX,
  which is well understood.
- Truncation is a `rm`. There is nothing to vacuum.
- Backup and sync work with any file tool.

**Harder**

- Concurrent writers from multiple terminals interleave. On POSIX, `O_APPEND`
  writes under the pipe buffer size are atomic, so this is safe for realistic
  line lengths, but it is a real limit. A line longer than the atomic write size
  could interleave; the 2 KB output cap keeps lines small.
- Reading backwards requires knowing where lines end. Line boundaries are found
  by scanning for `\n` backwards from EOF, which is straightforward and fast.
- No indexed queries. Fine for the access pattern; would need revisiting for
  analytics.
- A very large file is slow to open if the last line is not near EOF. Bounded by
  rotation at 10 MB.
- No schema evolution. Adding a field is backward compatible because readers
  ignore unknown fields, but renaming one is not. Handled by never renaming.

**Forecloses**

- Nothing significant. The format is an implementation detail behind
  `context::history`, so it can be replaced with a real store later without
  touching callers — the only cost would be migrating existing files.

## Validation

- Test: read the last 3 of 100 entries returns the correct 3.
- Test: a corrupt line at position 50 does not stop the read and warns once.
- Test: a truncated final line is silently discarded.
- Test: a 10 MB fixture rotates and the result stays under 5 MB.
- Test: the file mode is `0600` after the first append, on a clean umask.
- Test: 100 concurrent appends from parallel processes produce 100 valid lines.
- `gcode doctor` reports the file size, entry count, and mode.

