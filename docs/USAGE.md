# Using gcode

gcode turns a sentence into a shell command, shows you the risk, and waits for
your approval. It never executes anything you did not confirm.

---

## The 10-second version

```bash
gcode -c "list all files larger than 1GB"
```

```
🔍 Analyzing with context from last 12 commands...
📝 Generated:

   find . -type f -size +1G -exec ls -lh {} + 2>/dev/null

⚠️  Risk: MEDIUM — recursive filesystem walk
    Expands globs across the tree; slow on large mounts.

→ [y] run  [n] cancel  [e] edit  [c] copy  [?] explain
```

Press `y` and Enter. That is the whole product.

---

## Modes

### 1. Generate — `-c / --command`

The primary mode. Natural language in, command out.

```bash
gcode -c "count lines in all python files"
gcode -c "أريد عرض كل الملفات الأكبر من 500 ميجابايت"   # Arabic works
gcode -c "kill the process listening on port 8080"
```

### 2. Fix — `--fix`

Reads the most recent failed command *and its error output* from history, then
proposes a corrected command. This is where history context pays for itself.

```bash
$ gcode -c "install nodejs 20"
  ⚡ apt install nodejs=20
  ✗ E: Version '20' not found for 'nodejs'

$ gcode --fix
🔍 Reading failure from history (exit 100, 2s ago)...
📝 Fixed:

   curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - \
     && sudo apt install -y nodejs

⚠️  Risk: HIGH — pipes a network download into a shell
```

### 3. Complete — `--complete`

Finishes a half-typed command, respecting what you already wrote.

```bash
gcode --complete "find /var/log -type f -name"
# → find /var/log -type f -name '*.log' -exec tail -n 50 {} \;
```

### 4. Explain — `--explain`

Explains an existing command instead of generating one. No execution.

```bash
gcode --explain "tar -xzf backup.tar.gz -C /srv --strip-components=1"
```

### 5. Interactive — `gcode` with no arguments

A prompt loop for exploratory work.

```bash
$ gcode
gcode 1.0.0 — natural language shell · Ctrl+D to exit

> show me disk usage by directory
  ⚡ du -h --max-depth=1 . | sort -rh | head -20
  ⚠ Risk: LOW — reads directory sizes
  → executed

> now only the top 3
  ⚡ du -h --max-depth=1 . | sort -rh | head -3
  ⚠ Risk: LOW
  → executed

> save that as a script
  ⚡ printf '#!/usr/bin/env bash\n...' > du-top.sh && chmod +x du-top.sh
  ⚠ Risk: MEDIUM — writes a file
```

Each turn sees the previous turns, so follow-ups work without restating context.

### 6. Chat — `--chat` ⛔ *planned (Phase 9)*

Full conversational mode with file-scoped editing. Tracked in
[ROADMAP.md](ROADMAP.md). The interactive mode above covers the same ground for
v1.0.

---

## Every flag

### Input

| Flag | Default | Meaning |
|---|---|---|
| `-c`, `--command <TEXT>` | — | Natural language request. Required unless another mode is chosen. |
| `--fix` | false | Repair the last failed command from history |
| `--complete <PARTIAL>` | — | Continue a partial command |
| `--explain <CMD>` | — | Explain a command, generate nothing |

### Execution control

| Flag | Default | Meaning |
|---|---|---|
| `-y`, `--yes` | false | Skip the confirmation prompt. **Still classified, still logged.** |
| `-n`, `--no` | false | Generate and print, never execute (alias of `--dry-run`) |
| `--dry-run` | false | Print the command and risk, then exit |
| `--edit` | false | Open `$EDITOR` on the generated command before confirming |

### Context

| Flag | Default | Meaning |
|---|---|---|
| `--context <N>` | 15 | Number of recent history entries to read |
| `--no-history` | false | Ignore history entirely |
| `--no-git` | false | Do not include git status/branch in the prompt |
| `--no-env` | false | Do not include OS/shell/cwd in the prompt |
| `--in <DIR>` | cwd | Build context as if running in this directory |

### Model

| Flag | Default | Meaning |
|---|---|---|
| `--model <PATH_OR_NAME>` | registry default | GGUF path or registry name |
| `--list-models` | — | Print the registry with sizes and checksums |
| `--download-model [NAME]` | — | Download a model (optional name) |
| `--update-model` | — | Re-download the default if the checksum changed |
| `--use-model <NAME>` | — | Set the config default permanently |
| `--n-threads <N>` | all cores | Inference thread count |
| `--n-gpu-layers <N>` | 0 | Layers to offload to GPU (Metal/CUDA) |
| `--context-size <N>` | 4096 | Prompt context window in tokens |
| `--temperature <F>` | 0.2 | Sampling temperature; keep low for commands |

### Shell integration

| Flag | Default | Meaning |
|---|---|---|
| `--init` | — | Install the history hook for the detected shell |
| `--init --check` | — | Report hook status, change nothing |
| `--init --remove` | — | Remove the hook |
| `--generate-completions <SHELL>` | — | Emit bash/zsh/fish completions to stdout |

### Diagnostics

| Flag | Default | Meaning |
|---|---|---|
| `--doctor` | — | Full health report (see [TROUBLESHOOTING.md](TROUBLESHOOTING.md)) |
| `-v`, `--verbose` | — | Debug logging to stderr |
| `--log-level <LEVEL>` | info | `error`/`warn`/`info`/`debug`/`trace` |
| `--log-file <PATH>` | — | Also write logs to a file |
| `--no-color` | auto | Disable ANSI colour |
| `--json` | — | Machine-readable output (no prompts, no colour) |
| `--version` | — | Print version, commit hash, build date |
| `--help` | — | Print help |

---

## The confirmation prompt

| Key | Action |
|---|---|
| `y` | Execute |
| `n` | Cancel (nothing runs, nothing is logged as executed) |
| `e` | Edit in `$EDITOR`, then re-classify the edited command |
| `c` | Copy to clipboard, do not execute |
| `?` | Explain why the risk level was assigned |
| `r` | Refine the natural-language request and regenerate |
| `s` | Split a multi-step plan and run step by step |
| `a` | Abort a multi-step plan |
| `Ctrl+C` | Cancel everything |

Edit-then-reclassify matters: gcode re-runs the classifier on whatever you edited,
so you cannot launder a `CRITICAL` command past the gate by editing it.

---

## Recipes

```bash
# Free up space safely — dry run first, always
gcode -c "delete log files older than 30 days in /var/log" --dry-run
gcode -c "delete log files older than 30 days in /var/log"

# What did I just install?
gcode --explain "$(history 1)"

# Find who is using port 3000
gcode -c "which process is listening on port 3000" -y

# Review before running anything at all
gcode -n -c "restart nginx"

# Start a session with no memory of what came before
gcode --no-history -c "what is the fastest way to compress a directory"

# Reproduce a bug from a failed command, with context
gcode --fix --context 30

# Scriptable: get JSON, parse it
gcode --json -n -c "count files by extension" | jq -r '.command'
```

---

## Configuration

`~/.config/gcode/config.toml`. Every field is optional; the defaults are chosen to
be correct for most people.

```toml
# ~/.config/gcode/config.toml

[model]
name = "kitty-bash-llm"
path = "~/.local/share/gcode/models/kitty-bash-llm-q4_k_m.gguf"
n_threads = 0            # 0 = all cores
n_gpu_layers = 0
context_size = 4096
temperature = 0.2
top_p = 0.9
max_tokens = 256

[context]
history_entries = 15
output_tail_bytes = 2048
include_git = true
include_env = true
include_cwd = true

[safety]
# Minimum risk level that always prompts, regardless of --yes
always_confirm = "MEDIUM"
# Refuse to ever run these, even with --yes and even if edited
blocklist = [
  "rm -rf /",
  "mkfs",
  "dd of=/dev/",
  ":(){ :|:& };:",
]
# Show the explanation line by default
explain = true

[shell]
hook = true
capture_output = true
redact_env = ["*_TOKEN", "*_SECRET", "*_KEY", "AWS_*", "GITHUB_TOKEN"]

[ui]
color = true
emoji = true
animation = false
```

Precedence: CLI flag > environment variable > config file > built-in default.

Environment variables: `GCODE_MODEL`, `GCODE_MODEL_MIRROR`, `GCODE_NO_HISTORY`,
`GCODE_NO_INIT`, `GCODE_NO_MODEL`, `GCODE_CONFIG`, `GCODE_LOG_LEVEL`,
`GCODE_HISTORY_FILE`, `RUST_LOG`.

---

## Shell aliases worth setting

```bash
# Generate and run in one keystroke, still with confirmation
alias g='gcode -c'

# Show the command, never run it — for when you just want the syntax
alias gq='gcode -n -c'

# Re-run the last suggested command that you approved
gcode --last
```

---

## Privacy in one paragraph

gcode reads your shell history to build context. That history stays on your
machine in `~/.gcode/history.jsonl` with mode `0600`. Nothing is uploaded. The
only network call the tool ever makes is fetching the model file, and only once.
Environment variables matching `*_TOKEN`, `*_SECRET`, `*_KEY`, `AWS_*`, and
`GITHUB_TOKEN` are redacted before they can enter a prompt. You can verify all
of this by reading the source — it is 100% open source under MIT.

See [SAFETY.md](SAFETY.md) for the full security model.
