# 0004. Risk classification is independent of the model

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers

## Context

A tool that classifies commands by asking a model whether they are dangerous has
a circular safety model: the component that must never be wrong is the component
most likely to be wrong, and its failure is invisible. There is no fallback,
because the fallback is the thing that failed.

This is not hypothetical. gcode's prompt includes shell history, cwd, git branch
names, and file paths — all of which an attacker can influence by getting a
victim to clone a repository with an oddly named directory. Any model-based risk
assessment is therefore answering a question about attacker-influenced input,
using a model that has just read that input.

The classifier is also the component a user trusts most. When it says
`CRITICAL`, a user either runs the command or does not. Its accuracy has to be
high in both directions: false positives train users to press `y` reflexively,
which destroys the value of the gate entirely.

## Decision

Risk classification is deterministic, hand-written, and never consults the model.

1. `src/safety/` contains no model dependency, no I/O, and no randomness. It is
   a pure function from command string to `Verdict`.
2. Classification is re-run on the emitted string, not on anything the model
   says about it. The model's output is treated as untrusted input.
3. Three stages, cheapest first: normalise and split, then blocklist, then
   regex patterns plus structural checks.
4. Every segment of a compound command is classified independently, and the
   whole command takes the maximum. `ls; rm -rf /` is `CRITICAL`.
5. Editing a command re-runs classification from the edited string. There is no
   path from the editor to execution that skips the classifier.
6. 100 % statement coverage is a merge gate, not a goal.

## Alternatives considered

**Model-based classification.** Would scale to commands no author anticipated.
Rejected: it makes the safety gate depend on the same stochastic component that
produces the command, it is trivially influenced by injected context, and its
errors are not debuggable.

**Model classification with the hand-written rules as a first pass.** A hybrid.
Rejected for the same circularity. A second opinion from the same source does not
add independent evidence.

**Shell parsing to build an AST, then reason about the tree.** More precise than
regex, especially for quoting and redirection. Rejected as the *primary* mechanism
for v1.0: it needs a real shell parser, which is a large dependency with its own
security history, and it does not help with intent (`rm -rf ./build` parses
cleanly and is still a bad idea in the wrong directory). **It remains a
reasonable enhancement** for the structural checks in Phase 7, layered on top.

**Ask the user.** "Does this look dangerous?" Rejected: it is the opposite of
zero-friction and pushes all the work onto the person who wanted a tool to do it.
Confirmation is *added to* classification, never a substitute for it.

**Blacklist only.** Simple, few false positives. Rejected: it provides no
explanation, so users cannot calibrate, and "not on the list" reads as "safe" when
it means "unknown".

## Consequences

**Easier**

- The safety property is testable exhaustively and provable by inspection. No
  statistical argument is needed.
- No model, no network, no config: `safety` is pure and its tests run in
  milliseconds.
- A false negative is a bug with a failing test, not a tuning problem.
- Explanations come from the same table that assigns the level, so the reason
  shown to the user is always the actual reason.

**Harder**

- Every new dangerous pattern needs a human. This is a real ongoing cost, and it
  is the price of the guarantee.
- Regex over shell is imprecise in the ways noted above. The near-miss negative
  tests in [../TESTING.md](../TESTING.md) exist specifically to catch the
  over-broad patterns.
- The classifier cannot know intent, so the guarantee is "this is not obviously
  catastrophic", not "this is correct". Documented honestly in
  [../SAFETY.md](../SAFETY.md) § What the classifier does not do.
- Adding a pattern is a security-relevant change and gets a stricter review than
  an ordinary PR.

**Forecloses**

- Adaptive risk assessment. gcode will not get better at judging risk from
  experience.
- Contextual warnings like "this deletes files you created today", unless built
  as a separate non-blocking advisory layer later.

## Validation

- `cargo test --test safety` is a CI gate; the classifier cannot be merged
  without full coverage.
- The safety matrix in [../TESTING.md](../TESTING.md) lists a positive and a
  near-miss negative for every pattern.
- `cargo fuzz run fuzz_classify` runs 10 000 inputs with zero panics and zero
  unclassifiable outputs.
- `rg 'llama|inference|engine' src/safety/` returns nothing. A CI check enforces
  the module has no inference dependency.
- Every pattern carries a user-facing reason string, asserted by a test.

