# Changelog

All notable changes to this project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with an honest reading of the pre-1.0 part: anything may change before `1.0.0`.

**A feature is not shipped until it appears here under a released version.**
This rule is what keeps the documentation honest — see
[docs/README.md § The three truths](docs/README.md#the-three-truths-in-this-repository).

---

## [Unreleased]

### Added

- Complete documentation set: install, usage, architecture, safety, models,
  testing, troubleshooting, contributing, releasing, decisions, and a man page
- 16 architecture decision records under `docs/adr/`
- Build roadmap with 11 phases and per-phase acceptance criteria
- OpenCode agent tooling in `.opencode/` — 7 specialist agents, 8 commands, and
  a delivery skill — plus `AGENTS.md` and `opencode.json`
- Project rules for the agent: no secrets, no writes outside the repository, no
  commits without review
- GitHub issue, pull request, and config templates
- A release checklist at `docs/RELEASE_CHECKLIST.md`
- CI, release, and nightly workflows. **Written, not yet executed** — there is
  no Rust project for them to run against.

### Changed

- Nothing yet. The code has not been written.

---

## [0.1.0] — Unreleased

Not started. This version will hold the Rust foundation described in Phase 0 of
[docs/ROADMAP.md](docs/ROADMAP.md). The commands shown in the README are the
target interface, not shipped features.

---

## Roadmap summary

Full detail in [docs/ROADMAP.md](docs/ROADMAP.md).

| Phase | Name | Status |
|---|---|---|
| 0 | Repository foundation | 🟡 docs and tooling done, code not started |
| 1 | Core inference pipeline | ⬜ |
| 2 | Shell integration & history | ⬜ |
| 3 | Safety layer | ⬜ |
| 4 | Packaging | ⬜ |
| 5 | Distribution & CI/CD | ⬜ |
| 6 | Polish & launch | ⬜ |
| 7 | Reliability hardening | ⛔ planned |
| 8 | Shell coverage: fish, nushell | ⛔ planned |
| 9 | Agentic multi-step execution | ⛔ planned |
| 10 | WASM plugin system | ⛔ planned |

---

## Planned for 1.0.0

Not implemented. Listed so the intent is on record.

- Natural language to shell command generation, entirely offline
- Session-history context, with secret redaction before prompt assembly
- Five-level risk classifier with an unrunnable `CRITICAL` tier
- bash and zsh integration via `gcode --init`
- `gcode --fix` for failed commands, `--complete`, `--explain`
- One-line installer, plus packages for Debian, Fedora, Arch, Alpine, macOS, Nix
- Sub-second inference on a two-core CPU, under 1.5 GB RSS

---

## Version history

| Version | Date | Highlights |
|---|---|---|
| 0.1.0 | — | Not started. Documentation and tooling so far. |
| 1.0.0 | — | Planned |

---

## Authoring

Every entry corresponds to a merged change, and the claims here are verifiable
against the repository history.
