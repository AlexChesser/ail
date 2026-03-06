# AIL Runner Specification

> **ail** — Alexander's Impressive Loops

---

> ⚠️ **This document is a stub under active development.**
>
> The direction and intent are established. The contract details — specific flags, output formats, exit code semantics, capability declarations — will be defined through implementation experience with the Claude CLI proof of concept and expanded as additional runners are brought into scope.
>
> Do not implement against this document yet. Open a discussion issue if you are a CLI tool author interested in AIL compliance.

---

## Purpose

This document defines the **AIL Runner Contract** — the behavioural specification that a CLI tool must satisfy to be considered AIL-compliant and work with `ail`'s built-in generic adapter without requiring a custom implementation.

It is a companion to `SPEC.md`, which defines the pipeline language. `SPEC.md` is for pipeline authors. This document is for **CLI tool authors** who want their tool to work as a first-class `ail` runner.

For Rust developers writing custom runner adapters, see `ARCHITECTURE.md` *(forthcoming)*.

---

## Background: What `ail` Needs from a Runner

`ail` wraps a CLI tool and orchestrates a pipeline of follow-up prompts after each response. To do this reliably, `ail` needs to answer four questions about any runner it works with:

1. **How do I invoke it non-interactively?** — passing a prompt programmatically, not via a human typing at a terminal.
2. **Where does the response appear?** — stdout, a file, a structured format?
3. **How do I know it has finished?** — exit code, a delimiter, a timeout?
4. **What optional capabilities does it support?** — structured output, extended thinking, tool calls, context passing?

The contract defined in this document answers these questions in a way that any compliant runner implements consistently.

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

### Extended Compliance *(under design)*

An extended-compliant runner additionally declares optional capabilities, unlocking richer `ail` features. Capability declaration mechanism is to be defined — likely a `--ail-capabilities` flag that returns a structured JSON object.

Candidate optional capabilities:

- **Structured output** — the runner can return a JSON response conforming to a declared schema
- **Extended thinking** — the runner exposes reasoning traces separately from the final response
- **Tool call inspection** — the runner exposes tool invocations and results as structured data
- **Context passing** — the runner accepts a prior context object to maintain conversation state across invocations
- **Streaming** — the runner streams output as it is produced rather than returning it all at once

---

## Reference Implementation

The Claude CLI (`claude`) is the reference implementation for this specification. All contract decisions will be validated against Claude CLI behaviour first.

Relevant Claude CLI flags and behaviours to document:

- [ ] Non-interactive invocation mode
- [ ] Prompt passing mechanism
- [ ] Output format and streaming behaviour
- [ ] Exit code semantics
- [ ] Context/session handling
- [ ] Structured output support
- [ ] Extended thinking exposure

*This section will be populated during the v0.0.1 spike. See the spike notes in `SPIKE.md` (forthcoming).*

---

## Known Target Runners

The following CLI tools are on the roadmap for first-class `ail` support. Each will be assessed against the contract during their respective integration phases.

| Runner | Status | Notes |
|---|---|---|
| Claude CLI (`claude`) | **In progress** — v0.0.1 target | Reference implementation |
| Aider | Planned | |
| OpenCode | Planned | |
| Codex CLI | Planned | |
| Gemini CLI | Planned | |
| Qwen CLI | Planned | |
| DeepSeek CLI | Planned | |
| llama.cpp | Planned | Non-interactive mode needs investigation |

---

## Writing a Custom Adapter

If your CLI tool does not implement this contract, you can still integrate it with `ail` by writing a custom adapter in Rust. Adapters implement the `Runner` trait defined in `ail`'s core and are loaded at runtime as dynamic libraries.

See `ARCHITECTURE.md` *(forthcoming)* for the trait definition, dynamic loading system, and a worked example adapter.

---

## Open Questions

These questions must be answered before the contract can be considered stable.

- **Context passing** — how does `ail` pass the accumulated context of a session to a runner for each invocation? As a file path, an environment variable, a flag, or stdin alongside the prompt? The answer differs per runner and may need to be part of the contract.
- **Streaming vs batch** — should the minimum contract require streaming output, or is batch (full response on exit) sufficient? Streaming is better for the TUI experience but harder to implement correctly.
- **Exit code catalogue** — beyond 0/non-zero, should the contract define specific exit codes for specific error types (timeout, model error, context limit exceeded)?
- **Capability negotiation** — should `ail` query capabilities at session start and cache them, or query per invocation? Caching is faster; per-invocation is more correct if capabilities change.
- **Minimum flag set** — what is the smallest set of flags a runner must support to be considered minimally compliant? This needs to be validated against each target runner to ensure the bar is achievable.

---

*This is a stub. When this document reaches v0.1, it will be versioned and tagged alongside the main `SPEC.md`.*
