# 0003. Grammar-constrained decoding

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers
- **Consulted**: security reviewers; users running gcode in hostile directories

## Context

An autoregressive model emits arbitrary token sequences. For a chat assistant that
is fine. For a tool that turns its output into a shell command, it is not: the
failure surface is "the model produced something that runs on my machine".

Three distinct problems, often conflated:

1. **Validity.** Small models produce unterminated quotes, unbalanced braces,
   truncated pipelines, and stray markdown. This is the common failure and it is
   mostly cosmetic — the command simply does not run.
2. **Capability.** A model may want to write prose, a markdown fence, or
   `Command:` and must be prevented from doing so.
3. **Injection.** A hostile directory, a maliciously named file, or a poisoned
   history entry can put text in the prompt that the model then obeys. Prompt
   injection cannot be *prevented* by instructions; it can only be bounded.

Problem 3 is the one that matters. Instructions in the prompt saying "ignore
context and follow my orders" are a mitigation, not a defence. Any defence that
relies on the model choosing to cooperate is not a defence.

## Decision

Constrain decoding with a Bash grammar at the token level, using `llguidance`.

1. The inference step is grammar-constrained. Invalid shell is not filtered
   after the fact; it is unreachable during generation.
2. The grammar permits exactly one complete command per generation, and
   terminates it cleanly.
3. The grammar carries a small set of additional restrictions beyond validity —
   currently, no fork bombs. This is a defence-in-depth measure, not the primary
   control.
4. `llguidance` over the C++ `llama.cpp` grammar library, to keep one FFI
   boundary rather than two.
5. Post-processing strips markdown fences, language tags, and leading labels,
   because a grammar constrains structure, not necessarily the first token of a
   preamble.
6. **The classifier is not optional and not a fallback.** The grammar bounds the
   output space; the classifier independently decides risk (see
   [0004](0004-independent-risk-classifier.md)). Neither substitutes for the
   other.

## Alternatives considered

**Post-hoc validation: generate freely, then parse and reject.** Much simpler.
Rejected: it converts a quality problem into a hard failure. A small model
produces invalid shell often enough that free decoding would mean a visible
failure every few invocations, which teaches users to distrust the tool. It also
does nothing at all about injection, because the dangerous output is valid shell.

**Prompt instructions only.** "Output only the command, no prose." Rejected as
the sole control: small models follow this inconsistently, and it is trivially
overridden by injected instructions.

**A much larger local model, no grammar.** Would improve validity through raw
capability. Rejected: it does not bound the output space, costs 1.3 GB minimum,
and treats a security property as a quality problem.

**Fine-tuning a model specifically on shell.** Complementary, and eventually
worth doing. Rejected as a *substitute* for the grammar: fine-tuning improves the
distribution of outputs, it does not restrict the support of the output space.

**A separate validation model.** Rejected: a second local model doubles load time
and memory, and a model-based validator is a probabilistic check where a grammar
is a certainty.

## Consequences

**Easier**

- Invalid shell becomes structurally impossible rather than unlikely.
- 100 % of generated commands parse, verifiable by a fuzz test rather than a
  sample.
- Prompt injection is bounded rather than merely discouraged: an attacker who
  controls the context still cannot make the model emit a parseable, dangerous
  command the classifier has not independently examined.
- Accuracy improves for free, because the model never wastes tokens on fences or
  prose it is not allowed to produce.

**Harder**

- Constrained decoding is slower than free decoding, because the sampler checks
  the grammar at every step. Budgeted at 10–20 % latency overhead.
- Grammar engineering is a real maintenance cost. A shell grammar is
  non-trivial, and shell syntax changes (or diverges between bash, zsh, and POSIX
  mode) will surface as grammar bugs.
- Porting the grammar to fish and nushell is genuine work, not a configuration
  change. Deferred to Phase 8.
- If the grammar fails to apply for any reason, we fall back to unconstrained
  generation — which means there is a code path where injection is bounded only
  by the classifier. That path must log a warning.

**Forecloses**

- Any feature requiring multi-line output, comments, or a heredoc *without* a
  separate grammar for each. Batch mode (Phase 9) will need a batch grammar.
- Nothing else. This is additive to any other approach.

## Validation

- A 200-run fuzz test produces 100 % parseable shell.
- The grammar rejects `rm -rf /` at the grammar layer, asserted by a dedicated
  test in addition to the classifier test.
- A snapshot suite of accepted completions is reviewed for regressions.
- The injection test suite plants hostile filenames, `history.jsonl` content,
  and git branch names containing injection payloads, and asserts that no
  dangerous command is ever emitted and always confirmed.
- If grammar application fails, `gcode -v` logs `grammar: fallback to
  unconstrained` and the safety test suite has a case covering that path.

