# Safety Model

gcode runs commands on your machine. The entire safety design exists to make
that acceptable. This document is the honest version: what protects you, what
does not, and what the tool refuses to do at all.

---

## The core promise

> **gcode never executes a command you did not see, classify, and confirm.**

Three separate mechanisms enforce that, and each one alone would be insufficient.

1. **Grammar-constrained decoding.** The model physically cannot emit a token
   sequence that is not valid shell. This bounds the output space.
2. **Independent classification.** Every emitted string is re-examined by
   hand-written code that has no knowledge of the model. A command cannot be
   "allowed" because the model said it was fine.
3. **Consent.** Nothing runs without an affirmative `y`, unless you passed
   `--yes` yourself. Editing a command re-runs classification, so a `CRITICAL`
   command cannot be laundered by editing it.

---

## Risk levels

| Level | Meaning | Default behaviour |
|---|---|---|
| **SAFE** | Read-only, no side effects, bounded cost | Executes on `y`; skipped confirmation at HIGH trust |
| **LOW** | Read-only, or trivial reversible write (`touch`, `mkdir`) | Prompts, `[y/N]` default `N` |
| **MEDIUM** | Broad filesystem walk, package install, service restart, network fetch | Prompts with reason; can be suppressed by `always_confirm` |
| **HIGH** | Deletes data, changes permissions broadly, pipes network into shell, alters firewall, power state | Prompts with reason; prints the full expanded command |
| **CRITICAL** | Destroys a filesystem, wipes home, overwrites raw disks, fork bombs | **Hard blocked.** Not runnable, not editable, not overridable. |

### What CRITICAL means precisely

CRITICAL is not a warning. It is a refusal. There is no flag that runs a
CRITICAL command: not `--yes`, not `--edit`, not a config setting. The only way
to run one is to type it yourself, outside gcode, which is the correct outcome —
a tool that generates commands should not be the thing that destroys your disk.

The blocklist is evaluated **before** level assignment, and it is a plain string
match on the normalised command. It is short on purpose. A long blocklist is a
false sense of security; the HIGH tier plus consent is what actually protects
you.

---

## The classifier

Three stages, in order. Each stage is cheap; the expensive one never runs if an
earlier stage has already decided.

### Stage 1 — Normalise

Before any matching: strip comments, collapse whitespace, expand `$VAR` names to
their literal form where resolvable, resolve `~`, remove line continuations,
split on `;` and `&&` and `||` so **every segment is classified independently**.
A safe command chained to a destructive one is a destructive command.

This stage is the one that matters most and is the most commonly gotten wrong.
`ls; rm -rf /` must classify as CRITICAL even though the first segment is SAFE.

### Stage 2 — Blocklist

Literal substring matches against `[safety.blocklist]` in config, plus the
built-in set. A match is CRITICAL and stops everything.

### Stage 3 — Patterns and structure

`src/safety/patterns.rs` holds a table of `(regex, level, reason)`. Examples of
what is matched:

| Pattern | Level | Reason shown to the user |
|---|---|---|
| `\brm\s+-[a-z]*r[a-z]*f\s+/(\s\|$)` | CRITICAL | recursive delete of root |
| `\bmkfs(\.\w+)?\s` | CRITICAL | formats a filesystem |
| `\bdd\s+.*of=/dev/(sd\|nvme\|hd\|vd)` | CRITICAL | overwrites a raw block device |
| `>\s*/dev/(sd\|nvme\|hd\|vd)` | CRITICAL | redirects to a raw block device |
| `:\(\)\s*\{\s*:\|\:&\s*\}\s*;\s*:` | CRITICAL | fork bomb |
| `\brm\s+-[a-z]*r[a-z]*f\s+(~/\|\$)` | HIGH | recursive delete of home directory |
| `\bchmod\s+-R\s+777\s+/` | HIGH | world-writable permissions on system paths |
| `\b(curl\|wget)\b.*\|\s*(sudo\s+)?(sh\|bash)` | HIGH | pipes a network download into a shell |
| `\biptables\s+-F` | HIGH | flushes all firewall rules |
| `\b(shutdown\|reboot\|halt\|poweroff)\b` | HIGH | changes system power state |
| `\bgit\s+push\s+.*--force` | HIGH | force-pushes and can destroy remote history |
| `\bhistory\s+-c\b` | MEDIUM | clears your shell history |
| `\b(find\|grep\|du)\s.*\s/\s*$` | MEDIUM | recursive walk from filesystem root |
| `\b(apt\|yum\|dnf\|apk)\s+(install\|remove)\b` | MEDIUM | changes installed packages |
| `\bsystemctl\s+(stop\|restart)\b` | MEDIUM | restarts a system service |
| `\b(truncate\|shred)\b` | MEDIUM | destroys file contents |

Alongside the regex table, three **structural** checks that no regex catches:

- **Taint propagation.** If a segment writes to a path that a later segment
  deletes, the whole command is elevated one level. `cp x /etc/ && rm /etc/x`
  is worse than either half.
- **Privilege escalation.** Any segment invoking `sudo`, `su`, or `doas` is at
  least MEDIUM, regardless of what else it does.
- **Network + execute.** Any command that fetches and then executes, in one
  segment or chained, is at least HIGH.

### What the classifier does not do

It does not predict whether a command will succeed, whether a path is
important, or whether a *safe* command is *wise*. `rm -rf ./build` in the wrong
directory is classified SAFE and is still a bad idea. The classifier protects
you from catastrophic accidents, not from bad judgement.

---

## The confirmation flow

```
  command string
        │
        ▼
   normalise + split segments
        │
        ▼
   blocklist?  ──yes──▶  CRITICAL  ──▶  refuse, print why, exit
        │ no
        ▼
   patterns + structure
        │
        ▼
   ┌──────────┬─────────┬──────────┬──────────┬────────────┐
   │  SAFE    │  LOW    │  MEDIUM  │   HIGH   │  CRITICAL  │
   ├──────────┼─────────┼──────────┼──────────┼────────────┤
   │ print    │ prompt  │ prompt + │ prompt + │  refuse    │
   │ reason   │ reason  │ reason + │ reason + │            │
   │          │         │ cost est │ diff hint│            │
   └──────────┴─────────┴──────────┴──────────┴────────────┘
        │
        ▼
   user presses e ──▶ open $EDITOR ──▶ re-normalise ──▶ re-classify
                                            │
                                            └──▶ loop back to the prompt
```

The re-classification loop on `e` is not a convenience. Without it, editing a
generated command would be a trivial bypass of the entire safety model.

---

## `--yes` and what it actually does

`--yes` suppresses the *prompt*. It does not suppress:

- Classification. Levels are always computed.
- The risk line. It is always printed.
- History logging. The execution is always recorded.
- The blocklist. CRITICAL remains unrunnable.
- The edit re-classification.

`--yes` means "I have read the risk line already, do not make me press a key."
It does not mean "skip the safety system."

---

## Threat model

### Protected against

| Threat | Mitigation |
|---|---|
| Model hallucinates a destructive command | Grammar constraints + classifier + consent |
| Model produces invalid shell | Grammar-constrained decoding; parse-check before display |
| Malicious model file (tampered GGUF) | SHA256 pinned in the registry; mismatch = delete and refuse |
| Prompt injection via hostile filenames or history | Untrusted-data delimiters, redaction, grammar, and an independent classifier |
| History leaking secrets into a prompt | Redaction before assembly; 2 KB output cap; 15-entry cap |
| Accidental `rm -rf /` | CRITICAL blocklist, unreachable by any flag |
| Hook breaking the user's shell | Idempotent install, preserves existing hooks, byte-precise `--remove` |
| Supply chain compromise | `cargo-deny`, `cargo-audit`, `cargo-vet` in CI, no `[patch]` from outside |
| Log file readable by other users | `0600` on history, explicit umask on the data dir |
| Ctrl-C leaving a half-written history entry | Single `write` + `fsync` of one line; a torn line is skipped on read |

### Explicitly not protected against

| Not protected | Why, and what to do |
|---|---|
| A malicious **local** gcode binary | Anyone who can replace your binary owns your shell. Verify signatures. |
| A compromised **mirror** | Mitigated by the registry checksum, which is in the signed repo |
| Commands that are *wrong but safe-looking* | No classifier can know your intent. Read the command. |
| Secrets already in `history.jsonl` from gcode's own executions | Redaction covers prompt assembly, not your own shell's output. Scrub the file if you pasted a token. |
| Resource exhaustion (a command that eats your RAM) | Cost estimates are shown, not enforced. `ulimit` yourself if it matters. |
| Side effects of a well-formed but *irreversible* action on a remote system | Out of scope. gcode runs locally; remote consequences are your call. |

---

## Recovery: when a hook goes wrong

If `gcode --init` ever disturbs your prompt:

```bash
gcode --remove
exec $SHELL          # or open a new terminal
```

Manual removal, if the binary is unavailable — the block is delimited by markers,
so it is safe to delete by hand:

```bash
# bash
sed -i '/# >>> gcode init >>>/,/# <<< gcode init <<</d' ~/.bashrc

# zsh
sed -i '/# >>> gcode init >>>/,/# <<< gcode init <<</d' ~/.zshrc
```

**gcode never edits `.bashrc`/`.zshrc` outside those markers.** If you find lines
without them, they were not written by gcode.

---

## Reporting a vulnerability

See [../SECURITY.md](../SECURITY.md). Responsible disclosure, 90-day window, CVE
for critical findings.

If you find a way to make gcode execute a CRITICAL-classified command, or to
leak a redacted secret, that is a critical finding — report it privately first
and do not open a public issue.
