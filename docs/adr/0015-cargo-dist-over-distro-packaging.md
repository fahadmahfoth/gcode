# 0015. cargo-dist as the release builder

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

A release needs binaries for six targets, tarballs and archives, checksums,
shell completions, a man page, and ideally platform-native installers — plus
signing and provenance. Done by hand this is a two-day chore that nobody wants
to do, and doing it inconsistently is how checksum mismatches and missing
artefacts happen.

The open question is how much of this is automated versus how much is written by
hand, because the honest answer for a small project is that hand-written CI glue
becomes unmaintained within three releases.

## Decision

`cargo-dist` builds and publishes the release artefacts. Hand-written packaging
metadata exists only where a native package format demands it.

1. `cargo-dist` produces the binary matrix, archives, installers, checksums, and
   shell completions from `dist-workspace.toml`.
2. `.deb` and `.rpm` are produced by `cargo-dist`; the maintainer-written files
   in `packaging/` are limited to distribution-specific metadata —
   maintainer scripts, package splitting for the model, and licence files.
3. AUR `PKGBUILD` and the Homebrew formula are generated templates, kept in the
   repository so users can install without a tap, and updated by a bot on
   release.
4. The man page and completions are generated from the same `clap` definitions,
   so they cannot drift from `--help`.
5. The APT repository is published from GitHub Pages by a workflow that signs with
   a key the maintainer holds 🔒. This is the one thing cargo-dist does not do.
6. A `verify-release.sh` script installs the artefact on a clean machine and
   smoke-tests it. Nothing is published without passing it.

## Alternatives considered

**Fully hand-written CI.** Total control, no tool dependency, and a YAML file
that grows to several hundred lines and breaks whenever a target triple changes.
Rejected: the maintenance cost lands on whoever does releases, which is one
person, repeatedly.

**GoReleaser.** Excellent, and comparable in capability. Rejected: it is
Rust-unaware, so it would not produce completions or a man page from `clap`
definitions, and the drift risk between the CLI and its documented flags is
exactly what we want to avoid.

**Native distro build systems only** (Debian's `dpkg-buildpackage`, RPM's
mock). Highest fidelity to each distribution's conventions, which matters if we
want to be accepted into Debian proper eventually. Rejected as the primary path:
the build environment requirements are heavy, the matrix is slow, and six
platforms of it is a lot of CI. Kept as a follow-up for a proper Debian
submission, which is a separate project.

**A CI matrix that builds natively on each platform.** Slowest and most faithful.
Rejected: six runners, long wall time, and it is testing the runners as much as
the code.

**Publish a container image only.** One artefact, fully reproducible. Rejected as
the primary path: it excludes everyone without Docker.

**Nothing automated, a documented manual process.** Zero tooling. Rejected:
releases would happen twice a year and be wrong every time.

## Consequences

**Easier**

- Releases are reproducible from a tag, which is what
  [0016](0016-keyless-signing.md) and the provenance attestation build on.
- Completions and the man page cannot drift from the CLI.
- Adding a target is a config change, not a CI rewrite.
- The APT repository can be built by a maintainer with one command, and verified
  by a script.

**Harder**

- A dependency on `cargo-dist`'s release cadence. Its major version upgrades will
  need attention, and its generated installers have occasionally had packaging
  bugs of their own.
- Debian and RPM packages that are not quite what a distribution maintainer would
  write. Acceptable for our own repository; a real Debian submission will need
  the native toolchain and will be handled separately.
- Generated formulae need a bot to keep them updated, and that bot is another
  thing that can silently break.
- `--locked` plus a generated artefact means a lockfile change can break a
  release. Caught in CI, but it is a class of surprise.

**Forecloses**

- Nothing structural. The artefacts are the artefacts; changing the builder
  later is transparent to users.

## Validation

- `cargo dist` runs in CI on a tag and produces every expected artefact.
- `scripts/verify-release.sh` installs and smoke-tests the artefact on a clean
  machine before publication.
- CI asserts the published asset list is complete: all six binaries, checksums,
  the man page, and both completions.
- A weekly dry-run release workflow catches breakage before it is needed.
- The Homebrew formula and `PKGBUILD` are verified to build on a schedule, not
  only on release day.

