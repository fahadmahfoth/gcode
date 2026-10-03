# Architecture Decision Records

An ADR captures a decision that was **non-obvious**, had **real trade-offs**, and
would be **expensive to reverse**. Anything that does not meet all three does not
deserve a record; it goes in a commit message.

---

## The format

Each record lives in [`adr/`](adr/) as `NNNN-short-title.md` and is
**immutable once merged**. A decision that turns out to be wrong is not edited; it
is superseded by a new record that links back. That history is the point.

```markdown
# NNNN. Title

- **Status**: Proposed | Accepted | Superseded by [NNNN](NNNN-name.md)
- **Date**: YYYY-MM-DD
- **Deciders**: who
- **Consulted**: affected parties

## Context
The forces at play. Constraints, deadlines, what we already knew.

## Decision
What we are doing, stated plainly.

## Alternatives considered
Each with a real reason for rejection, not a strawman.

## Consequences
What this makes easy. What it makes hard. What it forecloses.

## Validation
How we will know it worked. The signal, and the number.
```

---

## Index

| # | Title | Status | Date |
|---|---|---|---|
| [0001](adr/0001-local-first-offline-inference.md) | Local-first, offline inference | Accepted | 2026-09-30 |
| [0002](adr/0002-rust-single-binary.md) | Rust and a single static binary | Accepted | 2026-09-30 |
| [0003](adr/0003-grammar-constrained-decoding.md) | Grammar-constrained decoding | Accepted | 2026-09-30 |
| [0004](adr/0004-independent-risk-classifier.md) | Risk classification independent of the model | Accepted | 2026-09-30 |
| [0005](adr/0005-jsonl-history-store.md) | JSONL append-only history | Accepted | 2026-09-30 |
| [0006](adr/0006-redact-before-prompt-assembly.md) | Redact before prompt assembly | Accepted | 2026-09-30 |
| [0007](adr/0007-shell-hook-integration.md) | Shell hooks over a wrapper script | Accepted | 2026-09-30 |
| [0008](adr/0008-critically-unrunnable.md) | CRITICAL commands are unrunnable | Accepted | 2026-09-30 |
| [0009](adr/0009-crates-io-dependencies.md) | crates.io only, no vendoring | Accepted | 2026-09-30 |
| [0010](adr/0010-embed-the-model-registry.md) | Embed the model registry at compile time | Accepted | 2026-09-30 |
| [0011](adr/0011-defer-the-plugin-system.md) | Defer the plugin system | Accepted | 2026-09-30 |
| [0012](adr/0012-one-liner-install.md) | One-liner install as the primary path | Accepted | 2026-09-30 |
| [0013](adr/0013-deny-windows-native.md) | Deny native Windows for v1.x | Accepted | 2026-09-30 |
| [0014](adr/0014-default-model-kitty-bash-llm.md) | Default model: kitty-bash-llm | Superseded by 0018 | 2026-09-30 |
| [0015](adr/0015-cargo-dist-over-distro-packaging.md) | cargo-dist as the release builder | Accepted | 2026-09-30 |
| [0016](adr/0016-keyless-signing.md) | Keyless release signing | Accepted (see 0019) | 2026-09-30 |
| [0017](adr/0017-explicit-file-modes-over-umask.md) | Explicit file modes over umask | Accepted | 2026-10-01 |
| [0018](adr/0018-bilingual-default-model.md) | Default model: qwen3-0.6b | Accepted | 2026-10-03 |
| [0019](adr/0019-local-ci-not-github-actions.md) | Local CI, not GitHub Actions | Accepted | 2026-10-03 |
| [0020](adr/0020-clipboard-via-platform-tool.md) | Copy to the clipboard by shelling out | Accepted | 2026-10-03 |
| [0021](adr/0021-http-client-behind-a-feature.md) | One HTTP client, optional, confined to the downloader | Accepted | 2026-10-03 |
| [0022](adr/0022-no-signal-handler.md) | No signal handler; Ctrl+C is the process default | Accepted | 2026-10-03 |
| [0023](adr/0023-executor-inherits-stdio.md) | The executor runs `sh -c`, inherits stdio, captures nothing | Accepted | 2026-10-03 |
| [0024](adr/0024-confirm-every-command-by-default.md) | Confirm every command by default | Accepted | 2026-10-03 |
| [0025](adr/0025-two-licences-beyond-the-allowlist.md) | Accept MPL-2.0 and CDLA-Permissive-2.0 | Accepted | 2026-10-03 |
| [0026](adr/0026-msrv-1-85.md) | Raise the minimum supported Rust version to 1.85 | Accepted | 2026-10-03 |

---

## The decisions that would hurt most to reverse

If you inherit this project and change one of these, you need a very good reason:

**Local-first (0001).** Every feature, every model choice, every line of the
prompt, and the entire privacy story follows from this. A cloud-backed tier is
compatible with it, but a cloud-first design is a different product.

**Grammar-constrained decoding (0003).** It is the load-bearing mitigation for
prompt injection, not just a nicety. Removing it changes the security model
fundamentally.

**Independent classification (0004).** The moment risk assessment depends on the
model's own opinion, the safety model becomes circular and worthless.

**CRITICAL is unrunnable (0008).** A soft block is a block users will learn to
bypass. This one is deliberately user-hostile in order to be safe.

**Defer the plugin system (0011).** Building it early means designing against a
guessed API. This one is cheap to reverse now and expensive later.

---

## Writing a new ADR

Add a row to the index above, in the same commit as the file. Use the next
number. Do not renumber. Do not delete.

Ask these three questions first — if any answer is "no", it is a commit message:

1. Would a competent engineer joining this project plausibly choose differently?
2. Are there real alternatives that we genuinely rejected?
3. What would it cost to change our minds in six months?
