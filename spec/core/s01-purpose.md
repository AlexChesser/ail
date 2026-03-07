## 1. Purpose & Philosophy

Current agentic coding tools treat a human prompt as a single transactional event. If a developer wants a refactor or a security audit after code is generated, they must manually type the follow-up prompt every single time. This creates inconsistent quality and *prompt fatigue*.

**ail** introduces the **Deterministic Post-Processor**: a YAML-orchestrated pipeline runtime that ensures a specific, pre-determined chain of automated prompts fires after every human prompt — consistently, without manual intervention.

> **The Core Guarantee**
> For every completion event produced by an underlying agent, `ail` will begin executing the pipeline defined in the active `.ail.yaml` file before control returns to the human. Steps execute in order. Individual steps may be skipped by declared conditions or disabled explicitly. Execution may terminate early via `break`, `abort_pipeline`, or an unhandled error. All of these are explicit, declared outcomes — not silent failures. The human never receives runner output without the pipeline having had the opportunity to run.

The AIL Pipeline Language (APL) is the product. The orchestration engine is its runtime. Everything else — context distillation, learning loops, multi-model routing — are optional pipeline steps, not architectural prerequisites.

### The Two Layers

`ail` operates across two distinct layers that must never be confused:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime engine | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable.

---
