# Installing gcode

gcode is a single static binary. It has no runtime dependencies beyond a POSIX
shell, and it does not phone home. The only network access it ever performs is
downloading the model file, and that is opt-in.

---

## Requirements

| Requirement | Version | Notes |
|---|---|---|
| OS | Linux (any), macOS 12+ | Windows: use WSL2, see below |
| Arch | x86_64, aarch64 | arm64 first-class, not an afterthought |
| RAM | 2 GB free | 1.5 GB is the model, the rest is headroom |
| Disk | 500 MB free | ~8 MB binary + ~400 MB model |
| Shell | bash ≥ 4, zsh ≥ 5 | fish/nushell: planned Phase 8 |
| Network | Only for model download | Fully functional offline afterwards |

No Rust toolchain required. No Python. No API key. No account. No telemetry.

---

## Method 1 — one-line installer (recommended)

```bash
curl -fsSL https://get.gcode.dev | sh
```

The installer is POSIX `sh`, ~150 lines, and does eight things in order:

1. Detects OS and CPU architecture.
2. Selects the matching release artifact.
3. Downloads it to a temp dir and verifies the SHA256 checksum against the
   published `checksums.txt` before extracting anything.
4. Installs to `/usr/local/bin` if writable, otherwise `~/.local/bin` — no
   `sudo` prompt if it can be avoided.
5. Starts the model download in the background so it never blocks the prompt.
6. Installs the shell integration (`--init`), which is the *only* step that
   modifies your shell config, and only for shells it detects.
7. Runs a self-test that proves inference works.
8. Prints the one command to try next.

### Installer safety properties

- Verifies checksums. Refuses to install on mismatch.
- Never runs as root unless `/usr/local/bin` genuinely requires it.
- Does not modify `~/.bashrc`/`~/.zshrc` silently — it appends a single marked
  block, and prints the exact lines it added.
- `GCODE_INSTALL_DIR` overrides the install location.
- `GCODE_NO_MODEL=1` skips the model download entirely.
- `GCODE_NO_INIT=1` skips shell integration.
- Review it any time: it lives in this repo at `scripts/install.sh`, and is Phase 4 work
per [ADR 0012](adr/0012-one-liner-install.md).

---

## Method 2 — package managers

```bash
# macOS (Homebrew)
brew install gcode/tap/gcode

# Debian / Ubuntu (APT repository)
sudo apt install gcode

# Fedora / RHEL (COPR)
sudo dnf install gcode

# Arch (AUR)
yay -S gcode

# Alpine (static binary package)
apk add gcode

# Nix
nix-env -iA nixpkgs.gcode
```

Each repository is signed. GPG keys are published at
`https://get.gcode.dev/gcode.gpg.key`.

---

## Method 3 — Docker

No local install, no model on your disk:

```bash
docker run --rm -it \
  -v "$HOME/.gcode:/root/.gcode" \
  -v "$PWD:/work" \
  -w /work \
  ghcr.io/fahadmf/gcode:latest \
  -c "list all files larger than 1GB"
```

The image ships with the model baked in, so the first run is instant. History
persists in the mounted volume, never in the container layer.

---

## Method 4 — build from source

You need Rust 1.85+.

```bash
git clone https://github.com/fahadmahfoth/gcode.git
cd gcode
cargo build --release
./target/release/gcode --version
```

For a fully static Linux binary:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

Enable shell completions and the man page:

```bash
cargo install --path . --features completions
```

---

## Method 5 — Windows (WSL2)

Native Windows is out of scope for v1.0 by design — the shell grammar, the
history hooks, and the package formats are all POSIX. WSL2 gives you the real
thing:

```powershell
wsl --install -d Ubuntu-24.04
```

Then install inside WSL using Method 1. Do **not** run the Linux binary natively
under Git Bash or MSYS — the process and filesystem semantics differ enough that
path handling and history capture break in confusing ways.

---

## Post-install

### Shell integration

```bash
gcode --init          # detect shell, install hooks, idempotent
gcode --check         # report status without changing anything
gcode --remove        # uninstall hooks
```

The hook captures the last command, its exit code, the working directory, and the
timestamp into `~/.gcode/history.jsonl`. It is what makes `gcode --fix` and
context-aware generation work. Output is deliberately not captured, so that the
hook cannot make `[ -t 1 ]` false and degrade colour, pagers, and editors for
every command the user runs. The hook:

- Preserves any existing `PROMPT_COMMAND` / `precmd` hooks.
- Skips gcode's own commands (no self-referential history).
- Costs under 5 ms per prompt.
- Writes nothing if `GCODE_NO_HISTORY=1` is set.

If it ever breaks your prompt, `gcode --remove` fixes it, and
[SAFETY.md § Recovery](SAFETY.md#recovery-when-a-hook-goes-wrong) covers the
manual repair.

### Model download

```bash
gcode --list-models            # what the registry knows about
gcode --download-model        # fetch the default
gcode --use-model bashgemma   # switch default
gcode --model /path/to.gguf   # one-off, no registry change
```

Models on first run:

| Model | Size | Speed (2-core CPU) | Quality |
|---|---|---|---|
| `kitty-bash-llm` (default) | 398 MB | ~0.9 s | good |
| `bashgemma-2b` | 1.3 GB | ~2.4 s | better |
| `qwen2.5-0.5b-instruct` | 380 MB | ~0.8 s | decent |
| `custom` (any GGUF) | varies | varies | yours |

See [MODELS.md](MODELS.md) for the full registry, custom model setup, and tuning.

---

## Uninstall

```bash
gcode --remove                             # shell hooks first
rm -f ~/.config/gcode/config.toml          # config (keeps history)
rm -f ~/.gcode/history.jsonl               # history
rm -f ~/.local/share/gcode/models/*.gguf   # model cache, Linux
rm -f ~/Library/Application\ Support/gcode/models/*.gguf   # macOS
rm -f "$(command -v gcode)"                # binary
```

Package-manager installs uninstall normally (`apt remove gcode`, `brew uninstall
gcode`) and handle the shell integration via maintainer scripts.

---

## Verify the install

```bash
gcode --version
gcode doctor          # prints a full health report
gcode -c "list files here" --dry-run
```

`gcode doctor` checks: binary signature, model presence and checksum, shell hook
status, available RAM and disk, inference latency, and history file permissions.
Run it first whenever something misbehaves — the output is the fastest route to
a diagnosis, and it is written to `docs/TROUBLESHOOTING.md` as a checklist.

---

## Where files live

| Platform | Config | Data | Models |
|---|---|---|---|
| Linux | `~/.config/gcode/config.toml` | `~/.local/share/gcode/` | `~/.local/share/gcode/models/` |
| macOS | `~/.config/gcode/config.toml` | `~/Library/Application Support/gcode/` | `~/Library/Application Support/gcode/models/` |
| Docker | `/root/.config/gcode/` | `/root/.local/share/gcode/` | baked into image |

History is `~/.gcode/history.jsonl` on all platforms, mode `0600`. It never
leaves the machine. It is deliberately **not** inside the data directory: ADR
0005 fixes that location, and an accepted ADR is not changed quietly. `gcode` has
no `data` subdirectory to put it in.

Config is `~/.config/gcode` on macOS too, not `~/Library/Application
Support`. A config file is something you edit by hand, and one location for
every platform is easier to document and easier to remember.

---

## Troubleshooting the install

| Symptom | Cause | Fix |
|---|---|---|
| `gcode: command not found` | installed to `~/.local/bin`, not on PATH | `export PATH="$HOME/.local/bin:$PATH"` |
| Installer aborts on checksum | proxy or mirror truncated download | retry, or use the package manager |
| macOS blocks the binary | Gatekeeper quarantine | see [TROUBLESHOOTING.md § Gatekeeper](TROUBLESHOOTING.md#macos-blocks-the-binary-gatekeeper) |
| Model download stalls | network policy | set `GCODE_MODEL_MIRROR`, see [MODELS.md](MODELS.md#mirrors) |
| Prompt looks broken after `--init` | hook conflict | `gcode --remove`, then file an issue with `gcode doctor` output |

Full symptom table: [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
