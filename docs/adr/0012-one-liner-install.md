# 0012. One-line install as the primary path

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

The stated goal is that a user goes from nothing to a working command in 30
seconds. For a terminal tool, the entire addressable market is decided by the
first thirty seconds. A tool that takes five minutes to install has already lost
the casual user, and the casual user is the majority of the market.

But `curl … | sh` is also the command that security reviewers distrust the most
correctly. It executes remote code as root, often without the reader understanding
what it does. Recommending it is a real trade-off, not a free win.

The question is how to get the thirty seconds without lying about the risk.

## Decision

The one-line installer is the primary install path, is a readable POSIX `sh`
script in this repository, and is documented with an explicit statement of what
it does and how to audit it.

1. `curl -fsSL https://get.gcode.dev | sh` is the first thing in the README.
2. The script lives in the repository at `scripts/install.sh`, is under 200
   lines, and has no obfuscation, no base64, and no `eval`.
3. The README links to the script source and says explicitly that it is
   auditable and short enough to read.
4. It verifies the SHA256 of the artefact **before** extracting anything.
5. It installs to `/usr/local/bin` if writable, otherwise `~/.local/bin`,
   requesting `sudo` only when genuinely unavoidable.
6. It never modifies a shell config outside marked, reversible blocks.
7. It runs a self-test at the end and exits non-zero if that fails.
8. `GCODE_NO_MODEL=1`, `GCODE_NO_INIT=1`, and `GCODE_INSTALL_DIR` let a user opt
   out of each step.
9. Package managers, Docker, and a source build are documented alongside as
   fully supported alternatives — never as second-class options.

## Alternatives considered

**Package managers only (`apt`, `brew`, `yay`).** Most trustworthy, and the
right answer for anyone who thinks about it. Rejected as the *primary* path
because distribution setup is slow: publishing to five repositories, each with
its own review queue, its own delay, and its own failure mode. Most users would
hit a repository that does not have the version they read about.

**Build from source.** Maximum trust, maximum friction. Requires a Rust
toolchain, which is a 300 MB prerequisite for an 8 MB binary. Correct for
contributors, disqualifying for users.

**A GUI installer.** Best experience for people who do not use terminals, and
gcode is a terminal tool. Not the audience.

**A `curl` one-liner with no self-verification.** Faster to write. Rejected
outright: an unauthenticated script that pipes an unverified binary into a root
path is indefensible in a tool whose entire pitch is safety.

**Homebrew-only as the primary path.** Homebrew is excellent and free to use on
Linux too. Rejected because it excludes users without Homebrew, and because a
single-vendor primary path means a single point of failure outside our control.

**Ship a container image only.** Perfectly reproducible. Rejected: it excludes
every user without Docker, which is most of the market.

## Consequences

**Easier**

- Maximum reach. The only prerequisites are `curl`, `tar`, and a POSIX shell,
  which is a safe assumption on the target platforms.
- Install time is dominated by the 8 MB binary, not by setup, so the thirty
  seconds is achievable.
- The script is the same artefact for every platform, so there is one code path
  to test rather than five.
- Users on package managers still have a supported, arguably better path.

**Harder**

- We are recommending a pattern that will be copied carelessly by others, and we
  are responsible for the norm. This is a real cost of the choice.
- A compromised `get.gcode.dev` or a hijacked GitHub release would be very bad.
  Mitigated by the SHA256 check, keyless signing, and SLSA provenance, but the
  script itself is still remote code.
- The script must handle six platform combinations and a range of shells, and
  shell scripting is where portability bugs live. `shellcheck` in CI and a
  Docker test matrix are mandatory, not optional.
- Some organisations block `curl | sh` by policy, and that is fine — the
  alternatives are documented one click away.

**Forecloses**

- Nothing. This is a distribution choice, entirely reversible, and independent of
  the code.

## Validation

- The script passes `shellcheck` with no warnings.
- The install script test suite runs in a Docker matrix across all six
  supported platforms.
- A test verifies that a checksum mismatch aborts **before** any file is written.
- A test verifies the script is idempotent.
- A test verifies the `GCODE_NO_MODEL` and `GCODE_NO_INIT` paths do what they
  say.
- The script is short enough that a reviewer can read it in one sitting, and
  this is checked by a human rather than a metric.
- The README states plainly what the script does, and that it can be read before
  running.

