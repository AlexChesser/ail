# AIL Runner Specification

> **ail** — Alexander's Impressive Loops

---

> ⚠️ **This document is a stub under active development.**
>
> The direction and intent are established. The contract details — specific flags, output formats, exit code semantics, capability declarations — will be defined through implementation experience with the Claude CLI proof of concept and expanded as additional runners are brought into scope.
>
> Do not implement against this document yet. Open a discussion issue if you are a CLI tool author interested in AIL compliance.

---

---

## Purpose

This document defines the **AIL Runner Contract** — the behavioural specification that a CLI tool must satisfy to be considered AIL-compliant and work with `ail`'s built-in generic adapter without requiring a custom implementation.

It is a companion to `SPEC.md`, which defines the pipeline language. `SPEC.md` is for pipeline authors. This document is for **CLI tool authors** who want their tool to work as a first-class `ail` runner.

For Rust developers writing custom runner adapters, see `ARCHITECTURE.md` *(forthcoming)*.

---

---

## Background: What `ail` Needs from a Runner

`ail` wraps a CLI tool and orchestrates a pipeline of follow-up prompts after each response. To do this reliably, `ail` needs to answer four questions about any runner it works with:

1. **How do I invoke it non-interactively?** — passing a prompt programmatically, not via a human typing at a terminal.
2. **Where does the response appear?** — stdout, a file, a structured format?
3. **How do I know it has finished?** — exit code, a delimiter, a timeout?
4. **What optional capabilities does it support?** — structured output, extended thinking, tool calls, context passing?

The contract defined in this document answers these questions in a way that any compliant runner implements consistently.

---

---

## Compliance Tiers

### Minimum Compliance

A minimally compliant runner must:

- [ ] Accept a prompt in non-interactive mode via a flag or stdin
- [ ] Write its complete response to stdout
- [ ] Write errors to stderr
- [ ] Exit with code `0` on success
- [ ] Exit with a non-zero code on error, with a human-readable message on stderr

A minimum-compliant runner works with all text-based `ail` pipeline features: `prompt:` steps, `on_result:` matching, `condition:` evaluation, HITL gates, and template variable injection.

> **Note:** `context:` steps (`shell:`, `mcp:`) are executed directly by `ail` and do not pass through the runner. A minimum-compliant runner is sufficient for pipelines that include `context:` steps — the runner handles only `prompt:` and `skill:` steps.

### Extended Compliance

An extended-compliant runner implements the structured bidirectional JSON interface, unlocking the full `ail` feature set. The Claude CLI is the reference implementation.

Extended compliance requires:

- **`--output-format stream-json`** — NDJSON event stream with typed events for tool calls, tool results, text, and completion
- **`--mcp-config` + `--permission-prompt-tool mcp__<server>__<tool>`** — HITL tool permission intercept via MCP bridge subprocess + Unix domain socket
- **`--allowedTools` / `--disallowedTools`** — pre-approved and pre-denied tool patterns
- **`--dangerously-skip-permissions`** — headless/automated mode bypass

> **Session continuity:** `ail` uses `--resume <session_id>` per step rather than `--input-format stream-json`. The `--resume` approach spawns one subprocess per pipeline step and passes the session ID from the previous step's `result` event. `--input-format stream-json` is not a compliance requirement — `ail` does not use it. It may become relevant for real-time HITL injection in a future version.

Optional extended capabilities (declare via `--ail-capabilities` — mechanism to be defined):

- **Extended thinking** — exposes reasoning traces as typed events in the stream
- **Structured output** — constrains final response to a JSON schema (`--json-schema`)
- **Session resumption** — `session_id` from result events can be passed back to resume a prior session

---
