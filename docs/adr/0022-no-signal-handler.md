# 0022. Do not install a signal handler; Ctrl+C is the process default

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Relates to**: [0002](0002-rust-single-binary.md), [0004](0004-independent-risk-classifier.md), [0021](0021-http-client-behind-a-feature.md)

## Context

The confirmation prompt is documented as "`Ctrl+C` cancels the invocation", and
the roadmap listed a Ctrl+C key behaviour as open and "not implementable as
written". Two facts had to be reconciled:

1. The crate is `#![forbid(unsafe_code)]`, so a hand-written `libc` signal
   handler is out. The portable option is a crate such as `ctrlc`.
2. The prompt reads a line with `std::io::stdin().read_line`, and Rust's standard
   library **retries** an interrupted read on `EINTR`. A handler that only sets an
   atomic flag therefore cannot unblock the prompt: the read resumes and the user's
   Ctrl+C appears to do nothing.

`ctrlc` installs a process-wide handler for the life of the process. That means a
handler bought for the downloader would also swallow Ctrl+C at the prompt, turning
a working default into a hang. Scoping the handler to the download alone is not
portable: the crate does not offer a supported reset, so the handler outlives the
transfer it was installed for.

## Decision

**Install no signal handler.** Ctrl+C keeps the operating system default: the
process is terminated with the usual `128 + SIGINT` status. This is what "cancel
the invocation" means, and it is already what `docs/gcode.1` and `docs/USAGE.md`
promise.

The consequence that matters is preserved for free: a terminated download leaves
its `<name>.part` file on disk, because nothing in the crate deletes it on exit,
and the next `--download-model` resumes from the byte range. Correct resume was
never contingent on a graceful shutdown.

`Ctrl+C` during generation is likewise the process default. The roadmap's
"cooperative cancellation with a message" is dropped: it would cost a dependency,
an `unsafe` boundary through that dependency, and a handler that regresses the
prompt, to replace a default that already does the right thing.

## Alternatives considered

**Add `ctrlc` and stop the download cooperatively.** A nicer message ("cancelled;
the partial file is kept") and a clean return code. Rejected: the only long
operation it improves is the download, the message is cosmetic, and the
process-wide handler makes Ctrl+C a no-op at the prompt — a safety-relevant
regression. The `.part` file and the resume path already survive the default.

**A `libc` handler with `SA_RESETHAND`, or a self-pipe.** Would need `unsafe`,
which the crate forbids, and an ADR to lift it. Not worth it for the message.

**Drain stdin on SIGINT instead.** Still needs a handler, and draining does not
cancel an in-flight generation; the model call would keep running.

## Consequences

**Easier**

- No new dependency, no `unsafe`, no signal-safety reasoning.
- The documented behaviour and the actual behaviour match with no code.
- An interrupted download resumes, which is the property users notice.

**Harder**

- There is no custom "cancelled" message: the shell prints the familiar `^C` and
  the exit status. Accepted; the shell is where a user expects to see that.
- If a future feature needs graceful state on shutdown, it will need this ADR
  superseded with a mechanism that does not hold a handler past the operation.

## Validation

- No signal handler is registered anywhere in the crate.
- `grep -rn 'ctrlc\\|SIGINT\\|signal_hook' src/` returns nothing.
- `--download-model` interrupted by SIGINT leaves a `.part` file, and the next
  run resumes from its length (covered by the resume tests with `FakeTransport`).
