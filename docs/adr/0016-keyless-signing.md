# 0016. Keyless release signing

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

A user running `curl … | sh` executes a binary they have not verified. The only
meaningful defence is a signature they can check without trusting us — which
means the signature must be anchored in an identity the user already trusts.

Conventional release signing uses a long-lived private key: an upload token, a
GPG key, or an SSH key. Long-lived keys are the weakest link in a distribution
chain. They leak from laptops, from CI secret stores, from backups, and from
people. A leaked key means every artefact ever signed with it is untrusted, and
recovery means rotating it and asking every user to check for revocation.

For a project run by one or two people, a long-lived signing key is a
disproportionate risk for a modest benefit.

## Decision

Sign releases with Sigstore keyless OIDC. No long-lived signing secret exists in
this project.

1. Release artefacts are signed with [Sigstore keyless](https://github.com/sigstore/fulcio)
   using GitHub Actions OIDC. The signing certificate is short-lived and bound to
   the workflow run, the repository, and the commit.
2. Verification is done with `gh attestation verify` and `cosign verify-blob`,
   which consult the public Sigstore transparency log. No key distribution step
   for users.
3. Releases carry an SLSA provenance attestation describing how the artefact was
   built, including the commit and the workflow.
4. `checksums.txt` is published alongside the artefacts and referenced in the
   release notes.
5. The APT repository is the **exception**, and the reason is documented: APT
   requires a stable, long-lived GPG key with an expiry date, and there is no
   keyless option. That key is held by the maintainer 🔒, is backed up offline,
   and has a documented rotation procedure.
6. The key-rotation and artefact-revocation procedure is written down in
   [../RELEASING.md](../RELEASING.md) **and exercised once before 1.0**. An
   unexercised rotation procedure is not a procedure.

## Alternatives considered

**A long-lived GPG signing key in CI secrets.** Conventional, well understood,
and what most projects do. Rejected: it is the single most common source of
release compromise, and it puts a durable secret in a place that must be
protected forever. For this project the risk is not worth the familiarity.

**Not signing at all, publishing checksums only.** Checksums in the same place as
the artefact prove nothing, because an attacker who can replace the artefact can
replace the checksum. Rejected: it provides no protection while creating the
impression of some.

**A hardware security module (HSM) or a sigstore-backed long-lived key.** The
strongest traditional answer. Requires hardware, a vendor, and an operational
burden disproportionate to a project this size. Revisit if the project gains an
enterprise or paid dimension.

**Sign in CI with a key stored as an encrypted secret.** Better than plaintext.
Rejected: still a long-lived key, with the same compromise profile, plus the
complexity of secret management.

**Publish source only and let packagers build.** Maximum trust, no binary
distribution. Rejected: it defeats the thirty-second promise in
[0012](0012-one-liner-install.md) and outsources the build to someone else.

## Consequences

**Easier**

- No signing secret to store, rotate, back up, or accidentally leak. The class of
  compromise is eliminated rather than mitigated.
- Users verify with one command and no key management.
- A compromised CI runner cannot mint a valid signature for a different
  artefact, because the certificate is bound to the specific workflow run and
  commit.
- The transparency log means signature existence is publicly auditable.

**Harder**

- Verification requires network access to the transparency log, which is a
  tension with the offline-first positioning ([0001](0001-local-first-offline-inference.md)).
  Mitigated: a user can verify once at install time, or use the bundled
  certificate, and the tool itself never needs the network to run.
- The APT repository still has a long-lived GPG key, so the risk is reduced, not
  eliminated. This is stated plainly rather than glossed.
- Sigstore's own infrastructure becomes a dependency of the release path.
- Keyless verification tooling is newer and less familiar than `gpg --verify`, so
  the documentation has to explain it. This is a real onboarding cost for
  cautious users.

**Forecloses**

- Offline verification against a bundled key. Accepted.
- The ability to sign releases from a laptop outside CI. Accepted: signing in CI
  is the point.

## Validation

- Every release artefact has a signature and a provenance attestation, and CI
  fails the release if either is missing.
- `gh attestation verify` succeeds against a published artefact, and this is
  checked in `verify-release.sh` on every release.
- The transparency log entry is recorded in the release notes.
- The APT GPG key rotation procedure is documented and **has been tested** —
  including a full revoke-and-republish dry run — before 1.0.
- A dependency or CI change that could read a signing secret is reviewed
  explicitly, since the premise of this ADR is that no such secret exists.

