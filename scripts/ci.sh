#!/usr/bin/env bash
#
# The project's CI gate, run locally.
#
# This repository does not use GitHub Actions (ADR 0019). Every check that can
# run on the machine you are on runs here, in one command, so "CI is green" means
# "this script passed", not "a workflow that has never executed looks correct".
#
#   ./scripts/ci.sh
#
# Exit status is the number of failed checks. `set -e` is deliberately not used
# at the top level: the point is to run every check and report all failures, not
# to stop at the first, which is what a contributor would otherwise get and have
# to fix one at a time.
#
# Checks that need privileges, a second operating system, or a tool that is not
# installed are skipped with a printed reason rather than reported as passing.
#
# With `--strict` (or CI_STRICT=1) a skip is a failure, and the shell hook tests
# refuse to pass without bash and zsh installed. A gate that passes because it
# could not run its checks is not a gate; use this mode where the toolchain is
# meant to be complete.

set -uo pipefail

cd "$(dirname "$0")/.."

STRICT=0
if [ "${1:-}" = "--strict" ] || [ "${CI_STRICT:-}" = "1" ]; then
    STRICT=1
    export GCODE_REQUIRE_SHELLS=1
fi

FAILED=0
SKIPPED=0

pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILED=$((FAILED + 1)); }
skip() {
    if [ "$STRICT" = "1" ]; then
        printf '  FAIL  %s (%s; --strict)\n' "$1" "$2"
        FAILED=$((FAILED + 1))
    else
        printf '  skip  %s (%s)\n' "$1" "$2"
        SKIPPED=$((SKIPPED + 1))
    fi
}
section() { printf '\n== %s\n' "$1"; }

has() { command -v "$1" >/dev/null 2>&1; }

if has cargo; then
    section "format"
    if cargo fmt --all -- --check; then pass "cargo fmt --check"; else fail "cargo fmt --check"; fi

    section "lint"
    if cargo clippy --all-targets --all-features -- -D warnings; then
        pass "cargo clippy -D warnings"
    else
        fail "cargo clippy -D warnings"
    fi

    section "test"
    if cargo test --all-features --locked; then pass "cargo test"; else fail "cargo test"; fi

    section "build (debug)"
    if cargo build --locked; then pass "cargo build --locked"; else fail "cargo build --locked"; fi

    section "build (release)"
    # A release build is refused while any registry entry is `verified = false`
    # (ADR 0018 / build.rs). That is the intended state until the model has been
    # downloaded and hashed once, so the failure is reported as a skip, not a bug.
    if cargo build --release --locked >/tmp/gcode-release-build.log 2>&1; then
        pass "cargo build --release --locked"
    elif grep -q "is not \`verified\`" /tmp/gcode-release-build.log; then
        skip "cargo build --release --locked" "a registry entry is not verified yet; see docs/MODELS.md"
    else
        fail "cargo build --release --locked (see /tmp/gcode-release-build.log)"
    fi
else
    skip "cargo checks" "cargo is not on PATH"
fi

section "policy greps"
# Comment lines are ignored: a doc comment that *says* src/safety/ never imports
# the model is not an import. The original workflow grep did not filter these and
# would have failed on the first real run.
noncomment() { grep -vE ':[0-9]+:[[:space:]]*(//|/\*|\*)' || true; }

# ADR 0004: the classifier stays a pure function with no model dependency.
SAFETY_HITS=$(grep -rInE 'llama|inference|engine::|crate::inference' src/safety/ 2>/dev/null | noncomment || true)
if [ -n "$SAFETY_HITS" ]; then
    printf '%s\n' "$SAFETY_HITS" | sed 's/^/        /'
    fail "src/safety/ must not depend on inference"
else
    pass "src/safety/ is pure"
fi

# ADR 0011: no plugin system before v1.0.
PLUGIN_HITS=$(grep -rInE 'wasm|dylib|dlopen|Lua|plugin_api' src/ 2>/dev/null | noncomment || true)
if [ -n "$PLUGIN_HITS" ]; then
    printf '%s\n' "$PLUGIN_HITS" | sed 's/^/        /'
    fail "plugin infrastructure appeared before v1.0"
else
    pass "no plugin infrastructure"
fi

# ADR 0001: network access is confined to the model downloader.
ALLOWED='src/model/download.rs'
NET=$(grep -rInE 'reqwest|ureq|hyper::client|TcpStream' src/ 2>/dev/null | noncomment | grep -v "^${ALLOWED}:" || true)
if [ -n "$NET" ]; then
    printf '%s\n' "$NET" | sed 's/^/        /'
    fail "network code outside ${ALLOWED}"
else
    pass "network access confined to the model downloader"
fi

section "secrets"
if has git && git ls-files | grep -E '\.(pem|key|asc|gpg|p12|pfx)$' >/dev/null 2>&1; then
    fail "key material is tracked"
else
    pass "no key material tracked"
fi
if has git && git ls-files | grep -E '\.(gguf|bin)$' >/dev/null 2>&1; then
    fail "a model file is tracked"
else
    pass "no model files tracked"
fi
# The backstop from AGENTS.md § 8. Two reviewed false positives exist: a doc line
# and a test comment that both contain the literal `grep -r 'token=' src/`. Only
# those are allowed; any other match fails the gate.
CREDS=$(grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"']{8,}' \
    --exclude-dir=target --exclude-dir=.git . 2>/dev/null || true)
UNREVIEWED=$(printf '%s\n' "$CREDS" | grep -v "grep -r 'token=' src/" || true)
if [ -n "$UNREVIEWED" ]; then
    printf '%s\n' "$UNREVIEWED" | sed 's/^/        /'
    fail "credential scan found a string that is not a reviewed false positive"
else
    pass "credential scan (only the two reviewed false positives)"
fi

section "man page"
if has cargo && has comm; then
    help_flags=$(cargo run -q -- --help 2>/dev/null | grep -oE -- '--[a-z-]+' | sort -u)
    # Option definitions only: macro lines that do not start an example, and not
    # the section that lists planned options.
    man_flags=$(awk '/^\.SS Not yet implemented/{skip=1;next} /^\.SH /{skip=0} !skip' docs/gcode.1 \
        | sed 's/\\-/-/g' | grep -E '^\.(B|BI|BR) ' | grep -vE '^\.B gcode ' \
        | grep -oE -- '--[a-z-]+' | sort -u)
    only_help=$(comm -23 <(printf '%s\n' "$help_flags") <(printf '%s\n' "$man_flags"))
    only_man=$(comm -13 <(printf '%s\n' "$help_flags") <(printf '%s\n' "$man_flags"))
    if [ -z "$only_help" ] && [ -z "$only_man" ]; then
        pass "docs/gcode.1 lists exactly the flags --help lists"
    else
        fail "docs/gcode.1 and --help disagree (help only: ${only_help:-none}; man only: ${only_man:-none})"
    fi
fi

section "docs"
if has mandoc; then
    if mandoc -T lint docs/gcode.1; then pass "mandoc -T lint docs/gcode.1"; else fail "mandoc -T lint docs/gcode.1"; fi
else
    skip "mandoc lint" "mandoc is not installed"
fi

if has python3; then
    if python3 scripts/check-doc-links.py; then pass "internal doc links resolve"; else fail "internal doc links resolve"; fi
else
    skip "doc link check" "python3 is not installed"
fi

section "coverage"
if has cargo-llvm-cov; then
    if cargo llvm-cov --fail-under-lines 85 >/dev/null; then pass "cargo llvm-cov >= 85% lines"; else fail "cargo llvm-cov >= 85% lines"; fi
else
    skip "line coverage" "cargo-llvm-cov is not installed"
fi

section "supply chain"
if has cargo-audit; then
    if cargo audit; then pass "cargo audit"; else fail "cargo audit"; fi
else
    skip "cargo audit" "cargo-audit is not installed"
fi
if has cargo-deny; then
    if cargo deny check all; then pass "cargo deny"; else fail "cargo deny"; fi
else
    skip "cargo deny" "cargo-deny is not installed"
fi

printf '\n%s\n' "-----"
printf 'failed: %d   skipped: %d\n' "$FAILED" "$SKIPPED"
exit "$FAILED"
