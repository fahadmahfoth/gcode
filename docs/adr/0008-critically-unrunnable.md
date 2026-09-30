# 0008. CRITICAL commands are unrunnable, not merely warned about

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers
- **Consulted**: users who asked for a `--yes` escape hatch

## Context

A risk classifier has five levels. `CRITICAL` covers the commands that destroy
data irrecoverably: `rm -rf /`, `mkfs` on a real device, `dd` onto a raw disk,
fork bombs.

The obvious design is to treat `CRITICAL` like the other levels but with a louder
warning and a default of `N`. Users then ask for `--yes` to skip the prompt, and
the natural expectation is that `--yes` covers everything.

That is a bad design, and the reasoning matters more than the rule.

A soft block has a predictable failure mode. Users press `y` reflexively —
because they have learned to trust the tool on the ninety-nine other occasions.
On the hundredth, the one command they should never have run is the one they
approved without reading. The warning stops working precisely when it is most
needed, because the habit of approving has already been formed.

There is also a simpler argument. If a user wants `rm -rf /`, they can type
`rm -rf /`. The capability is not the problem. The problem is a machine that
*proposes* it. Making the tool refuse is therefore not a loss of function; it is
the correct division of labour.

## Decision

`CRITICAL` is a hard refusal. There is no flag, config setting, or environment
variable that runs a `CRITICAL` command through gcode.

1. The blocklist is evaluated **before** level assignment and yields `CRITICAL`.
2. No `--yes`, `--force`, `--no-confirm`, config key, or env var overrides it.
3. `--edit` cannot reach it. Editing a generated command re-runs classification
   from the edited string (see [0004](0004-independent-risk-classifier.md)), so
   an edit that produces a `CRITICAL` command is refused again.
4. The refusal is explained: the matched rule, the reason, and what to do instead.
5. The only route to the command is to type it manually, outside gcode, where
   the user is acting on their own intent rather than on a suggestion.
6. `HIGH` is not a soft `CRITICAL`. `HIGH` prompts with a full reason, prints the
   expanded command, and is overridable with `--yes`. Over-blocklisting `HIGH`
   would train the reflex this decision exists to prevent.

## Alternatives considered

**`CRITICAL` prompts with a default of `N`, overridable by `--yes`.** The
reflex argument above. Rejected. It is the design most tools choose and it is
precisely the design that fails silently under habituation.

**A `--force` flag that overrides `CRITICAL`, documented as dangerous.** Keeps
the refusal as the default path. Rejected: the flag is discoverable, gets used
once for a false positive, and is then in everyone's shell history and muscle
memory. A capability that is one flag away is not a capability that is absent.

**Ask for the word `I understand this is irreversible` rather than `y`.** A
friction bump that survives habituation better than a single keypress. Genuinely
a good idea, and it *is* what `HIGH` does. Rejected for `CRITICAL` only because
a correct-answer prompt on a command that should never run is a UX improvement
for an event that should not happen.

**Refuse only `rm -rf /`, allow everything else.** A minimal blocklist. Rejected:
it implies the rest of the tier is permitted, and `mkfs` on the wrong device is
no less final.

**A long, comprehensive blocklist covering hundreds of destructive patterns.**
Rejected: a long blocklist is a false sense of coverage and creates an
unmaintainable surface. The blocklist is short and obvious; the breadth comes
from the pattern table plus consent, and honesty about what the tool cannot know
is documented in [../SAFETY.md](../SAFETY.md).

## Consequences

**Easier**

- The strongest safety property in the system is unconditional, so it is simple
  to state, simple to test, and simple for a user to rely on.
- No "why is this not blocking that?" questions. The rule has no exceptions to
  reason about.
- `rm -rf /` is refused even if the model is wrong, the user is absent, a
  history entry is poisoned, and `--yes` is set. Four independent conditions
  still produce a refusal.
- A contributor adding a pattern to the blocklist cannot weaken safety, only
  extend it.

**Harder**

- Rare false positives are unfixable in-place. If a legitimate command is
  blocked, the user's only path is to type it manually, which is a worse
  experience than a prompt. Accepted, and mitigated by keeping the built-in
  blocklist short and only including genuinely irreversible operations.
- Users may find it paternalistic, and some will say so. The documentation
  addresses this directly: the capability is not removed, only the suggestion.
- Cannot be used for automated, unattended workflows that genuinely need
  something this destructive. That is the correct outcome; unattended
  destruction is not a use case.
- `--json` mode must report the refusal rather than exit ambiguously, so
  automation can distinguish "refused" from "failed".

**Forecloses**

- Any future "batch mode" or agentic mode must treat a `CRITICAL` step as a hard
  stop, and cannot offer a bypass. Phase 9 inherits this constraint.

## Validation

- Test: `gcode -c "delete everything from root" -y` exits non-zero and executes
  nothing.
- Test: editing a generated command into a blocklist pattern is refused again.
- Test: every blocklist entry has a positive test and a near-miss negative.
- Test: `rm -rf ./build` is **not** blocked, confirming the blocklist has not
  grown careless.
- Test: `--json` emits a distinct `refused` status, distinguishable from error.
- A CI check asserts no flag or config key maps to a `CRITICAL` override, so the
  property cannot be quietly reintroduced by adding a new option.

