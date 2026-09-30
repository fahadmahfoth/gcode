# 0010. Embed the model registry at compile time

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

gcode needs a list of known models: name, download URL, SHA256, size, default
context size, licence. This list has to be available even when the user has no
config file, and it has to be trustworthy — a wrong URL or hash is a supply chain
attack.

The same list is also useful to read and edit, so it is nicer as a TOML file in
the repository than as a Rust constant.

## Decision

The registry is a TOML file at `models/registry.toml`, embedded into the binary
at compile time with `include_str!` and parsed at runtime.

1. `models/registry.toml` is the single source of truth, and it is in the
   repository so it can be reviewed, diffed, and argued about in a PR.
2. `include_str!` embeds it, so there is no runtime file dependency, no search
   path, and no "registry not found" failure mode.
3. Parsed lazily into a `OnceLock` at first use.
4. Parsing validates strictly: every entry must have a `sha256`, exactly one
   entry may set `default = true`, and a violation is a **compile error** via
   `include_str!` plus a `const` assertion or a build script check.
5. Users may add their own entries via config, and those are merged with lower
   precedence than the built-in defaults.
6. Changing the registry changes the binary, so the version and the registry are
   always consistent. A user cannot have a registry newer than their binary.

## Alternatives considered

**Hard-code the registry as a Rust constant.** No parsing, no allocation, no
failure mode, and the compiler can check it. Rejected: a list of URLs, hashes,
and sizes in Rust source is unpleasant to read and unpleasant to diff, and
reviewing a model addition is a data change that should look like a data change.

**Read the TOML from disk at runtime.** Editable without a rebuild, and a user
could point at a different registry. Rejected: it creates a path where a
user-writable file controls download URLs and, worse, expectations about checks.
Also adds a lookup-path failure mode to a tool whose whole pitch is
zero-configuration.

**Query the Hugging Face API for available models.** Always current, no stale
list. Rejected: it requires the network at first run, which breaks the offline
promise, and it means model metadata is not auditable in the repository. A model
list that changes without a commit is a list nobody has reviewed.

**A compiled binary search: no registry, just `--model <url>`.** Simplest.
Rejected: it loses the pinned checksums, which are the whole point, and it makes
`--use-model` and `--list-models` impossible.

**A signature over the registry file.** Detects tampering with a downloaded
registry. Rejected as unnecessary: the registry is inside the signed binary, so
tampering would mean tampering with the binary, which
[0016](0016-keyless-signing.md) already covers.

## Consequences

**Easier**

- No runtime file lookup, no path resolution, no "registry missing" error.
- Adding a model is a small, reviewable diff to a data file.
- The registry is auditable: `git log models/registry.toml` is the complete
  history of every model gcode has ever been willing to download.
- Compile-time validation means a malformed registry can never ship.
- Offline operation is unaffected by this choice, because the data is already in
  the binary.

**Harder**

- A registry fix requires a new release. Someone pinned to an old version cannot
  get a new model without upgrading, which is a real operational constraint.
- `include_str!` means the file must exist at build time, so a `cargo package`
  or vendored build must include it. A `build.rs` check or an `include` guard is
  needed to make the failure legible.
- The binary grows by the size of the registry, which is negligible.
- Compile-time validation needs either a build script or a const-assert trick.
  Build scripts add a compile step and can be a source of their own confusion;
  decided: a small `build.rs` with a clear error message.

**Forecloses**

- Remote registries. Deliberate, and consistent with
  [0001](0001-local-first-offline-inference.md).

## Validation

- Test: a fixture registry with two `default = true` entries fails to parse.
- Test: a fixture registry with a missing `sha256` fails to parse.
- Test: a fixture registry with an invalid `sha256` length fails.
- Test: `gcode --list-models` output is snapshot-tested, so an accidental
  registry change is visible in review.
- A CI step confirms `models/registry.toml` is included in the published crate.
- Every `sha256` in the registry is verified once by actually downloading the
  file. A checksum copied from a blog post fails this step.

