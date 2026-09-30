# Troubleshooting

Start here before anything else:

```bash
gcode doctor
```

It checks the binary, the model, the shell hook, permissions, memory, disk, and
measured inference latency, and prints the first thing worth fixing.

```bash
gcode doctor --verbose      # everything, including paths
gcode -v -c "list files"    # debug logs to stderr
GCODE_LOG_FILE=/tmp/gcode.log gcode doctor
```

---

## Install problems

### `gcode: command not found`

Installed to `~/.local/bin`, which is not on your `PATH`.

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc   # or ~/.zshrc
exec $SHELL
```

---

### macOS blocks the binary (Gatekeeper)

The binary is unsigned, so macOS quarantines it. gcode signs and notarises
releases; if you hit this you have a locally built binary or a very old release.

```bash
# Check first — do not blindly clear the quarantine
xattr -l "$(command -v gcode)"

# If it shows a com.apple.quarantine attribute:
xattr -d com.apple.quarantine "$(command -v gcode)"
```

For a locally built binary, the honest fix is to build it yourself rather than
lifting the quarantine flag — see [CONTRIBUTING.md](CONTRIBUTING.md). The
security-conscious alternative:

```bash
codesign --sign - "$(command -v gcode)"   # ad-hoc sign
```

---

### The installer aborts on checksum mismatch

A truncated download, a corporate proxy, or a mirror serving stale bytes.

```bash
GCODE_MODEL_MIRROR=https://your-internal-mirror/gcode sh -c "$(curl -fsSL https://get.gcode.dev)"
```

If it persists, the release may genuinely be mis-signed. Verify independently
before proceeding:

```bash
curl -fsSL https://github.com/fahadmf/gcode/releases/download/v1.0.0/checksums.txt
shasum -a 256 gcode-x86_64-apple-darwin.tar.gz
```

Compare them by eye. A real mismatch is a security issue — report it per
[../SECURITY.md](../SECURITY.md) and do not install it.

---

### `apt install gcode` cannot find the package

The repository was added but `apt update` has not run since, or the GPG key is
missing.

```bash
sudo apt update
sudo apt install gcode
```

```bash
# If you get NO_PUBKEY
sudo apt-key adv --keyserver keyserver.ubuntu.com --recv-keys <KEYID>
# Or, preferred:
curl -fsSL https://get.gcode.dev/gcode.gpg.key | sudo gpg --dearmor -o /usr/share/keyrings/gcode.gpg
```

---

## Model problems

### `No model found`

```bash
gcode --download-model
gcode --list-models
```

---

### `model checksum mismatch` / `SHA256 mismatch`

The file on disk is corrupt, truncated, or was replaced. gcode deletes it
deliberately — a model that does not match its pinned hash is not loaded.

```bash
gcode --download-model          # re-download
gcode doctor                    # confirm the hash now matches
```

If it recurs, your disk is failing. Check SMART before blaming gcode.

---

### `unsupported quantization: Q2_K`

Deliberate. Below Q4_K_M the model emits syntactically valid but semantically
wrong commands, which is worse than an error because the grammar will not catch
it. Use Q4_K_M or better. See [MODELS.md § Quantisation](MODELS.md#quantisation).

---

### Inference is very slow

```bash
gcode --info                    # backend, threads, layers offloaded
gcode --n-threads 4             # cap threads; beyond physical cores it gets slower
gcode --n-gpu-layers 99         # offload to GPU
gcode --context-size 2048       # smaller window is faster
gcode --bench                   # measure p50 and p95 properly
```

Common causes, in order of likelihood:

1. Running with `--n-gpu-layers 0` on a machine with a GPU.
2. More threads than physical cores.
3. A 2 B model on a 2-core machine — that is a ~2.4 s operation, not a bug.
4. Thermal throttling on a laptop. `gcode --info` shows the current clock if
   supported.

---

### `No GPU backend compiled in`

The release build has no GPU support. Rebuild with the feature flag:

```bash
cargo build --release --features cuda     # NVIDIA
cargo build --release --features metal    # macOS — usually the default
```

---

### The model produces prose instead of a command

The grammar is not applied, or the model is too small.

```bash
gcode -v -c "..." 2>&1 | grep -i grammar     # confirm the grammar loaded
gcode --use-model bashgemma                  # a model tuned for shell
gcode --temperature 0.0                      # determinism
```

If it persists on a 0.5 B model, that is the model, not the tool. Small models
respect grammar less reliably; a larger one fixes it.

---

## History and shell problems

### `--fix` says there is nothing to fix

`--fix` needs a **failed** command in history. Check:

```bash
gcode --init --check       # is the hook installed?
tail -3 ~/.gcode/history.jsonl
```

The file is empty or stale means the hook is not firing.

---

### The hook is not recording anything

```bash
gcode --init --check
echo $?     # must be 0
```

Common causes:

| Cause | Fix |
|---|---|
| `GCODE_NO_HISTORY=1` set | unset it |
| Hook installed but the shell was not restarted | `exec $SHELL` |
| Wrong shell file edited | check the path `--init` reported |
| `PROMPT_COMMAND` clobbered by something else | see the next item |

```bash
# bash: is the hook still in PROMPT_COMMAND?
echo "$PROMPT_COMMAND" | grep gcode
grep -n "gcode" ~/.bashrc
```

---

### My prompt broke after `gcode --init`

```bash
gcode --init --remove
exec $SHELL
```

If the binary is unavailable, remove the marked block by hand — the markers make
this safe:

```bash
sed -i '/# >>> gcode init >>>/,/# <<< gcode init <<</d' ~/.bashrc   # bash
sed -i '/# >>> gcode init >>>/,/# <<< gcode init <<</d' ~/.zshrc   # zsh
```

gcode only ever writes between those markers. If you find shell config lines
without them, gcode did not write them.

---

### `$?` is always 0 after a command

This is a real bug in a shell hook, and it is the one gcode tests hardest. The
hook must capture `$?` as its first statement. If you are reading this, either
your build predates the fix or a third-party hook is clobbering it:

```bash
grep -n 'PROMPT_COMMAND\|precmd' ~/.bashrc ~/.zshrc
```

Disable other history-modifying hooks one at a time to find the conflict, then
please [file an issue](../SECURITY.md) — losing `$?` is a correctness bug, not a
preference.

---

### History is growing too large

Rotation is automatic at 10 MB, keeping the newest 5 MB. To shrink it now:

```bash
wc -c ~/.gcode/history.jsonl
tail -5000 ~/.gcode/history.jsonl > /tmp/h && mv /tmp/h ~/.gcode/history.jsonl
```

To stop recording:

```bash
export GCODE_NO_HISTORY=1
```

---

## Command quality

### The generated command ignores my context

```bash
gcode --context 40 -c "..."      # more history
gcode --no-git=false ...         # make sure git context is on
gcode -v -c "..." 2>&1 | grep prompt   # inspect what was actually sent
```

The prompt is snapshotted in the debug log, so you can see exactly what the model
saw. If the context is missing from the prompt, the hook is not recording.

---

### It generates something for the wrong platform

The model sometimes emits `apt` for a macOS box. gcode includes the OS in the
prompt, so this is a model limitation, mitigated by a larger model:

```bash
gcode --use-model bashgemma
```

You can also force it:

```bash
gcode -c "using brew, list all installed packages"
```

---

### It is classified CRITICAL but the command is harmless

Usually a false positive in the pattern table, and worth reporting. As a
workaround, rewrite the request to produce a different command. To understand
which rule fired:

```bash
gcode --explain "<command>"     # prints every matched pattern and its reason
```

If it is a genuine false positive, open an issue with the command. Pattern
accuracy is a core quality bar, and near-miss negatives are part of the test
matrix in [TESTING.md](TESTING.md).

---

### Commands run but produce no output

Probably a redirect, or a pager waiting for input. Common culprits:

```bash
gcode --explain "<command>" | grep -E '>|2>/dev/null'
```

Strip the redirect and re-run, or pipe through `head`.

---

## Performance

| Symptom | Check | Fix |
|---|---|---|
| Slow startup | `gcode --bench --no-model` | Should be < 500 ms. If not, a config or hook problem |
| Slow every prompt | `gcode --init --check` | A slow hook is added cost on every prompt |
| High memory | `/usr/bin/time -v gcode -c "..."` | Target < 1.5 GB. Reduce `--context-size` |
| Output is empty | `gcode --no-color` | Colour codes can confuse a pager |

---

## Diagnostics reference

| Command | Answers |
|---|---|
| `gcode doctor` | Is the installation healthy? |
| `gcode --info` | Which backend, threads, GPU layers? |
| `gcode --bench` | How slow is it, really? |
| `gcode --list-models` | What models are available? |
| `gcode --list-model-paths` | Where is it looking for models? |
| `gcode --init --check` | Is the shell hook installed? |
| `gcode --explain "<cmd>"` | Why is this classified this way? |
| `gcode -v <cmd> 2>&1` | What actually happened? |
| `gcode --json -n -c "<req>"` | Scriptable, no colour, no prompts |
| `tail -f ~/.gcode/history.jsonl` | What has it recorded? |

---

## Still stuck

Open an issue with:

1. `gcode doctor` output
2. `gcode --version` output
3. `gcode --info` output
4. The exact command you ran
5. What you expected, and what happened instead

**Never include your history file.** It contains your real commands and may
contain output with credentials in it. Reproduce the problem in a fresh
directory instead, or redact with `gcode --explain`.

For a suspected vulnerability, do not open a public issue — see
[../SECURITY.md](../SECURITY.md).
