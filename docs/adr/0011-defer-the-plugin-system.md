# 0011. Defer the plugin system

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

gcode is a tool that runs commands as the user. The natural extension is
plugins: someone else writes the risk patterns, the model integrations, the
output formatters, the shell integrations. That is how tools become platforms,
and it is how a small project scales past the maintainer's own taste.

It is also, for this project, the highest-risk thing we could build early.

The safety model ([0004](0004-independent-risk-classifier.md),
[0008](0008-critically-unrunnable.md)) is the entire value proposition. A plugin
API that can register risk patterns, or wrap the executor, or hook the
confirmation flow, is an API for disabling our safety guarantees. Designing that
API before we know what people actually want to build means designing it against
a guess, and the guess will be wrong in ways that are expensive to fix once
plugins exist in the wild.

## Decision

No plugin system before v1.0, and no plugin API design until there are at least
ten real plugins that cannot be expressed with what exists.

1. Phases 1–6 contain no plugin system and no plugin API surface.
2. Extension happens through the existing, deliberately small mechanisms:
   `--model` for models, `[safety.blocklist]` for risk patterns, files in
   `shell/` for shell support.
3. The `InferenceEngine` trait exists and is the seam a backend plugin would
   attach to, but it is a Rust trait, not a dynamic interface. It is not a
   plugin API and is not documented as one.
4. Phase 10 exists in the roadmap as a placeholder with no design, specifically
   so the deferral is a recorded decision rather than an omission.
5. Any future plugin system must be **out-of-process**. In-process plugins that
   can register risk patterns would break [0008](0008-critically-unrunnable.md),
   because a plugin could then lower a level or unblock a refusal.

## Alternatives considered

**WASM plugins, designed now.** Sandboxed, portable, a real ecosystem. This is
almost certainly the eventual answer, and it is written in the roadmap so we do
not forget. Rejected *now* for the guessing problem, and because a sandbox is not
automatically a security boundary for a tool that runs shell commands — a WASM
guest with file and network access to the user's data is a large hole to design
before we know the threat model.

**A Lua configuration layer.** Lowering the barrier for users to extend gcode
without compiling. Genuinely valuable and much lower risk than a plugin API,
because a configuration language cannot bypass the classifier. Deferred, not
rejected — it is a reasonable v1.2 feature.

**An open registry of risk patterns as plain TOML.** Users contribute patterns
through a PR to the repository. Rejected in favour of the *local*
`[safety.blocklist]`: a shared remote pattern list is a remote control over what
users consider dangerous, and it is a supply chain risk. PR-reviewed patterns in
the binary are auditable; a remote list that changes silently is not.

**A public Rust crate API for embedding gcode's classifier.** Lowering the cost
of building on top. Attractive, and it is the most likely candidate for a future
"stable core" release. Rejected for now: a public API is a compatibility
promise, and ours is not stable until 1.0.

**An LSP-style extension protocol via stdio.** The most conventional shape.
Rejected: it makes gcode a long-running server, which breaks the "no daemon"
property from [0002](0002-rust-single-binary.md) and [0005](0005-jsonl-history-store.md).

## Consequences

**Easier**

- The safety model is a small, auditable, testable core with no dynamic
  behaviour.
- No sandbox to design, no capability model, no versioning story for a plugin
  API.
- Every user gets the same risk patterns. A bug in a pattern is fixed for
  everyone on upgrade.
- Contributors can read the whole codebase without wondering which parts
  dynamically load code.

**Harder**

- The project cannot be extended by people who do not want to fork or submit a
  PR. This caps the ecosystem and is a genuine competitive weakness against
  tools with mature extension systems.
- Some genuinely useful functionality will be blocked on a PR rather than
  available immediately.
- If a competitor ships plugins first, catching up later is harder than leading.
  Accepted: plugins built on a guessed API are the thing most likely to need
  throwing away.
- Custom shell support means contributing a file to the repository. Fish and
  nushell support is therefore a PR, not a user action (Phase 8).

**Forecloses**

- Nothing irreversible. This is the cheapest decision in the project to reverse
  once the plugin count justifies it, which is exactly why it should be made now
  rather than half-committed to later.

## Validation

- `rg 'wasm|plugin|lua|dylib|dlopen' src/` returns nothing in phases 1–6. A CI
  check enforces it, so the deferral cannot erode quietly.
- The roadmap keeps Phase 10 as a visible placeholder, so this is a decision
  rather than an oversight.
- Revisit when the plugin count reaches ten, or when a user request for
  extensibility appears three times in issues. Whichever comes first.

