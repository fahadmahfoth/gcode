# 0018. A bilingual default model: qwen3-0.6b

- **Status**: Accepted
- **Date**: 2026-10-03
- **Deciders**: gcode maintainers
- **Supersedes**: [0014](0014-default-model-kitty-bash-llm.md), the choice of default

## Context

ADR 0014 chose `kitty-bash-llm` as the default and described it as a 0.5 B
shell-specialised model. That choice was made before the model was checked
against a requirement that has since become explicit: **gcode is used in Arabic
and English**, and the request the user types is very often Arabic. A model that
only understands English is not answering the request it is given.

`kitty-bash-llm` turned out to be real and to match its card — 0.5 B, 398 MB at
Q4_K_M, built on `Qwen/Qwen2.5-Coder-0.5B-Instruct`, Apache-2.0 — but its own
model card declares `language: en`. It turns an English request into a bash
command. It does not turn an Arabic request into one.

The constraints from ADR 0014 still hold and are not relaxed here:

1. Downloadable in the background, under ~500 MB.
2. Around one second on a two-core CPU.
3. Coherent shell, not word salad.
4. A licence that permits redistribution.

## Decision

The default model is **`qwen3-0.6b`**, the Q4_K_M GGUF (397 MB), Apache-2.0.

1. It meets every ADR 0014 constraint: 397 MB, the same parameter count as
   `kitty-bash-llm`, so the same latency class, under a redistribution licence.
2. It is multilingual, trained on 100+ languages including Arabic, so an Arabic
   or English request both reach the same model. That is the requirement
   `kitty-bash-llm` could not meet.
3. `kitty-bash-llm` remains in the registry, first-class, described as the
   English-only shell specialist. For an English speaker it is likely the more
   accurate shell model, and it stays one flag away.
4. `qwen3-1.7b` (Q4_K_M, 1.1 GB) is added as the accuracy-first multilingual
   choice, for a machine that can afford it.
5. The registry gains a `verified` field. Every entry ships `verified = false`
   until a maintainer has downloaded the file, hashed it, and confirmed the
   digest. `build.rs` refuses a **release** build while any entry is unverified,
   so a checksum taken from a web page cannot reach a user. Debug builds and the
   test suite are unaffected.
6. URLs are pinned to a Hugging Face commit, not a branch, so the bytes behind a
   URL cannot change under a fixed checksum.

## Alternatives considered

**Keep `kitty-bash-llm` as the default and tell Arabic users to pass
`--use-model`.** No wrong default for English, and the smallest first download.
Rejected: it makes the majority of this project's own users run the wrong model
by default, and the "request in Arabic, command in English" case is not an edge
case here.

**`qwen3-1.7b` as the default.** More capable. Rejected: 1.1 GB and several
seconds on the two-core baseline breaks constraints 1 and 2 for the default.
Kept as the accuracy option.

**`qwen2.5-1.5b-instruct`.** Also multilingual, Apache-2.0, ~1.0 GB. Rejected as
the default for the same size/latency reason; `qwen3-0.6b` keeps the promised
size while staying multilingual.

**Fine-tune a bilingual shell model.** The right long-term answer, and explicitly
worth doing. Rejected for now: it needs training infrastructure and a licensing
review for the training data, which is the ADR 0014 reasoning unchanged.

## Consequences

**Easier**

- A user can type the request in Arabic or English and get a command.
- The default still fits the background-download and one-second promises.
- A checksum cannot ship unverified: the release gate is now "did someone hash
  the file", which is the honest question.

**Harder**

- `qwen3-0.6b` is a general model, not shell-specialised, so its shell accuracy
  is expected to be below `kitty-bash-llm`. `gcode --bench` has to settle how far
  below, and the default may change back if the gap is large. This is the main
  open question this ADR accepts.
- `qwen3` models have a "thinking" mode; the sampler and prompt have to keep it
  off for a one-shot command, or latency and output shape both suffer.
- Two of the three entries are hosted by `unsloth`, a third party, not the
  upstream `Qwen` org. The commit pin and the checksum are what make that safe;
  the licence (Apache-2.0) is unchanged from upstream.

## Validation

- `build.rs` fails a release build while any entry has `verified = false`, and
  this is exercised before a release, not only reasoned about.
- `src/model/registry.rs` has a test that `verified` defaults to `false` and is
  parsed when set, so the flag cannot silently read as `false` for everything.
- `gcode --bench` compares `qwen3-0.6b` against `kitty-bash-llm` and
  `qwen3-1.7b` for latency, tokens/second, and Arabic and English shell accuracy,
  and the numbers in [../MODELS.md](../MODELS.md) come from it.
- A commit-pinned URL plus a verified checksum means a mirror can serve only the
  same bytes or fail.
