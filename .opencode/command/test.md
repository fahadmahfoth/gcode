---
description: Run the full test suite, coverage, fuzzing, and the secret scan
agent: gcode-test-engineer
---

Verify gcode. Run everything; report the real output.

## Arguments

`$ARGUMENTS` may narrow this: `unit`, `safety`, `coverage`, `fuzz`, `shell`,
`secret`. If empty, run everything that is available.

## Do this

Run in this order. Report each result separately, with the real output. Do not
summarise a failure away and do not claim a pass you did not observe.

### 1. Format and lint

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

### 2. Tests

```bash
cargo test
```

If `src/safety/` is touched, this is the gate:

```bash
cargo test --test safety
```

### 3. Coverage

```bash
cargo llvm-cov --fail-under-lines 85
```

If `cargo-llvm-cov` is not installed, say **"coverage tooling unavailable"**.
Do not estimate a number.

### 4. Fuzzing

```bash
cargo fuzz run fuzz_classify -- -max_total_time=300
```

Zero panics over 10 000 inputs is the bar. A panic in the classifier is a bug in
the code path that decides what is dangerous.

### 5. Shell tests — real shells in containers

```bash
docker run --rm -v "$PWD:/w" -w /w bash:5.2 bash tests/shell/bash_test.sh
docker run --rm -v "$PWD:/w" -w /w zshusers/zsh:5.9 zsh tests/shell/zsh_test.sh
```

The `$?` preservation test is mandatory. It has its own file, because losing
`$?` is the single most common way a history hook breaks a user's shell.

### 6. Supply chain

```bash
cargo audit        # when available
cargo deny         # when available
```

### 7. Secret scan — this repository is public

```bash
grep -rInE '(api[_-]?key|secret|token|password|BEGIN [A-Z ]*PRIVATE KEY)[[:space:]]*[:=][[:space:]]*["'"'"'][^"'"'"']{8,}' \
  --exclude-dir=target --exclude-dir=.git . || echo "clean"
```

**If this fires on a real credential, stop.** Report it immediately. Do not fix
it, do not commit it, do not continue. Rotation is the human's decision and it
comes first. See `AGENTS.md` § 0.1.

### 8. Timing

```bash
time cargo test
```

If the suite exceeds 30 seconds, find out why before reporting. Usually a test
loaded a real model, which is a design bug rather than a slow test.

## Report

```
Format:    [pass/fail]
Clippy:    [pass/fail, N warnings]
Tests:     [pass/fail, N passed, N failed]
Safety:    [pass/fail]  (if src/safety/ was touched)
Coverage:  [measured %, or "tooling unavailable" — never an estimate]
Fuzz:      [N inputs, N panics]
Shell:     [bash pass/fail] [zsh pass/fail]
Audit:     [N advisories, or "cargo audit unavailable"]
Secrets:   [clean, or THE FINDING — stop here]
Time:      [N seconds]

Failures:  <each one, with the actual error>
```

If everything passes, say so plainly and give the numbers. If something fails,
lead with the failure. Do not bury a broken safety test under a list of passes.

Do not fix anything here. This command verifies; `/build` implements.
