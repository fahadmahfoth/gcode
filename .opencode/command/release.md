---
description: Prepare a gcode release — verify, build artefacts, and stop before publishing
agent: gcode-release
---

Prepare a gcode release. **Never publish.** The final step is the human's.

## Arguments

`$ARGUMENTS` may be a version (`1.0.0`, `v1.0.0`) or empty, in which case read
the version from `Cargo.toml`.

## Do this

### 1. Run the full pre-release gate

From `docs/RELEASING.md`. Every item is a hard gate. If any fails, stop and
report which.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo llvm-cov --fail-under-lines 85    # when available
cargo audit                              # when available
cargo deny                               # when available
```

The safety gate:

```bash
cargo test --test safety
cargo fuzz run fuzz_classify -- -max_total_time=300   # 10k inputs, 0 panics
```

The supply-chain gate:

```bash
grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"']{8,}' \
  --exclude-dir=target --exclude-dir=.git . || echo "clean"
```

**If the secret scan fires, stop everything.** Report it. Do not commit, do not
tag, do not continue. Rotation comes first.

### 2. Check the documentation is true

- `docs/gcode.1` matches the real `--help` output
- `docs/USAGE.md` documents every flag
- No document claims a feature that is not in this build
- `CHANGELOG.md` is complete for this version
- Every acceptance criterion in phases 0–6 of `docs/ROADMAP.md` is ✅

### 3. Build the artefacts

```bash
cargo dist --artifacts=local
./scripts/verify-release.sh <version>
```

Verify the artefact list is complete: six binaries, `checksums.txt`, the man
page, both completion files.

### 4. Bump the version

`Cargo.toml`, `CHANGELOG.md`, and the tag must agree. Update the changelog: move
`Unreleased` under the new version with a date, user-visible entries only.

### 5. Stop here

Present the release to the human:

```
Version:   <v>
Artefacts: <list, with sizes>
Signed:    keyless Sigstore OIDC — provenance attached
Verified:  <what verify-release.sh proved, and where>
Ready:     yes / no, with the blocking item

Needs you — 🔒 every one of these:
  [ ] git tag -a <v> -m "..." && git push origin master --tags
  [ ] Merge the Homebrew formula PR
  [ ] Merge the AUR PR
  [ ] Publish the APT repository (your GPG key)
  [ ] Publish to COPR (your account)
  [ ] Post the announcement (your accounts)
```

## Never

- **Tag or push.** Publishing is the human's decision 🔒.
- **Introduce a long-lived signing secret.** Releases are keyless, per
  `docs/adr/0016-keyless-signing.md`.
- **Handle a signing key.** It is the maintainer's. Document its handling; never
  touch the key.
- **Re-tag a published version.** If a release is broken, publish a new one.
- **Skip the checksum verification** because it is inconvenient.
- **Ship a release where a gate failed.** Report the failure instead.

## Report

```
Did:        <version bumped, artefacts built, changelog updated>
Verified:   <each gate, with its real result>
Not done:   <what blocks the release>
Needs you:  <the 🔒 checklist above>
```

If the release is not ready, lead with why. A blocked release reported clearly
is more useful than a rushed one.
