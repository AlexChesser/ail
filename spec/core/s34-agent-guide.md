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
ail agent-guide                          # default — prints CLAUDE.md snippet
ail agent-guide --format claudemd        # explicit name for the default
ail agent-guide --format agents-md       # alias for claudemd (same output)
```

Output goes to stdout and is pipeable, so the canonical install is:

```bash
ail agent-guide >> CLAUDE.md
```

Re-run after upgrading `ail` to refresh the snippet against the
current binary's UX.

### 34.3 Snippet Design Principles

The emitted snippet is curated by hand and embedded in the binary at
build time (`ail/src/agent_guide/claudemd.md`, included via
`include_str!`). It deliberately:

- **References, does not duplicate.** The snippet points at
  `ail spec --format compact` (§31) as the canonical authoring
  reference. It does not restate spec content. This keeps the snippet
  short (~800 tokens) and resists drift as the spec evolves.
- **Marks itself as generated.** The first two lines are HTML comments
  identifying the command that produced the snippet, so a user (or a
  future agent) knows it is safe to regenerate.
- **Targets agent behaviour, not human reading.** The snippet tells
  the agent what to do (read the spec before authoring; run validate
  + materialize as the build/test cycle), not what `ail` *is* in
  marketing terms.

### 34.4 Test Contract

CI asserts the snippet:

1. Is non-empty (> 500 bytes).
2. Starts with the generated-by HTML comment marker.
3. Mentions `ail spec --format compact`, `ail validate`,
   `ail materialize`, and explains passthrough mode.

These checks live in `ail/src/agent_guide/mod.rs`. They guard against
the snippet drifting out of sync with the canonical command surface
without forcing a snapshot test that breaks on every prose tweak.

### 34.5 Future Formats

The `--format` flag is in place specifically so additional flavours
can be added without renaming the command. Likely candidates:

- A long-form variant suitable for project READMEs.
- A localised variant for non-English agent prompts.
- A project-aware variant that detects the current `.ail.yaml` and
  names it directly in the snippet.

None are implemented in v0.0.4. The flag accepts only `claudemd` and
`agents-md` (alias) today.
