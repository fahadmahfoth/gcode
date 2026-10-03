# 0025. Accept MPL-2.0 and CDLA-Permissive-2.0 in the dependency tree

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Relates to**: [0009](0009-crates-io-dependencies.md), [0021](0021-http-client-behind-a-feature.md)

## Context

[ADR 0009](0009-crates-io-dependencies.md) lists the licences `cargo deny` allows:
MIT, Apache-2.0, BSD, ISC, Unicode. Adding `deny.toml` showed two licences already
in the tree that the list does not name:

| Licence | Crate | Reached through | Always built |
|---|---|---|---|
| MPL-2.0 | `option-ext` | `dirs` → `dirs-sys` | yes |
| CDLA-Permissive-2.0 | `webpki-roots` | `ureq` (feature `download`) | no |

## Decision

Allow both in `deny.toml`.

- **MPL-2.0** is file-level copyleft. It obliges anyone who modifies `option-ext`
  itself to share those modifications. It does not reach gcode's own source, and
  gcode does not modify the crate, so MIT distribution of the binary is unaffected.
- **CDLA-Permissive-2.0** is a permissive licence for data (the root certificate
  list). It carries an attribution requirement, not a copyleft one.

Any other licence outside the ADR 0009 list still fails `cargo deny`.

## Alternatives considered

**Replace `dirs` with a few lines in `utils::paths`.** Removes the MPL crate. Not
done: `dirs` is small, widely used, and path logic is where portability bugs live.
Revisit if a distribution refuses MPL.

**Drop the `download` feature or `ureq`.** Removes CDLA. Rejected: the downloader
is the one network feature the project has (ADR 0001, ADR 0021).

## Consequences

- `deny.toml` carries the two licences with a pointer to this ADR.
- A packager who needs a licence-pure build can build without `download`, which
  drops `webpki-roots`.
- This is an extension of ADR 0009's list, not a reversal of it.
