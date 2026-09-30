# 0001. Local-first, offline inference

- **Status**: Accepted
- **Date**: 2026-09-30
- **Deciders**: gcode maintainers
- **Consulted**: users who handle untrusted repositories and regulated environments

## Context

Natural-language-to-command tools are usually hosted services. That model is the
default because it is easier: no model to ship, no GPU, no binary size, and
accuracy scales with how much you can afford to run.

It is also incompatible with the use case that matters most here. The prompts
people type into this tool are descriptions of their own machines — paths,
processes, file sizes, error messages, and frequently fragments of whatever they
were doing. A command line is a stream of confidential information. Sending it to
a third party to be useful is asking users to trade a real security property for
convenience they did not consent to.

The counter-argument is real: a 0.5 B local model is meaningfully worse than a
frontier hosted one. That is the actual cost of this decision and it is not
small.

## Decision

gcode is local-first and offline-capable.

1. The primary path is a GGUF model executed locally through llama.cpp.
2. After the model is downloaded, the tool performs **zero** network requests.
3. No API key, account, or registration exists in any tier, including future
   enterprise ones.
4. Shell history, cwd, and git context are assembled locally and never leave the
   machine.
5. A future cloud tier, if one exists, must be opt-in, separately configured,
   and must never be required for a feature available locally.

## Alternatives considered

**Hosted API as the primary path.** Highest accuracy, smallest binary, no
download step. Rejected: it makes the tool unusable offline and air-gapped, and
it makes the privacy promise unshippable. Users handling customer data, on
untrusted networks, or in regulated environments — a large share of the terminal
population — cannot use it at all.

**Hybrid: local by default, hosted when a key is present.** Rejected. A
conditional trust boundary is worse than a fixed one. The user cannot reason
about where their data went, and a compromised or misconfigured key silently
redirects everything.

**No LLM. Template and retrieval-based command construction.** Deterministic,
tiny, instant, perfectly private. Genuinely attractive and still worth building
as a fast path. Rejected as the *primary* mechanism because natural language
covers requests no template author anticipated, and because a system that
fails closed on novel input is not useful to the people who need it most.

**Local model with a small hosted component for planning.** Rejected for the same
reason as hybrid. Also forecloses the air-gapped case entirely.

## Consequences

**Easier**

- Air-gapped, offline, and field use all work identically.
- The privacy claim is verifiable by reading the source; there is no network
  code path to audit beyond the model downloader.
- No per-request cost, so there is no pressure to meter, throttle, or monetise
  usage.
- No key management, no rotation, no leak surface.
- Latency is bounded by hardware, not by a queue.

**Harder**

- 400 MB to 1.3 GB of weights before the first command. Mitigated by the
  background download in the installer.
- Accuracy is materially lower than a frontier model. Accepted; mitigated by the
  risk classifier, which does not depend on model quality, and by the ability to
  switch models.
- Inference latency is 0.9–2.9 s depending on model and hardware, versus
  effectively instant for a hosted model. The one-second promise is therefore
  scoped to the default 0.5 B model on reasonable hardware, and stated honestly.
- Supported hardware is bounded by what can run a GGUF. No exotic accelerators,
  no managed infrastructure.
- Enterprise buyers who want SSO and a compliance attestation have to work with
  us rather than configuring a cloud tenant. Deferred to post-1.0.

**Forecloses**

- Making accuracy the primary competitive axis. We compete on safety, privacy,
  and install simplicity instead.
- A usage-based business model, permanently. Any future revenue must come from
  enterprise support or non-core products.

## Validation

- `rg 'reqwest|ureq|hyper|TcpStream'` in `src/` matches only
  `src/model/download.rs`. Enforce in CI; any new network call fails the build
  until it is justified in a new ADR.
- `gcode doctor` reports network state and confirms no calls occurred after
  install.
- A CI job runs the full test suite with networking disabled.
- User reports of "it tried to phone home" are treated as critical bugs.

---

*Superseding this record requires a new ADR with a migration plan for existing
users.*
