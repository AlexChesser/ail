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

### Extended Compliance

An extended-compliant runner implements the structured bidirectional JSON interface, unlocking the full `ail` feature set. The Claude CLI is the reference implementation.

Extended compliance requires:

- **`--output-format stream-json`** — NDJSON event stream with typed events for tool calls, tool results, text, and completion
- **`--input-format stream-json`** — accept follow-up NDJSON messages on stdin for session continuity
- **`--permission-prompt-tool stdio`** — HITL tool permission intercept via stdin/stdout JSON protocol
- **`--allowedTools` / `--disallowedTools`** — pre-approved and pre-denied tool patterns
- **`--dangerously-skip-permissions`** — headless/automated mode bypass

Optional extended capabilities (declare via `--ail-capabilities` — mechanism to be defined):

- **Extended thinking** — exposes reasoning traces as typed events in the stream
- **Structured output** — constrains final response to a JSON schema (`--json-schema`)
- **Session resumption** — `session_id` from result events can be passed back to resume a prior session

---

## Reference Implementation — Claude CLI

The Claude CLI (`claude`) is the reference implementation for this specification. It is the only first-class runner in v0.0.1. All contract decisions are validated against Claude CLI behaviour first.

### Invocation Model

The Claude CLI supports a structured bidirectional JSON interface that `ail` uses instead of PTY wrapping:

| Direction | Flag | Format |
|---|---|---|
| Output from Claude | `--output-format stream-json` | NDJSON event stream on stdout |
| Input to Claude | `--input-format stream-json` | NDJSON messages on stdin |
| Prompt (non-interactive) | `-p "<prompt>"` or `--print "<prompt>"` | Plain string |

### Output Event Stream

`--output-format stream-json` produces a newline-delimited stream of JSON events. Key event types:

```json
// Session initialised
{ "type": "system", "subtype": "init", "session_id": "abc123", ... }

// Assistant tool call
{ "type": "assistant", "message": {
    "content": [{ "type": "tool_use", "id": "toolu_abc", "name": "Write",
                  "input": { "file_path": "./foo.txt", "content": "..." } }]
}}

// Tool result fed back
{ "type": "user", "message": {
    "content": [{ "type": "tool_result", "tool_use_id": "toolu_abc", ... }]
}}

// Run complete
{ "type": "result", "subtype": "success",
  "result": "<final text response>",
  "total_cost_usd": 0.003,
  "session_id": "abc123" }

// Run failed
{ "type": "result", "subtype": "error", "error": "...", "session_id": "abc123" }
```

**Completion signal:** `ail` considers a Claude CLI invocation complete when it receives a `result` event. `subtype: success` → pipeline step succeeded. `subtype: error` → `on_error` handling fires.

**Cost tracking:** `total_cost_usd` in the result event feeds directly into `ail/budget-gate` without any external token counting.

### Tool Permission Interface

When `ail` needs to intercept tool permissions (for tools not covered by `tools.allow` or `tools.deny`), it launches Claude CLI with:

```
--permission-prompt-tool stdio
```

Claude emits a permission request event on the NDJSON stream when it wants to invoke a tool requiring authorisation. `ail` reads the event, presents the HITL UI, and writes one of these responses to the Claude CLI process stdin:

```json
{ "behavior": "allow" }

{ "behavior": "deny", "message": "User rejected" }

{ "behavior": "allow", "updatedInput": { ...modified tool input... } }
```

The `updatedInput` form allows `ail` to present an inline editor — the human corrects a file path, removes a sensitive argument, or adjusts a command — and Claude executes the corrected version rather than its original parameters.

#### Permission Modes

Claude CLI supports six permission modes via `--permission-mode`:

| Mode | Behaviour |
|---|---|
| `default` | Checks `settings.json`, `--allowedTools`, `--disallowedTools`, then calls `--permission-prompt-tool` for anything unresolved |
| `accept_edits` | Auto-accepts file edits; prompts for other tool types |
| `plan` | Read-only; no file modifications or commands |
| `bypass_permissions` | No permission checks at all (equivalent to `--dangerously-skip-permissions`) |
| `delegate` | Delegates permission decisions to the MCP tool specified |
| `dont_ask` | Auto-accepts everything without prompting |

`ail` defaults to `default` mode. For headless/automated runs (Docker sandbox, CI), use `bypass_permissions` or `--dangerously-skip-permissions`. `ail` exposes this as a session-level CLI flag, never as a pipeline YAML option.

#### `PreToolUse` Hook (Alternative Intercept)

As an alternative to `--permission-prompt-tool`, Claude CLI supports a `PreToolUse` hook — a process `ail` registers that runs synchronously after Claude creates tool parameters but before the tool executes. The hook receives `tool_name`, `tool_input`, and `tool_use_id` and can allow, deny, or modify the call.

This is more suitable for automated validation (schema checking, path sanitisation) than for interactive HITL — the hook runs as a subprocess without a human UI. It is noted here for completeness; `ail`'s primary HITL mechanism remains `--permission-prompt-tool stdio`.

> **Spike validation required:** Confirm that `--permission-prompt-tool stdio` behaves correctly when combined with `-p` (non-interactive mode). The VSCode extension uses this combination in interactive mode; `ail`'s usage differs. Document actual permission event shapes from the NDJSON stream.

### Pre-Approved Tool Policy

`tools.allow` and `tools.deny` in the pipeline step are passed to Claude CLI as:

```
--allowedTools Read,Edit,Glob
--disallowedTools WebFetch,Bash
```

Pattern syntax (e.g. `Bash(git log*)`, `Edit(./src/*)`) is passed verbatim — `ail` does not parse or validate patterns.

### Context and Session Continuity

The `session_id` returned in each result event is retained by `ail` for the duration of the session. Whether it can be used to resume a session across separate subprocess invocations is to be validated in the v0.0.1 spike.

`--input-format stream-json` supports sending follow-up messages within an active session. Whether `ail` uses this for pipeline step continuity (vs. spawning a new process per step with context injected via template variables) is a spike decision.

### Flags Summary

| Flag | Purpose | `ail` usage |
|---|---|---|
| `--output-format stream-json` | Structured NDJSON event stream | Always |
| `--input-format stream-json` | Accept NDJSON messages on stdin | When session continuation needed |
| `-p / --print` | Non-interactive prompt | Single-turn steps |
| `--permission-prompt-tool stdio` | HITL tool permission intercept | When step has unspecified tools |
| `--allowedTools` | Pre-approve tools | From `tools.allow` |
| `--disallowedTools` | Pre-deny tools | From `tools.deny` |
| `--permission-mode` | Set permission enforcement level | Session-level; `default` unless overridden |
| `--dangerously-skip-permissions` | Bypass all permission checks | Headless/automated mode only |
| `--verbose --include-partial-messages` | Token-level streaming | Observability / debugging |

*Spike validation status: pending. This section reflects current understanding from CLI reference documentation. Actual behaviour must be verified in the v0.0.1 spike and this section updated with findings.*

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

- **Session continuity** — does `--input-format stream-json` support sending a new pipeline step prompt within an active Claude CLI session, or does each pipeline step require a new subprocess invocation? This determines whether context is maintained natively or via `ail`'s template variable injection. Spike validation required.
- **Session resumption** — can `session_id` from a result event be passed back to Claude CLI to resume a prior session? If so, `ail` could maintain session state across user prompts. Spike validation required.
- **Minimum flag set for non-Claude runners** — beyond the minimum compliance tier, what is the smallest set of behaviours any runner must implement? Needs to be validated against each target runner (Aider, Gemini CLI, etc.) to ensure the bar is achievable without the stream-json interface.
- **Capability declaration mechanism** — the `--ail-capabilities` flag is proposed but not yet defined. What format should it return? What capabilities must be declared vs. assumed?
- **Error event shapes** — the `result.subtype: error` event needs a defined set of error codes so `ail`'s `on_error` handling can distinguish timeout, model error, context limit exceeded, and permission denied.
- **`--permission-prompt-tool stdio` in non-interactive mode** — verify that the HITL permission intercept works correctly when combined with `-p` (non-interactive prompt flag). The VSCode extension uses it in interactive mode; `ail`'s usage pattern may differ.

---

*This is a stub. When this document reaches v0.1, it will be versioned and tagged alongside the main `SPEC.md`.*
