# AskUserQuestion: Path Forward Without MCP Bridge

**Date:** 2026-04-05  
**Context:** Assessment of whether AIL can handle `AskUserQuestion` without the MCP bridge, and
whether there is a path that delivers rich button chrome in the vscode-ail-chat extension.

---

## Current Architecture

When Claude wants to ask the user a question, it calls the built-in `AskUserQuestion` tool. In
AIL's current flow:

```
Claude (AskUserQuestion tool_use)
  └─► ail mcp-bridge (stdio MCP server, spawned by Claude via --mcp-config)
        └─► AIL Unix socket (write question JSON)
              └─► VSCode extension webview (render question)
              ◄── user clicks button
        ◄── AIL socket (return answer JSON)
  ◄── MCP bridge returns answer as tool result
Claude continues with the answer
```

Implementation pieces:
- `ClaudeCliRunner::write_mcp_config()` — writes a temp `mcp-config.json` pointing to `ail mcp-bridge --socket <path>` on every invoke
- `ail/src/mcp_bridge.rs` — JSON-RPC 2.0 MCP server over stdio; intercepts `ail_ask_user` calls and forwards to the Unix socket
- `ail/src/ask_user_types/` — normalises the 4+ payload shapes Claude produces
- `ail-core/src/ipc.rs` — cross-platform Unix socket / named pipe helpers

The MCP bridge exposes its own `ail_ask_user` MCP tool. Claude's system prompt (injected by AIL or
by the bridge's tool description) instructs Claude to call `ail_ask_user` instead of the native
`AskUserQuestion`. This is why there are 4 payload-shape parsers — different model versions ignore
the instruction and emit different formats anyway.

### Known friction

- A new MCP subprocess is spawned for every `claude -p` invocation; JSON-RPC 2.0 over stdio adds
  latency.
- The `--mcp-config` temp file must be written and cleaned up on every run.
- Claude's compliance with "use `ail_ask_user` not `AskUserQuestion`" is model-dependent; the
  normaliser is a workaround for non-compliance.
- The VSCode extension is not rendering questions with the desired button chrome — this is a
  **separate rendering problem** (see below), not caused by the MCP bridge itself.

---

## What the CLI Reference and Hooks Doc Reveal

### `PreToolUse` hooks fire for `AskUserQuestion`

The hooks reference explicitly lists `AskUserQuestion` as a hookable tool name. A `PreToolUse`
hook receives the question payload in `tool_input` and can:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": {
      "questions": [...],
      "answers": { "Which framework?": "React" }
    }
  }
}
```

This is a **first-class, officially supported** mechanism to satisfy `AskUserQuestion` from outside
Claude — no MCP server required.

### `"defer"` exits the process with a payload (non-interactive only)

In `-p` (print/headless) mode, a `PreToolUse` hook can return `"permissionDecision": "defer"`.
Claude CLI then exits with `stop_reason: "tool_deferred"` in its stream-json output, including a
`deferred_tool_use` payload that contains the complete tool call. The calling process surfaces its
UI, collects the answer, then resumes with `--resume <session_id>` and a hook that injects the
answer via `updatedInput`.

---

## Two Viable Hook-Based Paths

### Path A — Hook-and-Wait (drop-in replacement for MCP bridge)

```
Claude (AskUserQuestion tool_use)
  └─► PreToolUse hook script (registered in settings.json, spawned by Claude)
        └─► AIL Unix socket (write question JSON, block for answer)
              └─► VSCode extension webview (render question with chrome)
              ◄── user clicks button
        ◄── AIL socket (answer JSON)
  ◄── hook exits 0 with "allow" + updatedInput containing the answer
Claude continues
```

**What changes vs. today:**
- No MCP subprocess; hooks are registered in `settings.json` (or `--settings`), not a temp MCP
  config file.
- The hook binary is a simple socket client — far simpler than a JSON-RPC 2.0 MCP server.
- Claude's native `AskUserQuestion` is intercepted; no need to instruct Claude to call a different
  tool. The payload normaliser in `ask_user_types/` can move into the hook binary.
- Hook has no fixed timeout in the docs, but it is a blocking subprocess. Long user pauses are
  fine in practice; the hook holds Claude's turn open.

**Risks:**
- Hook timeout behavior is undocumented. Very long pauses (minutes) could be an issue on some
  Claude CLI versions.
- AIL must write (or pre-configure) the `settings.json` hook entry before spawning `claude -p`.
  This is the same bootstrapping problem as the MCP config file — different format, similar effort.

### Path B — Defer and Resume (cleanest for VSCode chrome)

```
Claude (AskUserQuestion tool_use)
  └─► PreToolUse hook: returns "defer"
      Claude CLI exits with stop_reason:"tool_deferred" + deferred_tool_use in stream-json
AIL parses deferred_tool_use from stream-json output
  └─► VSCode extension webview (render question with chrome — no time pressure!)
      ◄── user clicks button
AIL writes answer to a temp file / env var
AIL resumes: claude --resume <session_id> --settings <temp-settings-with-answer-hook>
  └─► PreToolUse hook (one-shot): reads answer from temp file, returns "allow" + updatedInput
Claude continues
```

**Advantages for VSCode chrome:**
- AIL has the question data as a clean, typed stream-json event — no socket forwarding needed in
  the critical path.
- The extension has unlimited time to render richly; no blocking subprocess is holding a timeout.
- The entire user-interaction phase is owned by AIL, not a subprocess of Claude.
- Answer injection is explicit and auditable (temp file or env var → hook → `updatedInput`).

**Risks and unknowns:**
- **Unverified:** does `deferred_tool_use` actually appear in `-p --output-format stream-json`
  output? The hooks guide describes it but the stream-json event catalog doesn't list it. Needs
  empirical testing before committing.
- The resume hook must be "one-shot" — it answers exactly this deferred call and then gets out of
  the way. Writing a per-resume temp settings file solves this but adds a second bootstrapping step.
- Two `claude` invocations per `AskUserQuestion`: the original run (that defers) and the resume.
  This is a real latency cost but probably acceptable.

---

## The VSCode Extension Chrome Problem Is Separate

Neither Path A nor Path B automatically fixes button rendering in the extension. The rendering
problem is in the extension's WebView, not in the transport. The data already reaches the
extension via the Unix socket today (otherwise the question would never appear at all).

Most likely causes for missing chrome:
1. The WebView message handler receives the question JSON but the component isn't re-rendering
   (React state not updated, or message posted before the frame is ready).
2. The button options array is nested differently than the renderer expects. The `ask_user_types`
   normaliser in Rust produces `{ questions: [{ header, question, multiSelect, options: [{ label,
   description? }] }] }` — the WebView must expect exactly this shape.
3. CSS for the button chrome exists but the conditional that shows buttons vs. plain text isn't
   triggered.

These need to be debugged in the extension independently. Path B makes this easier because AIL
owns the full interaction lifecycle and can log the exact JSON being sent.

---

## Recommendation

**Short term:** Path A is a low-risk migration from the MCP bridge. It removes the MCP subprocess
and the temp config file, and uses Claude's official hook mechanism for `AskUserQuestion`. The
socket/IPC layer is unchanged so the extension debugging can proceed in parallel.

**Medium term:** Path B, once the defer payload format is verified empirically. It gives AIL full
ownership of the HITL loop, enables richer extension UI with no time pressure, and makes the
entire flow easier to test and reason about.

**For the extension chrome independently:** instrument the WebView message handler to log what it
receives when a question arrives and compare it against what the component expects. That gap is
where the missing buttons live.

---

## Open Questions Before Implementing Path B

1. Does `stop_reason: "tool_deferred"` appear in `--output-format stream-json` output, or only in
   some other format? Run a test with a hook that returns `"defer"` on a known AskUserQuestion
   call and capture the raw NDJSON.
2. What is the exact shape of `deferred_tool_use` in the stream-json event?
3. When resuming, does the hook fire again for the same `tool_use_id`, or does Claude expect
   `updatedInput` through a different mechanism (e.g., piped stdin via `--input-format stream-json`)?
4. Is there a hook timeout? Under what conditions does Claude CLI kill a slow PreToolUse hook?

These questions can be answered with a focused spike (30–60 minutes, no AIL code changes needed —
just a shell hook script and `claude -p`).
