# 0009. crates.io dependencies, not vendored sources

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

gcode depends on llama.cpp through an FFI boundary, and that is the dependency
that matters. The rest of the tree uses well-known crates: `clap`, `serde`,
`tokio`-free `reqwest`, `regex`, `toml`, `sha2`, `anyhow`, `thiserror`.

Supply-chain security in a tool that runs as the user with the user's privileges
is not a theoretical concern. A compromised transitive dependency that executes
during the build, or in the binary, gets everything the user has.

Vendoring (`cargo vendor` plus a committed `vendor/`) is the common response:
the source is in the repository, so a compromised upstream cannot change what we
build.

## Decision

Dependencies come from crates.io with a committed `Cargo.lock`, enforced by
`cargo deny`, `cargo audit`, and `cargo vet`. No vendored source in the repository.

1. `Cargo.lock` is committed and CI builds with `--locked`, so the dependency
   tree is identical everywhere and cannot drift.
2. `cargo deny` enforces a licence allowlist (MIT, Apache-2.0, BSD, ISC, Unicode)
   and an advisory policy.
3. `cargo audit` fails CI on any unpatched RustSec advisory.
4. `cargo vet` records the audited dependency set, so an unexpected new
   transitive dependency is a visible change in review.
5. llama.cpp is pinned to a specific upstream commit, not a branch, with the
   commit recorded in the build script and in the release notes.
6. Release artefacts are signed with [keyless OIDC](https://github.com/sigstore/fulcio)
   (see [0016](0016-keyless-signing.md)) and carry SLSA provenance, so a user can
   verify what they installed without trusting a repository mirror.

## Alternatives considered

**Vendor all sources.** Removes the runtime risk of upstream changing under us.
Rejected: the `vendor/` directory would be tens of megabytes of third-party code
in the repository, and it does not actually help — the risk is the *build*, and a
vendored build still runs every build script from those dependencies. It trades
reviewability (nobody reads vendored code) for a false sense of control.

**Minimal dependencies: standard library only.** The strongest position, and
worth pursuing at the margins. Rejected for the core because `clap` alone would
be several thousand lines of reimplementation, and hand-rolled argument parsers
have their own bug classes. Honest policy instead: add a dependency only with
justification, in the PR.

**A curated allowlist of crates.io publishers.** Reduces the supply-chain
surface. Rejected for v1.0: crates.io does not support per-publisher pinning
natively, and the added tooling is not worth it for ~15 direct dependencies. The
`cargo vet` audited list covers most of the intent.

**Reproducible builds plus a verified source mirror.** Strong, and mostly
complementary. Deferred: reproducible Rust builds remain difficult, and the
provenance attestation in [0016](0016-keyless-signing.md) addresses the actual
user-facing concern — verifying which artefact was installed.

## Consequences

**Easier**

- A clean checkout builds anywhere with no vendoring step and no 50 MB directory
  in the diff.
- `cargo update` and a lockfile diff are the entire dependency review process,
  which fits naturally into code review.
- Licence compliance is a CI check rather than an audit.

**Harder**

- `--locked` means a lockfile update is required for every dependency bump, and
  a stale lockfile fails the build. This is the intended friction.
- The build still executes third-party build scripts. `cargo deny` cannot fully
  vet those. Honest limitation, documented in [SECURITY.md](../../SECURITY.md).
- Upstream llama.cpp is a moving target; pinning means a manual bump process, and
  it is a recurring chore that someone has to own.
- A crates.io outage blocks a fresh build. Mitigated by `Cargo.lock` and by
  crates.io's availability record, not eliminated.

**Forecloses**

- Nothing about the design. This is a policy decision and is reversible at any
  time by adding a vendor directory.

## Validation

- `cargo audit` reports zero advisories in CI.
- `cargo deny` passes the licence allowlist in CI.
- `cargo vet` is up to date, so `cargo deny` flags any dependency not in the
  audited list.
- CI builds with `--locked`; a modified lockfile fails the build.
- A secret scan and a licence scan both run on every PR.
- A documented, reproducible process exists to rotate the signing key and to
  revoke artefacts, exercised at least once before 1.0.

