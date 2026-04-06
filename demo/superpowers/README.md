# Superpowers as AIL Pipelines

This directory reproduces [obra/superpowers](https://github.com/obra/superpowers) skills as AIL pipeline workflows, demonstrating where deterministic pipeline orchestration adds value and where system prompt injection is the better approach.

See [docs/research/superpowers-as-pipelines.md](../../docs/research/superpowers-as-pipelines.md) for the full feasibility analysis.

## Key Insight: Two Categories

Superpowers fall into two categories that demand different treatment:

**Category A: Behavioral Disciplines** (TDD, verification, debugging) — These shape how the LLM *thinks*. They belong in `system_prompt:` / `append_system_prompt:` on pipeline steps, not as separate pipeline phases. Decomposing them would lose the tight feedback loops they depend on.

**Category B: Sequential Workflows** (brainstorming, planning, execution, review) — These have clear phase boundaries with distinct artifacts flowing between phases. These are genuine pipeline candidates.

## Pipeline Status

| Pipeline | Source Superpower | Status |
|---|---|---|
| `finishing-branch.ail.yaml` | finishing-a-development-branch | **Works today** |
| `code-review.ail.yaml` | requesting-code-review | **Works today** |
| `code-review-sub.ail.yaml` | (code-reviewer agent) | **Works today** |
| `writing-plans.ail.yaml` | writing-plans | **Works today** |
| `tdd-enriched.ail.yaml` | TDD + verification (Category A) | **Works today** |
| `git-worktree-setup.ail.yaml` | using-git-worktrees | **Works today** |
| `brainstorming.ail.yaml` | brainstorming | Partial (`pause_for_human` no-op in `--once`) |
| `executing-plans.ail.yaml` | executing-plans | Partial (no looping construct) |
| `subagent-development.ail.yaml` | subagent-driven-development | Mostly proposed (needs looping + parallel) |
| `parallel-debug.ail.yaml` | dispatching-parallel-agents | Mostly proposed (needs parallel execution) |

## Running the Demos

```bash
# Validate a pipeline
cargo run -- validate --pipeline demo/superpowers/finishing-branch.ail.yaml

# Inspect resolved YAML
cargo run -- materialize --pipeline demo/superpowers/code-review.ail.yaml

# Run a pipeline (requires claude CLI and release build)
cd your-project && /path/to/ail "your prompt" --pipeline /path/to/demo/superpowers/tdd-enriched.ail.yaml
```

## Category A: Disciplines as System Prompts

The TDD and verification superpowers are not pipelines — they're behavioral disciplines injected via `append_system_prompt:` on any step that writes code:

```yaml
- id: implement_feature
  prompt: "Implement the requested feature."
  append_system_prompt:
    - file: ./prompts/tdd-discipline.md
    - file: ./prompts/verification-discipline.md
```

See `tdd-enriched.ail.yaml` for a complete example.

## PROPOSED Features

Steps marked with `# PROPOSED:` comments in the YAML files use features that are not yet available in AIL:

- **Looping / iteration** — `iterate:` / `for_each:` construct for executing tasks from a plan one at a time. Not yet specced.
- **Parallel execution** — `parallel:` block for concurrent step execution. Specced in SPEC S21 but design is incomplete.

## Prompt Files

The `prompts/` directory contains system prompt content extracted and adapted from the superpowers SKILL.md files:

| File | Source | Used By |
|---|---|---|
| `tdd-discipline.md` | test-driven-development SKILL.md | `tdd-enriched.ail.yaml`, `executing-plans.ail.yaml` |
| `verification-discipline.md` | verification-before-completion SKILL.md | Multiple pipelines |
| `code-reviewer-system.md` | agents/code-reviewer.md | `code-review-sub.ail.yaml` |
| `writing-plans-system.md` | writing-plans SKILL.md | `writing-plans.ail.yaml` |
| `writing-plans-decompose.md` | writing-plans SKILL.md | `writing-plans.ail.yaml` |
| `brainstorming-system.md` | brainstorming SKILL.md | `brainstorming.ail.yaml` |
| `finishing-branch-system.md` | finishing-a-development-branch SKILL.md | `finishing-branch.ail.yaml` |
