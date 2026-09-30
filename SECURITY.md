# Security Policy

## Reporting a vulnerability

**Do not open a public issue for a security problem.**

| | |
|---|---|
| **Contact** | The maintainer via the email address on the [GitHub profile](https://github.com/fahadmf), or GitHub's [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories) if enabled |
| **Response** | Acknowledgement within 48 hours |
| **Assessment** | Severity and fix plan within 7 days |
| **Disclosure window** | 90 days from the report, coordinated with you |
| **Credit** | As you prefer: named, credited, or anonymous |

If you have already disclosed publicly, that is not a disqualification. Say so
plainly when you report, and we will work with you on timing.

---

## What counts as a vulnerability

### Critical

Findings that let a command classified `CRITICAL` execute anyway, or that let an
attacker make gcode execute a specific command of their choosing.

Examples:

- A path from the editor, from `--json`, or from a config value to execution that
  skips the classifier
- A `CRITICAL` override reachable through any flag, config key, or env var
  (see [ADR 0008](docs/adr/0008-critically-unrunnable.md))
- A classifier bypass: an input that normalises into a destructive command but
  is classified `SAFE`
- A grammar-constrained decoding failure that silently permits arbitrary
  unconstrained output on a path that assumes the grammar applied

### High

- A secret reaching a model prompt despite redaction
- A model file loading without checksum verification
- Command injection through history, cwd, or git context
- Local privilege escalation or arbitrary file write
- A supply-chain compromise in the release path

### Medium

- History file readable by other users (mode above `0600`)
- A hook that breaks a user's shell in a way that loses data
- A DoS that requires the user to run a specific command
- Sensitive information leaking into logs

### Low / Not a vulnerability

- The model generating a wrong but correctly-classified command
- Model accuracy complaints
- A user running a dangerous command gcode correctly warned about
- Missing hardening with no demonstrated impact
- Vulnerabilities in a dependency with no reachable path from gcode
- Anything requiring an attacker to already control the user's account
- Findings from automated scanners with no working proof of concept

**A proof of concept is not required to report.** A clear description of the
problem is enough to start.

---

## Security model in brief

The full model is in [docs/SAFETY.md](docs/SAFETY.md). The load-bearing
properties, each of which has an ADR and a test suite:

| Property | Enforced by | ADR |
|---|---|---|
| Nothing runs without consent | Confirmation prompt, fail-closed on non-interactive stdin | — |
| Classification is model-independent | `src/safety/` is pure, no model, no I/O | [0004](docs/adr/0004-independent-risk-classifier.md) |
| Invalid shell is unreachable | Grammar-constrained decoding | [0003](docs/adr/0003-grammar-constrained-decoding.md) |
| `CRITICAL` is unrunnable | Blocklist, no override path | [0008](docs/adr/0008-critically-unrunnable.md) |
| Secrets never enter a prompt | Redaction before prompt assembly | [0006](docs/adr/0006-redact-before-prompt-assembly.md) |
| Model files are verified | SHA256 pinned in the signed registry | [0010](docs/adr/0010-embed-the-model-registry.md) |
| Releases are verifiable | Keyless signing and SLSA provenance | [0016](docs/adr/0016-keyless-signing.md) |

### Known limitations

Stated here so they are not mistaken for oversights:

- **Redaction is a mitigation, not a guarantee.** A secret in a format gcode does
  not recognise will pass through. The real protection is that the model is
  local.
- **Prompt injection is bounded, not prevented.** Grammar constraints and
  independent classification bound the damage; they do not eliminate it.
- **A replaced local binary is a full compromise.** Anyone who can replace the
  binary owns the shell. Verify signatures.
- **The classifier cannot judge intent.** `rm -rf ./build` in the wrong directory
  is `SAFE` and is still a bad idea.
- **`cargo audit` cannot fully vet build scripts.** Third-party build scripts run
  during the build.

---

## Verifying a release

```bash
gh attestation verify gcode-x86_64-apple-darwin.tar.gz --repo fahadmf/gcode
cosign verify-blob gcode-x86_64-apple-darwin.tar.gz \
  --certificate-identity-regexp 'https://github.com/fahadmf/gcode/.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Or, in one step:

```bash
./scripts/verify-release.sh v1.0.0
```

Verification consults the public Sigstore transparency log, so it requires
network access once. The tool itself does not.

---

## Security measures in place

| Measure | Where |
|---|---|
| `cargo audit` on every push | `.github/workflows/ci.yml` |
| `cargo deny` licence and advisory policy | `.github/workflows/ci.yml` |
| `cargo vet` audited dependency list | `.github/workflows/ci.yml` |
| Secret scanning | `.github/workflows/ci.yml`, gitleaks |
| Dependency review on every PR | `.github/workflows/ci.yml` |
| Keyless release signing | `.github/workflows/release.yml` |
| SLSA provenance attestation | `.github/workflows/release.yml` |
| Classifier fuzzing | `cargo fuzz run fuzz_classify`, nightly |
| 100 % coverage gate on the classifier | CI, [docs/TESTING.md](docs/TESTING.md) |
| History file mode `0600` | `context::history`, tested |
| Network only in the model downloader | CI check, [ADR 0001](docs/adr/0001-local-first-offline-inference.md) |

---

## Security-relevant repository policy

This repository is public. Therefore:

1. **No secrets, ever.** No API keys, tokens, passwords, private keys, or
   connection strings in any tracked file, including documentation, examples,
   test fixtures, and comments.
2. **Test fixtures use synthetic secret-shaped values.** `sk-` followed by
   obviously fake characters. Never a real credential, not even an expired one.
3. **No personal paths.** Examples use `/home/user`, not anyone's real home
   directory.
4. **The signing key is never in this repository.** It lives in the maintainer's
   custody 🔒. Key *handling* is documented; key *material* is not.
5. **`.env` is gitignored. `.env.example` contains only variable names**, with
   empty or placeholder values.
6. **The OpenCode agents in `.opencode/` are configured to refuse** to write
   credential-shaped values, and CI scans every commit for them.

If a secret is ever committed:

1. **Rotate it first.** Removing it from git history does not un-leak it; the
   object may already have been cloned.
2. Then purge it from history (`git filter-repo`, force-push, and ask everyone to
   re-clone).
3. Then add it to the scanner's ignore list with a comment explaining why.

Rotation before cleanup. Always.

---

## Supported versions

Security fixes are provided for the current minor release. gcode is pre-1.0, so
there is no long-term-support branch. When 1.0 ships, this section gains an LTS
policy.

---

## Security contact

Do not use a public issue. Use the private channels at the top of this document.
