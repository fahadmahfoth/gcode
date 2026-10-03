# Release checklist

> Copy this into a release tracking issue. The
> authoritative process is [RELEASING.md](RELEASING.md).

## 1. Code

- [ ] `cargo test` green, coverage ≥ 85 %
- [ ] `cargo fmt --check` and `cargo clippy -D warnings` clean
- [ ] `cargo audit` reports zero unpatched advisories
- [ ] `cargo deny` passes the licence allowlist
- [ ] The full OS matrix is green

## 2. Safety

- [ ] The safety test matrix passes in full
- [ ] Classifier coverage is 100 %
- [ ] Fuzzing: 10 000 inputs, zero panics
- [ ] No new false positive in the safety corpus
- [ ] The blocklist has not been weakened
- [ ] `gcode-safety-auditor` found no HIGH or CRITICAL issue

## 3. Documentation

- [ ] `CHANGELOG.md` is complete for this version
- [ ] README examples still produce the output shown
- [ ] `docs/gcode.1` matches the real `--help` output
- [ ] `docs/USAGE.md` documents every flag
- [ ] Docs link check passes
- [ ] No document claims an unshipped feature

## 4. Security

- [ ] Secret scan is clean
- [ ] No credential, key, or private path in the tracked tree
- [ ] Provenance attestation is enabled
- [ ] An SBOM is generated
- [ ] The key-rotation procedure has been exercised

## 5. Build

- [ ] Artefacts build for all six targets
- [ ] `scripts/verify-release.sh` passes on every artefact
- [ ] The man page and both completion files are generated
- [ ] Checksums are present and complete

## 6. Publish 🔒 — maintainer only

- [ ] `git tag -a v<version> -m "..."` and `git push origin master --tags`
- [ ] GitHub Release created
- [ ] Homebrew formula PR merged
- [ ] AUR PR merged
- [ ] APT repository published (your GPG key)
- [ ] COPR published
- [ ] Container image pushed
- [ ] Installed from the release on a **clean** machine, not an upgrade
- [ ] Launch posts made from your accounts

## 7. After

- [ ] Watch the first 24 hours of issues
- [ ] Triage: bug, question, or duplicate
- [ ] Anything breaking a documented promise is a P0, and gets a patch release
- [ ] Update phase status in `docs/ROADMAP.md`
