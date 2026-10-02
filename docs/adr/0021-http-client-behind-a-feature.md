# 0021. One HTTP client, optional, confined to the downloader

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Amends**: [0001](0001-local-first-offline-inference.md), [0002](0002-rust-single-binary.md)

## Context

[0001](0001-local-first-offline-inference.md) allows network access in exactly one
place: the model downloader. The downloader has existed since Phase 1.4 as a
`Transport` trait with a fake implementation and a full test suite, but the real
socket was never attached. `--download-model` was documented and had no
implementation.

Attaching it needs an HTTP client. The requirements are narrow: HTTPS with the
system trust store not required, byte-range requests so an interrupted transfer
resumes, and streaming response bodies so a 400 MB file is never buffered whole.

## Decision

**Add `ureq` as an optional dependency, enabled only by the `download` feature,
and name it in exactly one file, `src/model/download.rs`.**

- `ureq = { version = "3", optional = true, default-features = false, features = ["rustls"] }`.
  Its declared minimum Rust version (1.71) is below this project's floor (1.75), so
  it does not move the MSRV.
- rustls rather than `native-tls`: no link against the platform TLS stack, which
  keeps [0002](0002-rust-single-binary.md)'s single-static-binary property and
  removes a class of "works on my machine" trust-store differences.
- `HttpTransport` is behind `#[cfg(feature = "download")]`. A build without the
  feature carries no HTTP client at all and reaches `Error::DownloadUnavailable`
  when a download is requested.
- The existing `scripts/ci.sh` network-containment check already fails the build
  if `ureq`, `reqwest`, `hyper::client`, or `TcpStream` appears anywhere in `src/`
  except `src/model/download.rs`. That check is the enforcement.

## Alternatives considered

**Make `ureq` a default, non-optional dependency.** Simpler: no feature flag.
Rejected: it puts an HTTP client in every build, including one where the model is
installed by a package manager and the downloader is never used. The feature is
the machine-checkable form of "no network except the downloader".

**`reqwest` with `rustls`.** More familiar, richer API. Rejected: a far larger
dependency tree (`tokio`, `hyper`) for a blocking one-shot GET, against a project
whose size budget is measured in megabytes.

**`native-tls` instead of rustls.** Uses the platform trust store, which some
users prefer. Rejected: linking OpenSSL on Linux reintroduces a runtime
dependency that [0002](0002-rust-single-binary.md) exists to remove.

**Keep `Transport` fake-only.** Ship no downloader, leave the flag documented as
unimplemented. Rejected: the roadmap's download task is not met, and the module
already claims a real implementation is behind the feature.

## Consequences

**Easier**

- `gcode --download-model` works, verifies the SHA-256 the registry pins, and
  resumes an interrupted transfer from the `.part` file.
- Builds that do not need the network are unchanged and provably free of an HTTP
  client.

**Harder**

- A release build with `--features download` now compiles `ring`, which needs a C
  toolchain at build time. The default build and the whole test suite do not.
- `ureq` joins the `cargo audit` / `cargo deny` surface. It is the only crate in
  the tree that talks to a socket.

## Validation

- `scripts/ci.sh`'s network-containment check passes: `ureq` appears only in
  `src/model/download.rs` and the manifest.
- The downloader's correctness — digest verification, resume, mirror fallback,
  partial-file handling — is tested against `Transport` fakes and is unchanged.
- `--download-model` without the feature reports `DownloadUnavailable`; with a
  name the registry does not have, it reports `UnknownModel` before any transfer.
