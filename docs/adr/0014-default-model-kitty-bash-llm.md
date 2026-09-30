# 0014. Default model: kitty-bash-llm

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

The default model determines the first impression and, for most users, the only
model they will ever use. It must satisfy four constraints at once:

1. **Downloadable in the background without dominating the install.** Under
   ~500 MB.
2. **Under one second on a two-core CPU.** This is the one-second promise.
3. **Accurate enough that users do not have to correct it constantly.** Below
   roughly 0.5 B the model stops producing coherent shell.
4. **Licensable for redistribution.** The tool is MIT and must not inherit a
   licence that forbids it.

These pull against each other. A bigger model is more accurate and slower. A
specialised model is better at shell and worse at everything else. A
general-purpose model is more robust and worse at shell.

## Decision

`kitty-bash-llm` at Q4_K_M (398 MB) is the default.

1. The default is the smallest model that produces coherent shell commands at an
   acceptable rate. Quality below this point degrades non-linearly.
2. The registry lists better models prominently, and the README tells users
   plainly that they exist and why they might want one.
3. `--use-model` persists a different choice, so upgrading the default never
   overrides a deliberate user decision.
4. The registry records the licence of every model, and the README distinguishes
   gcode's MIT licence from the model weights' own licences.
5. Selection is evidence-based. Every claim about a model comes from
   `gcode --bench` on measured data, never from a benchmark on a website.
6. `Q2_K` and `Q3_K` are refused outright rather than allowed with a warning,
   because below Q4_K_M the model emits *valid but wrong* shell, which the
   grammar will not catch and which is more dangerous than an error.

## Alternatives considered

**A 1.5 B general-purpose model as the default.** Better accuracy, ~1.0 GB, and
~1.7 s on two cores. Breaks the one-second promise and doubles the download.
Rejected: the download size in particular is a real drop-off point, and the
accuracy gain does not justify a first experience that feels slow.

**A 2 B shell-specialised model (`bashgemma`).** The most accurate option, and
the right answer for users who care about accuracy. ~1.3 GB, ~2.4 s on two cores.
Rejected as the *default* for the download and latency reasons above. It remains
in the registry, documented as the accuracy-first choice.

**A general-purpose 0.5 B model (`qwen2.5-0.5b`).** Comparable size and speed.
Rejected as the default: it is measurably worse at shell, and shell is the only
thing gcode does.

**Let users pick on first run.** No wrong default, and no one complains about a
choice they made. Rejected: it adds a decision to the thirty-second promise, and
most users cannot evaluate the options. The registry is there when they want it.

**Ship no model and require a download.** Maximum control over what runs.
Rejected: it adds a step and a failure mode to the most important moment of
first use.

**Fine-tune our own model on shell data.** Potentially the best answer.
Rejected as a v1.0 timeline item: it requires training infrastructure and a
licensing review for the training data. Genuinely worth doing later, and it would
be a significant advantage.

## Consequences

**Easier**

- Install stays in the background without dominating it. 398 MB is noticeable
  but not an event.
- The one-second promise holds on the two-core baseline.
- Users with a fast machine or a GPU get a better experience without changing
  anything, since the same binary loads any GGUF.
- A better model is one flag away, so we are not locked in by this choice.

**Harder**

- Users will need to correct the model sometimes. This is the main quality
  complaint, and it will appear in issues. The mitigation is honest positioning:
  the safety layer does not depend on model quality, which is the argument in
  [0003](0003-grammar-constrained-decoding.md) and
  [0004](0004-independent-risk-classifier.md).
- Tuning the prompt for one model may not transfer to another. Snapshot tests
  per model class, and documented prompt-sensitivity.
- A model dependency introduces licence and provenance risk. Mitigated by a
  pinned checksum and a recorded licence per entry.
- If `kitty-bash-llm` becomes unmaintained, the default has to move, and the
  prompt may need re-tuning. Accepted as a known cost.

**Forecloses**

- Nothing. The model is a config value, not a code dependency.

## Validation

- `gcode --bench` records p50 and p95 latency for every registry entry, and the
  numbers in [../MODELS.md](../MODELS.md) come from that, not from estimates.
- A CI job checks that the default entry is present, has a valid `sha256`, and
  has a compatible licence.
- The benchmark suite is run in CI on a schedule, and a regression in the default
  model's latency or accuracy blocks the release.
- Prompt snapshots are reviewed per model class, so a prompt change that breaks
  the default is caught.

