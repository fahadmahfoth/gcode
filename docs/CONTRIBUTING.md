# Contributing to gcode

Contributions are welcome, including ones written by an AI agent. This document
covers the mechanics. The technical design lives in
[ARCHITECTURE.md](ARCHITECTURE.md) and the build plan in [ROADMAP.md](ROADMAP.md).

**You do not need to write code or documentation by hand.** The OpenCode agents
configured in `.opencode/` can implement a phase, write the tests, and update
the docs from a single instruction. See [AI contributions](#ai-assisted-contributions)
below and `AGENTS.md` at the repository root.

---

## Quick start

```bash
git clone https://github.com/fahadmahfoth/gcode.git
cd gcode
rustup toolchain install            # rust-toolchain.toml pins the version
cargo build
cargo test
```

That is the whole setup. No API key, no model download, no cloud service, no
database. `cargo test` does not load a model — the test suite uses a fake
inference engine, which is why it finishes in under 20 seconds.

Optional, only if you need to test the real model path:

```bash
gcode --download-model
```

---

## Branches and commits

```
main              protected: needs a PR and green CI
├── develop       integration branch
├── feat/<name>
├── fix/<name>
├── docs/<name>
└── release/<version>
```

Commit format — [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <subject>

<body>

<footer>
```

| Type | Use for |
|---|---|
| `feat` | A new capability |
| `fix` | A bug fix |
| `perf` | A measurable performance change |
| `test` | Tests only |
| `docs` | Documentation only |
| `refactor` | No behaviour change |
| `build` | Dependencies, CI, packaging |
| `ci` | Workflow changes |
| `chore` | Housekeeping |

Scopes match the module map: `cli`, `config`, `context`, `inference`, `model`,
`safety`, `exec`, `ui`, `docs`, `ci`, `packaging`.

Examples:

```
feat(safety): add taint propagation check for write-then-delete chains
fix(shell): preserve $? in the zsh precmd hook
docs(models): document Q2_K refusal rationale
test(safety): add near-miss negatives for every root-delete pattern
```

---

## Pull requests

A PR is ready when:

- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` is clean
- [ ] `cargo test` passes
- [ ] New behaviour has a test; a bug fix has a regression test that fails
      without the fix
- [ ] Test coverage has not decreased
- [ ] `CHANGELOG.md` has an entry under `Unreleased`
- [ ] Documentation is updated if behaviour changed
- [ ] `cargo audit` is clean
- [ ] No secrets, no personal paths, no machine-specific config
- [ ] Snapshots are reviewed, not auto-accepted

### The one-PR rule for safety

**Any change to `src/safety/` needs a new row in the safety test matrix** in
[TESTING.md](TESTING.md), with both a positive and a negative case. A PR that
adds a pattern without a near-miss negative will not be merged. A classifier with
false positives destroys user trust faster than one with false negatives.

### Snapshots

```bash
cargo insta test      # report
cargo insta review    # accept, interactively
```

Commit the `.snap` files. A snapshot diff in CI is a failure on purpose —
prompt and output changes must be looked at by a human, not absorbed silently.

---

## Code style

Follow the surrounding code. Concretely:

| Rule | Reason |
|---|---|
| No `unwrap()` outside tests | A CLI should print an error, not panic |
| No `expect()` in library code | Same |
| Errors are `thiserror` types, contexts are `anyhow` | Distinguishes library from binary |
| No `println!` in `src/` except in `main.rs` and `ui/` | Keeps `--json` honest |
| No blocking calls in a `Drop` | Makes shutdown hang |
| Comments explain *why*, not *what* | The code says what |
| Public items have a doc comment | This generates the man page basis |
| `unsafe` requires a `// SAFETY:` comment | There is almost none; it should stay that way |

```rust
// Good
let entries = history::read_last(n)?;

// Bad
let entries = history::read_last(n).expect("history file is valid");
```

---

## Dependencies

Adding a dependency is a design decision, not a convenience. A PR that adds one
must justify:

1. Why the standard library is not enough
2. Why an existing dependency is not enough
3. Its maintenance status and licence
4. Its transitive dependency count
5. Its effect on binary size and build time

`cargo deny` enforces the licence allowlist. Prefer small, focused, actively
maintained crates. If a crate has had no commit in over a year, say so in the PR.

Current stack and the reason for each choice: [ARCHITECTURE.md](ARCHITECTURE.md#dependencies).

---

## Adding a model

Models live in `models/registry.toml` (Phase 1). To add one:

1. Confirm the licence permits redistribution and commercial use — record it.
2. Compute the checksum yourself:

   ```bash
   shasum -a 256 model.gguf
   ```

3. Add an entry with `name`, `url` (HTTPS), `sha256`, `size_bytes`,
   `context_size`, `license`.
4. Run `gcode --list-models` and confirm the size is right.
5. Benchmark it: `gcode --bench --model <name>`.
6. Add a line to [MODELS.md](MODELS.md) with measured, not estimated, numbers.

**Never commit a model file. Never copy a checksum from anywhere — compute it.**
A wrong checksum in a signed registry is either a broken release or a supply
chain attack waiting to happen.

---

## Documentation

- Docs are English. User-facing strings may be bilingual; the tool is used in
  both Arabic and English.
- Every new flag gets a row in [USAGE.md](USAGE.md).
- Every architectural change gets an ADR in [`adr/`](adr/).
- Never mark a feature as shipped in docs until it is in `CHANGELOG.md` for a
  released version. This is the rule that keeps the docs honest.

---

## AI-assisted contributions

Contributions written with an AI agent are welcome. Disclosure is not required
in the commit message, but the code must meet the same bar as any other
contribution: it must be understood, tested, and reviewable by the person
submitting it. An unreviewed agent dump is not a contribution.

This repository ships the agent setup it uses on itself, in `.opencode/`:

| Path | Role |
|---|---|
| `AGENTS.md` | The rules every agent follows in this repo |
| `opencode.json` | Project configuration, permissions, secret guardrails |
| `.opencode/agent/*.md` | Specialist agents: architect, rust engineer, safety auditor, docs, test, release, reviewer |
| `.opencode/command/*.md` | Slash commands: `/plan`, `/build`, `/test`, `/review`, `/docs`, `/status`, `/release` |
| `.opencode/skill/gcode-delivery/` | The end-to-end delivery workflow |
| `.opencode/STATE.md` | Which phase is in progress |

Using it:

```bash
cd /Users/fahad/gcode     # or your clone
opencode
```

Then:

```
/status                  # where are we
/plan implement Phase 1.5
/build
/test
/review
/docs
/release
```

The agents are configured to never write a credential, never read outside the
repository, and to leave anything requiring a key or an account marked 🔒 in
[ROADMAP.md](ROADMAP.md) for a human. That boundary is deliberate — see
[SECURITY.md](../SECURITY.md).

---

## Code of conduct

Be decent. Assume good faith, critique the code rather than the person, and
accept that a technical decision can be disagreed with without becoming a
conflict. Report unacceptable behaviour to the maintainers.

---

## Getting help

- Something unclear in the docs → open a docs issue
- A bug → [../.github/ISSUE_TEMPLATE/bug_report.md](../.github/ISSUE_TEMPLATE/bug_report.md),
  with `gcode doctor` output
- A security problem → [../SECURITY.md](../SECURITY.md), **not** a public issue
- A design debate → an issue, framed as a question with options

---

## Related

- [ARCHITECTURE.md](ARCHITECTURE.md) — how the code fits together
- [ROADMAP.md](ROADMAP.md) — what to work on next
- [TESTING.md](TESTING.md) — what the tests must cover
- [DECISIONS.md](DECISIONS.md) — why things are the way they are
- [../AGENTS.md](../AGENTS.md) — the rules the agent follows
