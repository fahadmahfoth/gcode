# gcode Documentation

Complete documentation set for **gcode** — the local-first, offline natural-language
to shell command generator.

Everything here is committed to the public repository. Nothing in it contains
credentials, tokens, private keys, or machine-specific paths.

---

## Read this first

| You are… | Read | Time |
|---|---|---|
| A new user, want to try it | [INSTALL.md](INSTALL.md) → [USAGE.md](USAGE.md) | 10 min |
| A user hitting a problem | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | 5 min |
| Worried about safety | [SAFETY.md](SAFETY.md) | 10 min |
| A power user / model tuner | [MODELS.md](MODELS.md) | 15 min |
| A contributor | [CONTRIBUTING.md](CONTRIBUTING.md) + [ARCHITECTURE.md](ARCHITECTURE.md) | 30 min |
| Building the project | [ROADMAP.md](ROADMAP.md) — the phase-by-phase build plan | 20 min |
| Cutting a release | [RELEASING.md](RELEASING.md) | 10 min |
| Curious about a technical choice | [DECISIONS.md](DECISIONS.md) + [`adr/`](adr/) | as needed |
| Writing or fixing tests | [TESTING.md](TESTING.md) | 10 min |

---

## Full document map

### Users

| Doc | Purpose |
|---|---|
| [INSTALL.md](INSTALL.md) | Install on Linux, macOS, Alpine, Windows-via-WSL; package managers; uninstall |
| [USAGE.md](USAGE.md) | Every flag, every mode, copy-paste recipes |
| [SAFETY.md](SAFETY.md) | Risk levels, classifier rules, confirmation flow, security model |
| [MODELS.md](MODELS.md) | Model registry, custom GGUF models, quantisation, performance tuning |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | Symptoms → causes → fixes; log locations; escape hatches |

### Contributors

| Doc | Purpose |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Module map, data flow, concurrency, extension points |
| [ROADMAP.md](ROADMAP.md) | Phases 0–6, task-by-task, with acceptance criteria |
| [TESTING.md](TESTING.md) | Test strategy, coverage targets, fixtures, benchmarks, matrix |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev setup, branch strategy, PR rules, commit format |
| [RELEASING.md](RELEASING.md) | Versioning, release checklist, signing, rollback |
| [DECISIONS.md](DECISIONS.md) | Human-readable index of architecture decisions |
| [`adr/`](adr/) | Architecture Decision Records, immutable, one per decision |

### Reference

| Doc | Purpose |
|---|---|
| [`gcode.1`](gcode.1) | `man gcode` page (roff) |
| [../CHANGELOG.md](../CHANGELOG.md) | Release history (Keep a Changelog format) |
| [../SECURITY.md](../SECURITY.md) | Vulnerability disclosure policy |
| [../AGENTS.md](../AGENTS.md) | Rules the OpenCode agent follows in this repo |

---

## Documentation conventions

- **Language**: English. Arabic user-facing strings are acceptable in code and
  examples (the tool is used bilingually) but docs stay English-first.
- **Tone**: direct, imperative, no marketing adjectives.
- **Every claim is checkable.** Commands in docs are commands that exist or are
  planned with a phase reference. Nothing aspirational is written as shipped.
- **Version markers**: features not yet implemented are marked
  `⛔ planned (Phase N)` so readers can tell shipped from planned.
- **Authorship footer**: none required. Documents may carry an attribution
  note if the author wants one, but `scripts/ci.sh` does not enforce it.

---

## The three truths in this repository

1. **`plan.md` is the vision.** It is aspirational and long-range. It is *not* a
   statement of what is implemented.
2. **`docs/ROADMAP.md` is the contract.** It lists what is actually being built,
   in order, with acceptance criteria that must be demonstrably true.
3. **`CHANGELOG.md` is the only record of what shipped.** If a feature is not in
   the changelog for a released version, it is not shipped.
