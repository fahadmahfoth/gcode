# gcode — Natural language to shell commands

**gcode** turns a sentence into a shell command, tells you how dangerous it is,
and waits for your approval. It runs entirely on your machine. No API key, no
account, no telemetry, and no network access after the model is downloaded once.

```
$ gcode -c "list all files larger than 1GB"

🔍 Analyzing with context from last 12 commands...
📝 Generated:

   find . -type f -size +1G -exec ls -lh {} + 2>/dev/null

⚠️  Risk: MEDIUM — recursive filesystem walk

→ [y] run  [n] cancel  [e] edit  [c] copy  [?] explain
```

---

## Status

**Pre-1.0, and earlier: there is no runnable gcode yet.** The design, the
decision records, the roadmap, and the agent tooling are written. The Rust
program is not. Every command in this README is a target interface, built in the
order defined by [docs/ROADMAP.md](docs/ROADMAP.md). Nothing here is shipped —
see [CHANGELOG.md](CHANGELOG.md) for the honest list of what exists.

| Phase | Name | Status |
|---|---|---|
| 0 | Repository foundation | 🟡 docs and tooling done, code not started |
| 1 | Core inference pipeline | ⬜ |
| 2 | Shell integration & history | ⬜ |
| 3 | Safety layer | ⬜ |
| 4 | Packaging | ⬜ |
| 5 | Distribution & CI/CD | ⬜ |
| 6 | Polish & launch | ⬜ |

---

## Install

```bash
curl -fsSL https://get.gcode.dev | sh
```

That is the whole thing. It detects your platform, verifies the checksum, picks a
writable install directory, downloads the model in the background, installs the
shell integration, and runs a self-test.

Package managers, Docker, and building from source:
**[docs/INSTALL.md](docs/INSTALL.md)**

```bash
brew install gcode/tap/gcode     # macOS
sudo apt install gcode          # Debian / Ubuntu
sudo dnf install gcode          # Fedora
yay -S gcode                    # Arch
```

---

## Why

Shell commands are powerful and completely memorised at the same time. You
already know what you want; you just do not remember the flags. `gcode` closes
that gap in about a second, without sending a single byte of your machine to
anyone.

Three things make it different from asking a web chatbot:

1. **It is offline.** The model is 400 MB on your disk. It works on a plane, and
   your history never leaves the machine.
2. **It knows your session.** Recent commands, their exit codes, and their error
   output are used as context, so `gcode --fix` can actually repair what just
   failed.
3. **A destructive command cannot slip through.** Every command is classified
   into five risk levels, and the critical tier is unrunnable by any flag.

---

## Modes

```bash
gcode -c "count lines in all python files"   # generate
gcode --fix                                   # repair the last failed command
gcode --complete "find /var/log -type f -name" # finish a partial command
gcode --explain "tar -xzf a.tgz -C /srv"      # explain, never execute
gcode                                          # interactive session
```

Full reference: **[docs/USAGE.md](docs/USAGE.md)**

---

## Safety

Five levels — `SAFE`, `LOW`, `MEDIUM`, `HIGH`, `CRITICAL`. The classifier is
hand-written code that never consults the model, so a model persuaded by a
malicious filename cannot talk its way past it.

`CRITICAL` is not a warning. It is a refusal. `rm -rf /` is unrunnable by
`--yes`, by `--edit`, and by any config setting. The only way to run one is to
type it yourself, outside gcode.

| Not protected against | Why |
|---|---|
| A replaced local binary | Anyone who can swap your binary owns your shell. Verify signatures. |
| Commands that are wrong but safe-looking | No classifier knows your intent. Read the command. |
| A corrupted model file | Mitigated by pinned SHA256; a mismatch is deleted, not loaded. |
| Secrets you printed yourself | Redaction covers what goes into a prompt, not your own scrollback. |

Full threat model: **[docs/SAFETY.md](docs/SAFETY.md)**

---

## Privacy

- **No telemetry.** No analytics, no crash reporting, no usage data.
- **No network after install.** One model download, then offline.
- **No API keys.** Not now, not later, not for any tier.
- **History stays local** in `~/.gcode/history.jsonl`, mode `0600`.
- **Secrets are redacted** before anything enters a prompt: `*_TOKEN`,
  `*_SECRET`, `*_KEY`, `*_PASSWORD`, `AWS_*`, `GITHUB_TOKEN`, `Bearer` headers,
  and long hex or base64 runs.
- **100% open source**, MIT licensed. Verify any of the above by reading
  the source itself (`src/` — see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).

```bash
gcode --no-history -c "..."   # opt out of history context entirely
GCODE_NO_HISTORY=1             # stop recording
```

---

## Requirements

| | |
|---|---|
| OS | Linux (any), macOS 12+ · Windows via WSL2 |
| Arch | x86_64, aarch64 |
| RAM | 2 GB free |
| Disk | 500 MB free |
| Shell | bash ≥ 4, zsh ≥ 5 |
| Network | Only for the one-time model download |

---

## Performance

Targets, measured on 2 physical cores with no GPU. Real numbers from
`gcode --bench` are in [CHANGELOG.md](CHANGELOG.md).

| Metric | Target |
|---|---|
| Inference, 64 tokens | < 1000 ms |
| Cold start, no model | < 500 ms |
| Classification | < 1 ms |
| Peak RSS | < 1.5 GB |

---

## Documentation

| | |
|---|---|
| [Install](docs/INSTALL.md) | Every platform, package manager, uninstall |
| [Usage](docs/USAGE.md) | Every flag, every mode, recipes |
| [Safety](docs/SAFETY.md) | Risk model, threat model, recovery |
| [Architecture](docs/ARCHITECTURE.md) | Modules, data flow, extension points |
| [Roadmap](docs/ROADMAP.md) | The build plan, with acceptance criteria |
| [Testing](docs/TESTING.md) | Strategy, the safety matrix, coverage |
| [Models](docs/MODELS.md) | Registry, custom models, tuning |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Symptoms to fixes |
| [Contributing](docs/CONTRIBUTING.md) | Setup, PR rules, AI-assisted contributions |
| [Releasing](docs/RELEASING.md) | Versioning, signing, rollback |
| [Decisions](docs/DECISIONS.md) | Why it is built this way |
| `man gcode` | [docs/gcode.1](docs/gcode.1) |

---

## Contributing

```bash
git clone https://github.com/fahadmf/gcode.git
cd gcode && cargo build && cargo test
```

The test suite never loads a model — it uses a fake inference engine, so it runs
in seconds. Safety changes require both a positive and a near-miss negative test.

**This repository builds itself with OpenCode.** The agents in `.opencode/`
implement phases, write tests, and update the docs from a single instruction, and
the rules they follow are in [AGENTS.md](AGENTS.md):

```
/status     /plan <phase>     /build     /test     /review     /docs     /release
```

Details: [docs/CONTRIBUTING.md § AI-assisted contributions](docs/CONTRIBUTING.md#ai-assisted-contributions)

---

## Licence

MIT — [LICENSE](LICENSE).

Model weights are distributed under their own licences, recorded per entry in
`models/registry.toml` — created in Phase 1, per [ADR 0010](docs/adr/0010-embed-the-model-registry.md).
