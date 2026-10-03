# Contributing to gcode

The full contributor guide lives at **[docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)**.
This file exists so that GitHub, the package registries, and anyone who lands on
the repository root find it immediately.

Start there. It covers the commit format, the test matrix, the documentation
rules, and the release process.

## The short version

1. Read [AGENTS.md](AGENTS.md). It is the operating contract for this repository.
2. Read [docs/ROADMAP.md](docs/ROADMAP.md). It is the build contract. Pick the
   first phase that is not ✅ and stay inside it.
3. Run the full gate before opening a pull request:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   ```

4. Open a pull request against `master`. Describe the change, the test that proves
   it, and what you deliberately did not do.

## What will be rejected

- A change to `src/safety/` that was not reviewed by the safety auditor
- A new dependency without a licence and maintenance check
- A documented feature that does not exist in the code
- An acceptance criterion ticked in `docs/ROADMAP.md` without the command that
  proves it
- A weakening of any safety invariant listed in `AGENTS.md` §3
- Any credential, key, or personal path in a committed file

## Security

Do not report a vulnerability in a public issue. See
[SECURITY.md](SECURITY.md).

## Licence

By contributing you agree that your work is licensed under the MIT licence in
[LICENSE](LICENSE). See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
