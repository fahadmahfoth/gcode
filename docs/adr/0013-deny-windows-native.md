# 0013. Deny native Windows for the v1.x line

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

Windows is a large terminal-user population, and excluding it is a real cost.
Windows 10 and 11 ship a terminal, PowerShell is a first-class shell, and the
developer population using Windows is substantial and growing.

But gcode is built on POSIX assumptions that are not incidental — they are
structural:

- The Bash grammar constrains generation. PowerShell has different syntax,
  different quoting rules, and different escaping.
- History capture is a `PROMPT_COMMAND` / `precmd` hook. PowerShell has
  `PSReadLine`, a different event model, and a different history format again.
- The whole execution and safety story is about `sh -c`, POSIX paths, `0600`
  permissions, and `umask`.
- The model stack builds against llama.cpp, which supports Windows, but every
  packaging, hook, and path assumption is Unix-shaped.

Supporting Windows "properly" would mean a second shell grammar, a second hook
implementation, a second history format, a second path model, and a second
packaging pipeline — roughly doubling the work for a version-1 audience that
WSL2 already serves well.

Supporting it *improperly* — a binary that runs under Git Bash or MSYS with
shimmed paths and approximate hooks — would be worse than not supporting it.
That is the failure mode this decision avoids: a tool that appears to work and
silently mishandles paths and history.

## Decision

No native Windows binary in the v1.x line. Windows users are supported through
WSL2, and the documentation says so explicitly.

1. No `*-pc-windows-*` target in the release matrix.
2. The README states the limitation plainly rather than burying it.
3. The install docs include a short WSL2 section, because "not supported" is not
   a complete answer.
4. A warning is issued if someone runs the Linux binary under MSYS or Git Bash,
   because that is the dangerous case: it half-works.
5. Revisit for v2.0, at which point the question is "is there a Rust-native path
   to supporting PowerShell" rather than "should we try".

## Alternatives considered

**Native Windows from v1.0.** Largest addressable market, and the honest
counter-argument: excluding Windows users is excluding a real and growing group.
Rejected on cost, not merit: it roughly doubles the maintenance surface before
the tool has a single user, and every hour spent on Windows path handling is an
hour not spent on the inference pipeline and the safety layer.

**A Windows build that runs under Git Bash, honestly labelled.** One binary, some
coverage. Rejected because the half-works case is the dangerous one. Users would
file confusing bug reports about path handling and history, and we would spend
the support time anyway while shipping something we do not trust.

**Windows via MSYS2 with a real POSIX layer.** More complete, and much more work:
a second CI matrix, a second installer, a second set of hooks, and ongoing
compatibility maintenance against a layer we do not control.

**A PowerShell-native gcode.** Architecturally cleaner than a POSIX build on
Windows, and genuinely appealing as a v2.0 project. It needs its own grammar,
its own risk patterns, its own hooks, and its own model tuning, and it is a
separate tool sharing a core. Not v1.0.

**Windows via WSL, promoted as the answer.** This is the decision: WSL2 is a
real Linux environment, it runs the same binary, and for a developer audience it
is a normal way to work. Documented clearly, with the trade-off stated.

## Consequences

**Easier**

- One shell model, one grammar, one hook design, one path model. Every test
  covers the only environment that exists.
- No dual-platform bug reports, and no ambiguity about which platform a bug
  report refers to.
- The release matrix stays at six targets instead of ten.
- Rust's cross-compilation story stays manageable.

**Harder**

- A real, measurable share of users cannot install gcode without changing how
  they work. This is a genuine loss of market, not a theoretical one.
- Competitive disadvantage against any tool that ships a Windows build.
- WSL2 has its own sharp edges: PATH translation, a separate filesystem, and
  performance characteristics that confuse people.
- Someone will run the wrong binary under Git Bash regardless of the warning, and
  we will get a bug report about it.

**Forecloses**

- Nothing permanent. This is the most reversible decision in the ADR set, which
  is appropriate: it is a scoping choice, not an architectural commitment.

## Validation

- The release matrix contains no Windows target.
- `gcode` detects MSYS and Cygwin and warns explicitly, with a pointer to the
  WSL instructions.
- The README states the limitation in the Requirements section, not in a
  footnote.
- The install docs have a tested WSL2 path, so "unsupported" is actionable
  rather than a dead end.
- The v2.0 roadmap entry is kept honest: it is a real project, not a checkbox.

