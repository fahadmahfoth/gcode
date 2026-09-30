# 0002. Rust and a single static binary

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

gcode's central promise is that a user goes from nothing to a working command in
30 seconds. That constraint eliminates almost every language choice:

- **Interpreted languages** (Python) require a runtime the user may not have, and
  the install step grows from one command to several.
- **Go** produces a single static binary and starts fast, but its cgo and
  cross-compilation story for llama.cpp is workable rather than pleasant, and its
  runtime characteristics make embedding a large model less predictable.
- **C and C++** would be the most direct route to llama.cpp and would produce the
  smallest, fastest binary, at the cost of memory-safety guarantees in a tool
  that shells out to the user's commands.
- **Node** requires a runtime, which breaks the promise outright.

The tool also needs to parse model output, match a large regex table, manipulate
UTF-8 from arbitrary byte streams, and do all of this in under a second. Memory
safety is not an abstract virtue here: a panic in the classifier is a crash
report, and an out-of-bounds read while slicing model output is a security
problem in a binary the user runs as themselves.

## Decision

Rust 1.75+, one static binary, musl targets for Linux, no runtime dependency.

1. Core language is Rust, edition 2021, MSRV 1.75.
2. Ship exactly one artefact per platform. No interpreter, no shared library, no
   system package beyond libc.
3. Link llama.cpp through `llama-cpp-rs` (FFI) rather than reimplementing
   inference.
4. `panic = "abort"` in release: a CLI that cannot handle an unexpected state
   should exit loudly, not unwind through a `Drop` that might write to a
   half-finished history entry.
5. Linux builds target `*-unknown-linux-musl` for true static linking.
6. Grammar constraints come from `llguidance`, which is Rust-native, rather than
   C++ grammar libraries requiring a second FFI boundary.

## Alternatives considered

**Go.** Single binary, easy cross-compilation, fast startup. Real advantages. The
deciding factor was that llama.cpp integration means either cgo (which breaks
cross-compilation and complicates static linking) or a C ABI shim (an untyped
boundary in the one place where malformed data flows in). Rejected: we would
have traded a type-safe FFI boundary for a convenience we did not need.

**C++.** The most natural fit for llama.cpp and the fastest binary. Rejected on
memory safety. A tool that runs as the user, parses arbitrary model output, and
decides what is dangerous should not have use-after-free as a failure mode.
Reviewability by contributors was a secondary but real factor.

**Python with PyInstaller.** Fast to prototype, and PyOxidiser makes a
single-file binary. Rejected: 30+ MB artefacts, slow cold start, and
`llama-cpp-python` adds a compiled dependency chain that breaks the "no runtime"
promise on half the platforms.

**Zig.** Compelling for a static binary with C interop. Rejected: the
contributor pool is far too small for a project that needs to attract
contributions.

## Consequences

**Easier**

- One file, no install friction, no runtime to explain.
- Static linking means the Alpine and glibc-vs-musl matrix collapses to "does it
  run", not "are the symbols right".
- Memory safety removes an entire class of bug reports.
- Contributor onboarding is a single `cargo build`.
- `cargo-dist` handles cross-compilation, installers, checksums, and signatures
  in one place.

**Harder**

- Cross-compiling to aarch64 from x86_64 needs a cross-linker and a cross-built
  llama.cpp. This is the single most annoying part of the build.
- llama.cpp moves fast, and every bump is an FFI surface change. Pinning
  upstream and wrapping it behind `InferenceEngine` is mandatory, not optional.
- Compile times are long enough that contributors notice.
- Any future contributor must learn Rust. That is a real recruiting cost, and it
  is the price of the safety argument.

**Forecloses**

- Nothing significant. The `InferenceEngine` trait means a second inference
  backend in another language could be added behind the same interface without a
  rewrite, though nobody should do that without reason.

## Validation

- `scripts/build-all.sh` produces artefacts for all six targets from a clean
  checkout on both Linux and macOS.
- `ldd` on the Linux artefact reports `not a dynamic executable`; on macOS,
  `otool -L` shows only system libraries.
- Cold start stays under 500 ms in the nightly benchmark.
- Binary size stays under 15 MB stripped.

