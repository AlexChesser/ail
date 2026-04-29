## 34. The `ail agent-guide` Command

> **Implementation status:** Implemented in v0.0.4. `ail agent-guide`
> prints a curated CLAUDE.md / AGENTS.md snippet to stdout.

### 34.1 Purpose

Pipelines that include LLM-driven coding agents (Claude Code, Codex,
etc.) typically rely on a project-level `CLAUDE.md` or `AGENTS.md` file
to brief the agent on local conventions. When a project adopts `ail`,
the agent needs to know:

1. That `ail` exists and is the harness it is running inside.
2. How to discover the project's pipeline.
3. How to learn the AIL language before authoring or editing pipelines.
4. How to validate and inspect a pipeline change.

The `ail agent-guide` command emits a short, ready-to-paste markdown
snippet that covers all four. It is the bootstrap counterpart to
`ail init`: `init` scaffolds the pipeline files; `agent-guide`
scaffolds the agent's understanding of them.

### 34.2 CLI Command

```
ail agent-guide                          # default — lean snippet
ail agent-guide --format claudemd        # explicit name for the default
ail agent-guide --format agents-md       # alias for claudemd
ail agent-guide --format claudemd-full   # long-form briefing (opt-in)
ail agent-guide --format full            # alias for claudemd-full
```

Output goes to stdout and is pipeable. Canonical install:

```bash
ail agent-guide >> CLAUDE.md
```

Re-run after upgrading `ail` to refresh the snippet against the
current binary's UX.

### 34.3 Two Tiers — Lean Default, Full Opt-In

`agent-guide` mirrors `ail spec`'s tiered design and for the same
reason: anything pasted into a `CLAUDE.md` is paid for on **every
turn** the agent runs. The default cannot be expensive.

| Tier | Tokens | Contents | Use case |
|---|---|---|---|
| `claudemd` (default) | ~150-200 | Four or five commands in a table with one-line "when to use" notes; one sentence pointing at `ail spec --format compact` as the spec entry point. | The right thing to paste into nearly every project's `CLAUDE.md`. |
| `claudemd-full` | ~800 | Long-form briefing — what ail is, how the harness relates to authored steps, the validation loop, passthrough mode, when *not* to bypass ail, plus the same command table. | Teaching repos, READMEs, onboarding docs — anywhere the per-turn cost is justified by the audience. |

Both tiers point at `ail spec --format compact` (§31) as the canonical
authoring reference. The lean snippet additionally links to the full
form so a reader who needs more depth can find it.

### 34.4 Snippet Design Principles

The emitted snippets are curated by hand and embedded in the binary at
build time (`ail/src/agent_guide/{claudemd,claudemd-full}.md`,
included via `include_str!`). They deliberately:

- **Reference, do not duplicate.** Both snippets point at
  `ail spec --format compact` as the canonical authoring reference —
  they do not restate spec content. This keeps the snippet small and
  resists drift as the spec evolves.
- **Mark themselves as generated.** The first line is an HTML comment
  identifying the command that produced the snippet, so a user (or a
  future agent) knows it is safe to regenerate. The lean snippet's
  header also names the full-form opt-in for discoverability.
- **Target agent behaviour, not human reading.** The snippet tells
  the agent what to do (read the spec before authoring; run validate
  + materialize as the build/test cycle), not what `ail` *is* in
  marketing terms.

### 34.5 Test Contract

CI asserts the lean snippet:

1. Stays under ~1000 bytes (~250 tokens) — the cap that makes
   per-turn inclusion cheap enough.
2. Starts with the generated-by HTML comment marker.
3. Mentions `ail spec --format compact`, `ail validate`,
   `ail materialize`, and the `claudemd-full` opt-in.

CI asserts the full snippet:

1. Is at least 3× the size of the lean snippet (otherwise the split
   is not paying for itself).
2. Mentions `ail spec --format compact` and explains passthrough mode.

These checks live in `ail/src/agent_guide/mod.rs` (unit) and
`ail/tests/cli_agent_guide.rs` (end-to-end). They guard against
unintended bloat of the default tier and against drift in the long
form, without forcing a snapshot test that breaks on every prose
tweak.

### 34.6 Future Formats

The `--format` flag is in place specifically so additional flavours
can be added without renaming the command. Likely candidates:

- A localised variant for non-English agent prompts.
- A project-aware variant that detects the current `.ail.yaml` and
  names it directly in the snippet.

None are implemented in v0.0.4. The flag accepts `claudemd`
(+ aliases `agents-md`, `agentsmd`) and `claudemd-full`
(+ aliases `agents-md-full`, `full`) today.
