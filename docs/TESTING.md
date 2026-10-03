# Testing Strategy

gcode generates commands that run on real machines. A test that passes while the
classifier has a hole is worse than no test, because it buys false confidence in
the one component that must not be wrong. The strategy below optimises for the
safety layer first and the model second.

---

## Principles

1. **Test the classifier harder than the model.** The model is stochastic and
   only ever as good as its training. The classifier is deterministic, small,
   and is the thing that must never be wrong.
2. **No test loads a real model.** The `InferenceEngine` trait exists so the
   whole pipeline runs against `FakeEngine` in milliseconds. A test that takes
   30 seconds will be skipped, and a skipped test protects nobody.
3. **Test files, not intentions.** Shell integration is tested by sourcing it in
   a real bash and a real zsh in a container, not by asserting on the source.
4. **Negative tests first.** For every pattern that matches, there is a
   near-miss that must not match. A test suite with only positives cannot tell
   a working classifier from a maximally paranoid one.
5. **Fuzz the parsers.** Anything that parses model output or user input gets a
   fuzz target. A panic in a CLI is a crash report, not a security incident, but
   it is still a bug.

---

## Coverage targets

| Module | Target | Tool | Why this number |
|---|---|---|---|
| `safety/classifier.rs` | **100 %** | `cargo test` | The gate. No uncovered branches. |
| `safety/patterns.rs` | **100 %** | `cargo test` | Every pattern positive + negative |
| `cli.rs` | 90 % | `cargo test` | Parse and validation |
| `config.rs` | 90 % | `cargo test` | Precedence is easy to break |
| `context/history.rs` | 90 % | `tempfile` | Corruption, rotation, torn lines |
| `context/prompt.rs` | 90 % | `insta` | Redaction is the security property |
| `model/download.rs` | 85 % | `mockito` | Resume and hash-mismatch paths |
| `model/registry.rs` | 90 % | `cargo test` | Validation errors |
| `inference/engine.rs` | 75 % | `FakeEngine` | The real path needs a model; test the seams |
| `exec/runner.rs` | 85 % | `cargo test` | Timeout, exit codes |
| `ui/` | 60 % | `cargo test` | Interactive; test the key handling, not the colours |
| **Repository total** | **≥ 85 %** | `cargo llvm-cov` | CI gate; a decrease fails the build |

Coverage is measured with `cargo llvm-cov --fail-under-lines 85`. The number
exists to catch *newly untested code*, not to chase a percentage — a module at
100 % with weak assertions is worse than one at 85 % with good ones.

---

## Test layers

### Unit tests — inline, next to the code

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_root_delete_as_critical() {
        let v = classify("rm -rf /");
        assert_eq!(v.level, Risk::Critical);
        assert!(v.reasons.iter().any(|r| r.message.contains("root")));
    }

    #[test]
    fn does_not_classify_build_dir_delete_as_root_delete() {
        // The near-miss test. A classifier that matches this is useless.
        let v = classify("rm -rf ./build");
        assert_ne!(v.level, Risk::Critical);
    }

    #[test]
    fn splits_chained_commands_and_classifies_each() {
        let v = classify("ls; rm -rf /");
        assert_eq!(v.level, Risk::Critical);
        assert_eq!(v.segments.len(), 2);
    }
}
```

### Integration tests — `tests/`

| File | Covers |
|---|---|
| `tests/cli.rs` | End-to-end flag handling via `assert_cmd` |
| `tests/api.rs` | The public surface from outside the crate, including the whole config precedence chain |
| `tests/safety.rs` | The full pattern table, one test per row |
| `tests/history.rs` | Corruption, rotation, torn lines, permissions |
| `tests/pipeline.rs` | The whole flow against `FakeEngine` |
| `tests/json_output.rs` | `--json` schema stability, snapshot-tested |
| `tests/install_script.sh` | The installer, in Docker |

Integration tests never load a model. `FakeEngine` is wired in through a
`#[cfg(test)]` constructor so production code has no test-only branches.

### Snapshot tests — `insta`

Snapshots are for things whose exact bytes matter:

- The rendered prompt, for 5 canonical contexts
- `--help` and `--version` output
- `--json` output shape
- `--explain` output
- Classifier error messages

```bash
cargo insta review    # interactive accept/reject
cargo insta test      # just report
```

A snapshot change in CI is a **failure**, not an auto-accept. Prompt changes
without reviewed snapshot changes are the fastest way to silently degrade
accuracy.

### Shell tests — real shells in Docker

```bash
docker run --rm -v "$PWD:/w" -w /w bash:5.2 bash tests/shell/bash_test.sh
docker run --rm -v "$PWD:/w" -w /w zshusers/zsh:5.9 zsh tests/shell/zsh_test.sh
```

These run against a real shell, which is the only way to catch the failures that
actually happen: `$?` clobbering, `PROMPT_COMMAND` mangling, `history` output
format differences, a hook that breaks under `set -u`.

The `$?` preservation test is mandatory and has a dedicated file, because it is
the single most common way a history hook breaks someone's shell.

### Fuzz tests — `cargo-fuzz`

| Target | Input | Property |
|---|---|---|
| `fuzz_classify` | any string | never panics; always returns a `Verdict` |
| `fuzz_prompt` | arbitrary history entries | redaction never panics; secrets never appear |
| `fuzz_json_output` | any `Output` | always serialises |

```bash
cargo fuzz run fuzz_classify -- -max_total_time=300
```

Ten thousand mutated commands must produce zero panics. A panic in the classifier
means an attacker-reachable crash in the one code path that decides what is
dangerous.

---

## The safety test matrix

The highest-value tests in the project. Every row must have a case.

| Category | Positive (must flag) | Negative (must not flag) |
|---|---|---|
| Root delete | `rm -rf /`, `rm -rf /*`, `rm -fr /` | `rm -rf ./build`, `rm -rf /tmp/x` |
| Chained | `ls; rm -rf /`, `true && rm -rf /` | `ls \| grep x` |
| Home delete | `rm -rf ~`, `rm -rf $HOME` | `rm -rf ~/project/tmp` |
| Disk | `dd if=/dev/zero of=/dev/sda`, `> /dev/sda` | `dd if=a.img of=b.img` |
| Format | `mkfs.ext4 /dev/sda1`, `mkfs -t xfs /dev/nvme0n1` | `mkfs -t ext4 disk.img` |
| Fork bomb | `:(){ :|:& };:` | `echo done` |
| Permissions | `chmod -R 777 /`, `chmod 777 /etc/passwd` | `chmod +x script.sh` |
| Download-execute | `curl x \| sh`, `wget -qO- x \| sudo bash` | `curl -o x url`, `bash script.sh` |
| Firewall | `iptables -F` | `iptables -L` |
| Power | `shutdown -h now`, `reboot` | `echo reboot` |
| Git | `git push --force origin main` | `git push origin main` |
| Privilege | `sudo anything` at all | plain command |
| History | `history -c` | `history \| tail` |
| Quoting | `echo 'a; rm -rf /'` → one segment, SAFE | `echo a; rm -rf /` |
| Taint | `cp x /etc/y && rm /etc/y` → escalated | `cp x /tmp/y` |
| Edit laundering | `e` turning HIGH into CRITICAL → re-blocked | — |
| User blocklist | a command containing a `[safety.blocklist]` entry → CRITICAL, with `--yes` too; an edit into the list → re-blocked | `make build` against an entry of `make deploy` |
| Non-interactive | piped stdin → no execution, `SAFE` included | `--yes` or `--dry-run` in the same pipe |
| Confirmation | `ls` and `mkdir -p out` → asked about; refused under `DenyAll` | `--complete` of a `SAFE` command; an explicit `always_confirm = "MEDIUM"` |
| Executor | `CRITICAL`, a blocklisted command, a denied `MEDIUM`, `--dry-run`, `-n`, `--complete` → the executor is never called | `ls -la` → runs; an approved `rm -rf ./build` → runs once, as edited |

### Config rows

Not classifier rows, but the same discipline applies: each of these has a case
that must pass and a near miss that must not.

| Property | Must | Must not |
|---|---|---|
| Malformed field | the message names the field, e.g. `model.context_size` | a message saying only "invalid config" |
| Unknown key | refused | silently ignored |
| Missing file | `Ok(None)` | an error |
| Broken file | exit 2 for every mode that classifies or generates | silently running on defaults; blocking `--explain`, `--list-models`, `--check`, `--remove` |
| Precedence | flag beats env beats file beats default | a flag's hidden default outranking the file |
| `always_confirm` | `SAFE`..`HIGH` accepted | `CRITICAL` accepted |
| `redact_env` | built-in patterns always present | built-ins removable by config |
| Range check | a file-only value checked, with its range named | an unchecked file value |

That last column is the one teams skip and the one that bites. A regex that is
too broad destroys trust in the tool; users start pressing `y` reflexively, and
then the classifier is pointless.

---

## Performance tests

```rust
#[bench]
fn bench_cold_start(b) { /* no model loaded */ }

#[bench]
fn bench_prompt_assembly(b) { /* 15 history entries */ }

#[bench]
fn bench_classify(b) { /* 100 realistic commands */ }
```

`bench_classify` runs in-process, no model, so it can gate every PR.

### Targets

| Metric | Target | Measured by |
|---|---|---|
| Cold start, no model loaded | < 500 ms | `gcode --bench --no-model` |
| Prompt assembly, 15 entries | < 10 ms | `cargo bench` |
| Classification | < 1 ms | `cargo bench` |
| Inference, 64 tokens, 2 cores | < 1000 ms | `gcode --bench` |
| Inference, 64 tokens, 8 cores | < 400 ms | `gcode --bench` |
| Peak RSS | < 1.5 GB | `/usr/bin/time -v` |
| Binary size | < 15 MB stripped | `ls -lh target/release/gcode` |

Classification under 1 ms matters for more than speed: it has to be cheap enough
to run on every segment of every command, unconditionally.

---

## Test matrix in CI

| OS | Arch | Runner | Notes |
|---|---|---|---|
| Ubuntu 24.04 | x86_64 | `ubuntu-latest` | Primary |
| Ubuntu 22.04 | x86_64 | `ubuntu-22.04` | Oldest supported glibc |
| Ubuntu 20.04 | x86_64 | `ubuntu-20.04` | musl target cross-check |
| Debian 12 | x86_64 | Docker | |
| Fedora 40 | x86_64 | Docker | RPM build |
| Arch | x86_64 | Docker | PKGBUILD build |
| Alpine 3.20 | x86_64 | Docker | Static binary |
| macOS 14 | aarch64 | `macos-14` | Apple Silicon |
| macOS 13 | x86_64 | `macos-13` | Intel |
| Linux | aarch64 | Docker/QEMU | Build only |

Gated on `push` to `master` and on every PR. The full matrix is nightly plus on
release tags; PRs run fmt, clippy, unit, integration, and the safety suite only,
to keep the feedback loop under five minutes.

---

## Test data

| Fixture | Location | Contains |
|---|---|---|
| Canonical contexts | `tests/fixtures/contexts.json` | 5 environments × 3 histories |
| Bench prompts | `tests/fixtures/bench_prompts.json` | 20 realistic requests |
| History corruption cases | `tests/fixtures/history/` | Torn, truncated, wrong-type lines |
| HTTP responses | `mockito` inline | Small GGUF stand-ins, never a real model |
| Safety corpus | `tests/fixtures/safety_corpus.json` | Every row of the matrix above |

**Fixture hygiene:**

- No real secrets, ever. Redaction tests use obviously fake values shaped like
  secrets (`sk-` + 32 chars) so the test is realistic without being dangerous.
- No real user data, no real command output from a real machine.
- No model files in git. Ever.
- Every fixture is reviewed for credential-shaped strings before merge, and the
  secret scanner runs on top of that.

---

## Running the suite

```bash
cargo test                       # everything, no model needed
cargo test --lib                 # unit only
cargo test --test safety         # the gate
cargo insta test                 # snapshot report
cargo insta review               # accept snapshot changes
cargo llvm-cov --html            # coverage
cargo llvm-cov --fail-under-lines 85
cargo bench                      # in-process benchmarks
cargo fuzz run fuzz_classify -- -max_total_time=300
./scripts/test-shell.sh          # real bash + zsh in Docker
./scripts/test-matrix.sh         # the full OS matrix locally
```

Expected wall time: `cargo test` under 20 seconds. If it is slower, something
loaded a model, and that is a bug in the test setup.

### The MSRV audit

`rust-version = "1.85"` in `Cargo.toml` is a promise, and a `cargo update` can
break it without touching a single line of this repository. Cargo does **not**
downgrade a dependency that has already been locked, and it does not check
`rust-version` of dependencies when resolving on behalf of a newer toolchain —
so `cargo update` will happily resolve a crate that needs a newer Rust and leave
you to discover it on someone else's 1.85 machine.

Run this after any `cargo update` or `cargo add`:

```bash
# Every package in the lockfile must declare rust-version <= 1.85.
cargo metadata --format-version 1 --locked \
  | python3 -c "
import json, sys
def rv(s):
    return tuple(int(x) for x in s.split('.')) if s else (0, 0, 0)
msrv = rv('1.85')
meta = json.load(sys.stdin)
bad = [(p['name'], p['version'], p.get('rust_version'))
       for p in meta['packages'] if rv(p.get('rust_version')) > msrv]
if bad:
    for name, ver, need in bad:
        print(f'{name} {ver} needs {need}')
    sys.exit(1)
print(f'OK: all {len(meta[\"packages\"])} packages build on 1.85')
"
```

A failure here is fixed with `cargo update -p <crate> --precise <version>`, not
by relaxing the floor. The alternative is to admit the MSRV moved, which is an
ADR question, not a lockfile edit.

CI runs this too, so the check that matters most is the one a contributor cannot
skip locally.

---

## Related

- [SAFETY.md](SAFETY.md) — what the classifier must guarantee
- [ROADMAP.md](ROADMAP.md) — which phase each test belongs to
- [ARCHITECTURE.md § Testing seams](ARCHITECTURE.md#testing-seams) — why the code
  is shaped to be testable
- [CONTRIBUTING.md](CONTRIBUTING.md) — what CI will check on your PR
