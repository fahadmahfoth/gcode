# 0019. Local CI, not GitHub Actions

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Amends**: [0009](0009-crates-io-dependencies.md), [0015](0015-cargo-dist-over-distro-packaging.md), [0016](0016-keyless-signing.md) — the parts that assume GitHub Actions

## Context

Every continuous-integration and release decision in this repository was written
against GitHub Actions. [0009](0009-crates-io-dependencies.md) runs `cargo
audit` and `cargo deny` there, [0015](0015-cargo-dist-over-distro-packaging.md)
builds and publishes releases there, and
[0016](0016-keyless-signing.md) signs them with GitHub's OIDC identity.

The account that owns this repository is locked for a billing reason, and GitHub
refuses to start any job: *"The job was not started because your account is locked
due to a billing issue."* A CI that cannot run is not a CI, and a workflow file
that has never executed is not evidence. The maintainer's decision is to stop
depending on GitHub Actions at all.

## Decision

**CI is a script that runs on the machine you are on.**
`scripts/ci.sh` is the gate. It runs format, clippy, the test suite, the debug
build, the policy greps from the removed workflow (the classifier purity check,
the plugin-deferral check, the network-containment check), the secret scan, the
man-page lint, and the documentation link check, and it reports every result
rather than stopping at the first failure. `scripts/check-doc-links.py` is the
extracted link checker.

1. The GitHub Actions workflows are removed from the repository.
2. "CI is green" means `./scripts/ci.sh` exits zero, and that is what a change is
   required to demonstrate before it is merged.
3. A check that needs privileges, a second operating system, or a tool that is
   not installed is **skipped with a printed reason**, never reported as passing.
4. The parts of [0015](0015-cargo-dist-over-distro-packaging.md) and
   [0016](0016-keyless-signing.md) that assume a hosted CI runner are **not
   satisfied by this ADR and are now open**. In particular:
   - keyless signing through GitHub's OIDC issuer is unavailable, so the signing
     mechanism has to be chosen again;
   - a hosted build matrix across six platforms no longer exists, so
     cross-platform releases become a local or self-hosted task;
   - the apt repository, which was published from GitHub Pages, has no home yet.
   This ADR records the gap rather than hiding it. A follow-up ADR decides the
   replacement.

## Alternatives considered

**Keep the workflows and wait for the account.** No change to the repository, and
the design stays as written. Rejected: the wait has no end date the maintainer
controls, and the project cannot verify itself in the meantime. The workflows
remain recoverable from git history if GitHub Actions is ever used again.

**A different hosted CI (GitLab, Buildkite, a self-hosted runner).** Keeps the
automation and the OIDC-style signing. Rejected for now: it moves the dependency
rather than removing it, and no account or runner exists yet. It remains the
likely answer for the release pipeline specifically, and is what the follow-up
ADR should weigh.

**Run only on a developer machine and call it "tested locally".** The status quo
minus the workflow files. Rejected: it is exactly the failure mode where a check
that was never run is believed to have passed. `scripts/ci.sh` exists so the
claim is one command and its output.

## Consequences

**Easier**

- The gate runs offline, immediately, on the machine where the change was made.
- No billing account, no runner minutes, no YAML that has never executed.
- The checks are the same commands a contributor already runs by hand, in one
  place, so they cannot drift from what CI "would" have done.

**Harder**

- There is no longer a second operating system in the loop. The Linux x86_64
  build remains unverified until someone runs `scripts/ci.sh` on Linux or a
  self-hosted runner is added.
- Releases, signing, provenance, and the package repositories have no automation.
  They are blocked, visibly, rather than silently assumed.
- Secret scanning is a `grep` backstop, not gitleaks over full history. Weaker,
  and stated here so it is not mistaken for the stronger check.

## Validation

- `scripts/ci.sh` is the acceptance criterion for Phase 0's "CI runs green"; the
  row changes from "a workflow file exists" to "this script exits zero".
- The removed workflow's portable checks were moved into the script verbatim
  where possible, so the policy greps did not disappear with the YAML.
- The skips are printed, so a green run cannot hide a check that did not run.
