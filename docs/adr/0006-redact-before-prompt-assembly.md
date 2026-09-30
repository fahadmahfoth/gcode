# 0006. Redact secrets before prompt assembly

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

gcode's prompt includes recent shell history **including output tails**. Output
is where credentials live:

```
$ export AWS_SECRET_ACCESS_KEY=<REDACTED-40-CHARS>
$ curl -H "Authorization: Bearer <REDACTED-JWT>" https://api.example.com
$ cat ~/.aws/credentials
$ psql postgres://<user>:<password>@<host>/<database>
$ cat id_rsa
```

Every value above is an inert placeholder. A real transcript would not be
safe to quote, so this ADR deliberately shows the shape and not the secret.

These strings reach the model. In the current design the model is local, so this
is not yet an exfiltration path — but that is a property of one deployment
configuration, not a property of the code. Anyone who adds an optional remote
backend, forks this, or reuses `context::prompt` in another tool inherits the
problem.

The classification argument is the stronger one. A secret embedded in the prompt
is a high-entropy token sitting in a high-entropy context. It degrades inference
quality, it can end up in `--json` output, and it can end up in a log file.

## Decision

Redact sensitive strings **before** they are assembled into a prompt, in one
place, as the last step of context construction.

1. `context::prompt::redact()` is the single redaction entry point and is called
   on every history output tail and every environment-derived string.
2. Patterns, at minimum: `*_TOKEN`, `*_SECRET`, `*_KEY`, `*_PASSWORD`,
   `AWS_*`, `GITHUB_TOKEN`, `Authorization:`, `Bearer <token>`, PEM
   `BEGIN ... PRIVATE KEY` blocks, and long high-entropy base64/hex runs.
3. Redaction is applied to the **history output tail** and env strings, not to
   the user's literal command text. Stripping the command would break the whole
   feature: `export TOKEN=...` is exactly the context `--fix` needs.
4. Output tails are truncated to the last 2 KB *before* redaction, so the
   redaction surface is bounded.
5. Redaction is a hard failure, not a warning: if `redact()` returns text that
   matches a secret pattern, the entry is dropped from the prompt entirely.
6. The redaction list is user-extensible in config, but the built-in patterns
   cannot be removed.

## Alternatives considered

**Redact after the model has seen it.** Useless. Once a token is in the prompt
it has been processed; the point is that it never enters.

**Rely on the grammar and classifier.** They bound what the model can *emit*,
not what it can *read*. Neither prevents a secret from influencing output, and
neither stops the secret reaching a log.

**Rely on the local-only guarantee.** Valid today, and this is the honest
counter-argument: with a local model, nothing is exfiltrated. Rejected as the
*only* control, because the guarantee is one architectural decision away from
being false, and because secrets still leak into logs and `--json` output.

**Strip history output entirely.** Maximum safety, and it kills the feature.
`--fix` is built on reading the error message. Rejected.

**Ask the user each time.** `gcode: this history contains something that looks
like a credential. Include it?` Technically correct and unusable at 15 entries
per invocation.

**Hash the secrets so the model still sees "a credential was here".** Clever, and
rejected: it adds complexity to a function that is the security boundary, and the
benefit is marginal because a hash of a secret is often itself sensitive.

## Consequences

**Easier**

- The privacy story in the README is checkable: there is one function to read.
- The `--json` output and the debug log cannot contain a redacted secret,
  because redaction happens upstream of both.
- Adding a remote backend later does not require an audit of the context path.
- Redaction shrinks the prompt, which is a small performance win.

**Harder**

- False positives. `KEY=value` in a config dump becomes `KEY=[REDACTED]`, which
  may remove context a user wanted. Accepted: over-redaction is safe,
  under-redaction is not.
- Regex-based redaction is inherently incomplete. An unusual secret format, or a
  secret with no recognisable prefix, will pass through. This is a mitigation,
  not a guarantee, and the docs say so.
- Entropy-based detection needs a threshold, and thresholds are tuneable, which
  means the behaviour is not perfectly deterministic across versions. The
  built-in prefix patterns are deterministic; the entropy pass is best-effort and
  documented as such.
- Every new pattern needs a test, and a test needs a secret-shaped value. Those
  fixtures are synthetic (`sk-` plus 32 obviously fake characters) and the secret
  scanner checks them.

**Forecloses**

- Very long prompts built from large outputs. The 2 KB tail cap is a hard
  constraint now.
- Any feature that wants raw output verbatim in the prompt. If that is ever
  needed, it needs a redaction bypass that is itself audited — and probably
  should not be built.

## Validation

- Test: `sk-` plus 32 alphanumerics in history is absent from the rendered prompt.
- Test: `Authorization: Bearer …` is absent.
- Test: a PEM `BEGIN RSA PRIVATE KEY` block is absent.
- Test: `AWS_SECRET_ACCESS_KEY=` and `GITHUB_TOKEN=` are absent.
- Test: the user's literal command text **is** preserved, so `--fix` still works.
- Snapshot: the five canonical prompts contain no unredacted high-entropy string,
  asserted by a test that scans every snapshot for the secret patterns.
- CI scans all fixtures for credential-shaped strings, so a test cannot
  accidentally commit a real secret.

