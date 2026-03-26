## 1. Purpose & Philosophy

LLM agents fail in predictable ways: perseveration, goal substitution, source monitoring failure, anosognosia. These are the behavioral profile of a system with capable reasoning and absent executive control. Cognitive scientists named this cluster Dysexecutive Syndrome in 1986. Working memory maintenance, inhibitory control, and metacognitive monitoring — the capacities the clinical literature identifies as missing — do not emerge from within the model. They have to be built on top of it.

`ail` is that layer. It is a YAML-orchestrated pipeline that fires a declared sequence of behaviors after every agent invocation, before control returns to the human.

> **The Core Guarantee**
> For every completion event produced by an underlying agent, `ail` will begin executing the pipeline defined in the active `.ail.yaml` file before control returns to the human. Steps execute in order. Individual steps may be skipped by declared conditions or disabled explicitly. Execution may terminate early via `break`, `abort_pipeline`, or an unhandled error. All of these are explicit, declared outcomes — not silent failures. The human never receives runner output without the pipeline having had the opportunity to run.

An instruction inside a context window is subject to everything else in that window. Sessions grow. Tool calls accumulate. Earlier instructions drift toward the middle of the context where the attention mechanism is weakest — the lost-in-the-middle effect is well-documented and consistent across frontier models. The carefully written `CLAUDE.md` entry becomes one voice in a crowd.

`ail` moves the behavior out of the context entirely. A pipeline step fires because it was declared, not because the model remembered to do it. The linter runs. The security review runs. The self-evaluation step runs. None of them depend on what ended up in the context window.

This is the executive function layer. Working memory maintenance, inhibitory control, and metacognitive monitoring — the cognitive capacities that LLMs structurally lack — are externalised into a file that lives in the repository, runs on every invocation, and is independent of session state. A `CLAUDE.md` entry is a request. An `ail` step is a guarantee.

### Scope Discipline

`ail` is the artificial neocortex — the executive function layer that LLM agents structurally lack. The compass for every implementation decision is: *does this serve the frontal lobe?*

A feature belongs in `ail` if it:
- **Addresses one of the four failure modes** — perseveration, goal substitution, source monitoring failure, or anosognosia; or
- **Strengthens one of Diamond's three executive function components** — inhibitory control, working memory updating, or cognitive flexibility; or
- **Extends `ail`'s capacity to select, compose, or improve its own pipelines** — the supervisory attentional layer that decides which script to run, not just that it runs.

The third category is the long arc. Norman and Shallice's Supervisory Attentional System sits above contention scheduling — it intervenes when tasks are novel, ambiguous, or require overriding a habitual response. Today `ail` is the contention scheduler: it executes the declared pipeline. The trajectory is toward the SAS: `ail` selecting and composing pipelines appropriate to the task. Features that serve general task execution without mapping to any of these three categories belong to the agent layer beneath `ail`, not to the control plane above it. See §22 for the planned trajectory.

### The Two Layers

`ail` operates across two distinct layers that must never be confused:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime engine | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable.

---
