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

---

## Writing a Custom Adapter

If your CLI tool does not implement this contract, you can still integrate it with `ail` by writing a custom adapter in Rust. Adapters implement the `Runner` trait defined in `ail`'s core and are loaded at runtime as dynamic libraries.

See `ARCHITECTURE.md` *(forthcoming)* for the trait definition, dynamic loading system, and a worked example adapter.

---

---

## Open Questions

These questions must be answered before the contract can be considered stable.

- ~~**Session continuity**~~ — **Resolved.** Each pipeline step uses a new subprocess with `--resume <session_id>`. `--input-format stream-json` is not used.
- ~~**Session resumption**~~ — **Resolved.** `session_id` from the `result` event is passed as `--resume <session_id>` to subsequent steps. Full conversation history is preserved.
- **Minimum flag set for non-Claude runners** — beyond the minimum compliance tier, what is the smallest set of behaviours any runner must implement? Needs to be validated against each target runner (Aider, Gemini CLI, etc.) to ensure the bar is achievable without the stream-json interface.
- **Capability declaration mechanism** — the `--ail-capabilities` flag is proposed but not yet defined. What format should it return? What capabilities must be declared vs. assumed?
- **Error event shapes** — the `result.subtype: error` event needs a defined set of error codes. Three categories are needed for `on_error` differentiation:
  - `runner_timeout` — step exceeded `timeout_seconds`; candidate for retry
  - `runner_error` — model/provider failure (context limit, rate limit, internal error); may be retried
  - `runner_permission_denied` — tool call blocked by permission check; retry is not useful
  The current implementation maps all three to a single `RUNNER_INVOCATION_FAILED` type. Differentiation is deferred to v0.1+.
- **Permission HITL in non-interactive mode (v0.1 implemented, validated)** — `--permission-prompt-tool stdio` does NOT work with `-p`. The correct approach is `--mcp-config <path> --permission-prompt-tool mcp__ail-permission__ail_check_permission`. This has been validated and implemented in v0.1. Headless runs bypass via `--dangerously-skip-permissions`.
- **`--verbose` requirement stability** — `--output-format stream-json` requires `--verbose` when combined with `-p`. This is undocumented in Claude CLI's `--help`. A compatibility test should verify this requirement still holds before each Claude CLI version upgrade.

---

*This is a stub. When this document reaches v0.1, it will be versioned and tagged alongside the main `SPEC.md`.*
