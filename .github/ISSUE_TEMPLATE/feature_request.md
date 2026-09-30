## What are you trying to do?

<!--
Describe the outcome you want, not the interface you imagine.
"I want to see which of my containers have been stopped for more than a week" is
useful. "Add a --stale flag" is a solution offered before we know the problem.
-->

## Why the current tool does not work

<!--
What do you do today, and where does it break?

This is the most valuable part of the report. A workaround you invented is
usually a better feature than the one you asked for.
-->

## Proposed solution

<!-- Optional. A sketch is fine; a patch is better. -->

## Which phase does this belong to?

<!-- Check docs/ROADMAP.md first. -->

- [ ] It is already in the roadmap (which phase: ____)
- [ ] It extends an existing phase (which: ____)
- [ ] It needs a new phase
- [ ] I have not checked

## Alternatives you considered

<!-- Did you solve this another way? What did you use instead? -->

## Scope check

gcode is local-first and offline, with no API keys and no plugin system before
v1.0. Before we accept a request, we check it against those.

- [ ] This works offline, with no network after the model download
- [ ] This needs no account, no key, and no service
- [ ] This does not weaken the safety model (no way to run a `CRITICAL` command)
- [ ] This does not require a new dependency, or I am willing to justify one

## Checklist

- [ ] I searched existing issues and discussions
- [ ] This is not a duplicate
- [ ] I understand gcode is pre-1.0 and the CLI surface may change
