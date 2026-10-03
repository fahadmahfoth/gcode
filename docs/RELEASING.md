# Release Process

Releases are built and verified locally. Your job is to tag, to hold the signing
key, and to review the notes. The project does not use GitHub Actions
([ADR 0019](adr/0019-local-ci-not-github-actions.md)); the build, signing, and
publication steps below are run by hand until a replacement release pipeline is
decided.

---

## Versioning

Semantic versioning, with an honest reading of the pre-1.0 part.

| Version | Meaning |
|---|---|
| `0.x.y` | Anything may change; the CLI surface is not stable |
| `1.0.0` | The flags in [USAGE.md](USAGE.md) are stable for the `1.x` line |
| `1.y.0` | New features, backward compatible |
| `1.y.z` | Fixes only |
| `2.0.0` | A flag removed or a default changed |

`1.0.0` is released only when every acceptance criterion in phases 0–6 of
[ROADMAP.md](ROADMAP.md) is ✅.

| Cadence | Type |
|---|---|
| As needed | Patch |
| Monthly | Minor |
| Yearly, or on a breaking change | Major |

---

## The signing key 🔒

**The private signing key never enters this repository, and never enters a
committed file.**

- It lives in your password manager and, at most, in an offline encrypted backup.
- **The signing mechanism is currently undecided.** It used to be Sigstore
  keyless OIDC — GitHub's identity, no long-lived secret — but that depended on
  a hosted GitHub Actions runner, which this project no longer uses
  ([ADR 0019](adr/0019-local-ci-not-github-actions.md)). A replacement ADR must
  pick between a local Sigstore flow, a hardware key, or a self-hosted runner
  before any release is signed.
- The APT repository uses a GPG key that you hold personally. If that key is ever
  exposed, rotate it immediately; a leaked package-signing key is a full
  compromise of the distribution channel.
- Losing the key means losing the ability to update the APT repository
  gracefully. You are the recovery path. Back it up now, not later.

---

## Pre-release checklist

Everything below is a hard gate. If any item is unchecked, do not tag.

### Code

- [ ] `cargo test` green, coverage ≥ 85 %
- [ ] `cargo fmt --check` and `cargo clippy -D warnings` clean
- [ ] `cargo audit` reports zero unpatched advisories
- [ ] `./scripts/ci.sh` exits zero on the build host
- [ ] `master` is the branch being released, and is up to date

### Safety

- [ ] The safety test matrix in [TESTING.md](TESTING.md) passes in full
- [ ] Fuzzing has run: 10 000 inputs, zero panics
- [ ] No new CRITICAL pattern has a false positive in the corpus
- [ ] The blocklist has not been weakened

### Documentation

- [ ] `CHANGELOG.md` is complete for this version
- [ ] `README.md` examples still produce the output shown
- [ ] `docs/gcode.1` matches the real `--help` output
- [ ] `docs/USAGE.md` documents every flag in `--help`
- [ ] The docs link check passes
- [ ] No document claims a feature that is not in this build

### Security

- [ ] The secret scanner is clean
- [ ] No credential, token, or private path is in the tracked tree
- [ ] Provenance attestation is enabled
- [ ] SBOM is generated

### Build

- [ ] Release artefacts build for all six targets
- [ ] `scripts/verify-release.sh` passes on every artefact
- [ ] The man page and completions are generated and embedded

---

## Release procedure

### 1. Freeze

```bash
git checkout master
git pull --ff-only
cargo test && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
```

### 2. Update the changelog

Move `Unreleased` under the new version, with a date. Every entry must be
user-visible. Internal refactors do not go in the changelog; they go in the
commit log.

### 3. Bump the version

```bash
# Cargo.toml
version = "1.0.0"
```

`CHANGELOG.md`, `Cargo.toml`, and the tag must agree. The local gate checks this.

### 4. Tag and push

```bash
git tag -a v1.0.0 -m "gcode 1.0.0"
git push origin master --tags
```

Pushing the tag no longer triggers automation. Until a release pipeline is
decided ([ADR 0019](adr/0019-local-ci-not-github-actions.md)), the steps that a
workflow used to perform are run locally:

1. Builds the target you are releasing with `--locked`
2. Archives and installers are assembled by hand
3. Generates the man page and completions
4. Signs with the chosen mechanism (see *The signing key*, still undecided)
5. Emits `checksums.txt` and an SBOM
6. Attaches provenance if the signing mechanism provides it
7. Creates a release on the hosting service with notes drawn from `CHANGELOG.md`
8. Builds and publishes the Docker image
9. Opens the packaging PRs (Homebrew, AUR)

### 5. Verify the release

```bash
./scripts/verify-release.sh v1.0.0
```

Installs the artefact on a clean machine and runs the smoke test. On a clean
container or VM, not your laptop — your laptop has state that hides bugs.

### 6. Publish downstream

| Channel | Automated? | Notes |
|---|---|---|
| GitHub Releases | No | Created by hand on tag; no runner (ADR 0019) |
| Docker Hub / GHCR | No | Built and pushed by hand |
| Homebrew | PR opened | You merge it |
| AUR | PR opened | You merge it |
| APT repo | Manual 🔒 | Needs your GPG key |
| COPR | Manual 🔒 | Needs your account |

### 7. Announce

The agent drafts the announcement text. You post it. Accounts are yours.

| Channel | Angle |
|---|---|
| Hacker News | The design decisions, the latency budget, what you got wrong |
| r/commandline | The tool, the install one-liner, the demo |
| Reddit r/linux, r/bash | Same, adapted per subreddit's rules |
| Lobsters | The technical content |
| A blog | The full story — safety model, benchmark methodology, roadmap |
| awesome-cli-apps, awesome-selfhosted | Submissions after the dust settles |

Announce once, well. Do not spam.

---

## Post-release

- [ ] Install from the release on a **clean** machine, not an upgrade
- [ ] Confirm the one-line installer works end to end
- [ ] Confirm `apt install gcode` and `brew install gcode`
- [ ] Watch the first 24 hours of issues
- [ ] Triage: bug, question, or duplicate
- [ ] Anything that breaks a documented promise is a P0 and gets a patch release
- [ ] Bump the roadmap status in [ROADMAP.md](ROADMAP.md)

---

## Rollback

| Situation | Response |
|---|---|
| A broken release | `git tag -d v1.0.1`, publish `v1.0.2` fast. Never re-tag a published version. |
| A bad package in APT | Remove the `.deb` from `pool/`, bump the repo version, push a fixed one |
| A broken model in the registry | Revert the `models/registry.toml` change. Users on `--locked` config keep the old one. |
| A security disclosure | Follow [../SECURITY.md](../SECURITY.md). Advisory, coordinated fix, patched release. |
| A compromised signing key | **Full stop.** Revoke, rotate, notify every downstream consumer, treat every artefact signed with it as untrusted. |

Rolling back is a normal operation, not a disaster. Shipping a broken release
and quietly fixing it without saying so is the disaster.

---

## Release notes template

```markdown
## gcode 1.0.0 — 2026-XX-XX

First stable release. The flags documented in docs/USAGE.md are now stable for
the 1.x line.

### Added
- Natural-language to shell command generation, fully offline
- Session-history context, with secret redaction before prompt assembly
- Five-level risk classifier with an unrunnable CRITICAL tier
- bash and zsh integration via `gcode --init`
- `--fix` for failed commands, `--complete`, `--explain`
- One-line installer, and packages for Debian, Fedora, Arch, Alpine, macOS, Nix

### Performance
- p50 inference: <1s on 2 physical cores, no GPU
- Peak RSS: <1.5 GB
- Cold start: <500 ms

### Security
- 100% open source, MIT
- No telemetry, no network access after the model download
- CRITICAL-classified commands are unrunnable by any flag

### Known limitations
- English and Arabic input only
- bash and zsh only; fish and nushell are not supported
- Linux and macOS only; Windows requires WSL2
- The default 0.5B model needs occasional correction
```

---

## Related

- [ROADMAP.md](ROADMAP.md) — the definition of done for 1.0.0
- [CHANGELOG.md](../CHANGELOG.md) — the release history
- [SECURITY.md](../SECURITY.md) — disclosure and the rotation procedure
- [ADR 0019](adr/0019-local-ci-not-github-actions.md) — why there is no
  release workflow to link to
