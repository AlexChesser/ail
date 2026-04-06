# Superpowers as AIL Pipelines: Feasibility Analysis

**Date:** 2026-04-06
**Source:** [obra/superpowers](https://github.com/obra/superpowers)
**Status:** Research complete; Phase 1 demos implemented

---

## Summary

[obra/superpowers](https://github.com/obra/superpowers) is a collection of 13 Claude Code "skills" — markdown files injected into system prompts to guide agent behavior through structured development workflows. This document analyzes whether these can be reproduced as deterministic AIL pipelines, and whether doing so is beneficial.

**Key finding:** Superpowers fall into two fundamentally different categories that demand different treatment. Forcing everything into pipelines would be counterproductive.

---

## Two Categories of Superpowers

### Category A: Behavioral Disciplines (system prompt injection)

These shape *how* the LLM thinks — they're guardrails and checklists baked into the model's context. They don't describe a sequence of automated steps; they describe *mindset*.

- **test-driven-development** — RED-GREEN-REFACTOR cycle. The LLM must internalize this as a way of working. The "steps" (write test, run, write code, run, refactor) happen *within* a single LLM turn, driven by tool use. A pipeline can't usefully split these because the LLM needs to see test output and react.
- **verification-before-completion** — "Run the command, read the output, THEN claim success." This is a behavioral constraint, not a pipeline.
- **systematic-debugging** — 4-phase process, but each phase requires iterative tool use, reading output, forming hypotheses. The phases are guidelines for reasoning, not automatable steps.
- **using-superpowers** — Meta-skill: "check skills before acting." Pure system prompt content.
- **writing-skills** — Meta-skill for creating new skills. Not a pipeline.

**Verdict:** These should be `system_prompt:` or `append_system_prompt:` content on steps, NOT broken into separate pipeline steps. Decomposing them would actually *hurt* — the LLM needs holistic context to apply these disciplines throughout its work, not in isolated prompt windows.

### Category B: Sequential Workflows (pipeline candidates)

These describe a clear progression of distinct phases where output from one phase feeds the next, and the phases are naturally independent.

- **brainstorming** -> **writing-plans** -> **executing-plans** -> **finishing-a-development-branch** (the "full lifecycle")
- **requesting-code-review** / **receiving-code-review** (review dispatch)
- **subagent-driven-development** (per-task dispatch with review stages)
- **dispatching-parallel-agents** (concurrent investigation)
- **using-git-worktrees** (workspace setup)

**Verdict:** These are genuine pipeline candidates — each phase produces a distinct artifact that feeds the next, and the phases benefit from focused prompting.

---

## Classification Matrix

| Superpower | Category | Pipeline? | Current AIL Support | Benefit of Decomposition |
|---|---|---|---|---|
| test-driven-development | A: Discipline | `system_prompt` | Yes (as prompt content) | **Low** — needs holistic context |
| verification-before-completion | A: Discipline | `system_prompt` | Yes (as prompt content) | **Low** — behavioral constraint |
| systematic-debugging | A: Discipline | `system_prompt` | Yes (as prompt content) | **Low** — iterative reasoning |
| using-superpowers | A: Meta | N/A | N/A | N/A |
| writing-skills | A: Meta | N/A | N/A | N/A |
| brainstorming | B: Workflow | **Yes** | Partial | **High** — clear phase gates |
| writing-plans | B: Workflow | **Yes** | Yes | **High** — distinct output artifact |
| executing-plans | B: Workflow | **Yes** | Partial | **Medium** — needs iteration |
| finishing-a-development-branch | B: Workflow | **Yes** | Yes | **High** — deterministic checks + LLM |
| requesting-code-review | B: Workflow | **Hybrid** | Partial | **Medium** — subagent dispatch |
| receiving-code-review | B: Workflow | **Hybrid** | Partial | **Medium** — needs review input |
| subagent-driven-development | B: Workflow | **Blocked** | No (needs parallel) | **High** but blocked |
| dispatching-parallel-agents | B: Workflow | **Blocked** | No (needs parallel) | **High** but blocked |
| using-git-worktrees | B: Workflow | **Yes** | Yes (shell context) | **Medium** — mostly shell commands |

---

## The Science: Does Pipeline Decomposition Help?

### Yes, for Category B workflows

Research on prompt chaining and task decomposition (2024-2026) shows:

- **~15.6% accuracy improvement** from prompt chaining vs monolithic prompts
- **Error isolation:** a failed review step doesn't pollute the implementation context
- **Auditability:** the turn log captures each phase's input/output independently
- **Cost optimization:** context steps (`shell:`) use zero tokens; verification steps can use cheaper models
- **Reduced hallucinations:** well-defined subtasks reduce hallucination rates

### No, for Category A disciplines

Breaking TDD into pipeline steps would:

- **Lose the tight feedback loop** — write test, see failure, write code, see pass must happen in one LLM context with tool use
- **Add 35-600% token overhead** from re-establishing context at each step
- **Remove holistic reasoning** needed to apply discipline *throughout* work, not just at phase boundaries

### The hybrid approach is correct

Use pipelines for **orchestration** (sequence of phases). Use system prompts for **behavioral discipline** within each phase. This matches the emerging best practice of deterministic orchestration with adaptive LLM reasoning within each step.

Sources:
- Prompt Chaining for AI Engineers (getmaxim.ai, 2024)
- Hierarchical Chain-of-Thought Prompting (arXiv:2604.00130, 2025)
- Least-to-Most Prompting Enables Complex Reasoning (OpenReview, 2023)
- A Practical Guide for Agentic AI Workflows (arXiv:2512.08769, 2025)
- The Decreasing Value of Chain of Thought (Wharton, 2025)

---

## Spec Gaps Identified

### Gap 1: No Looping / Iteration Construct (UNSPECCED)

**Impact:** Cannot express "for each task in plan, dispatch + review + fix". Blocks `executing-plans` (iterate over tasks) and `subagent-driven-development` (per-task loop).
**Severity:** High — the single biggest gap.
**Workaround:** Unroll loops manually for known task counts, but this defeats the purpose.

### Gap 2: No Parallel Execution (SPECCED, NOT IMPLEMENTED)

**Impact:** Blocks `dispatching-parallel-agents` entirely. Also blocks concurrent subagent patterns.
**Severity:** High — parallel execution is in the spec as a planned extension (SPEC S21) but the design is incomplete.

### Gap 3: `pause_for_human` is No-Op in `--once` Mode (KNOWN CONSTRAINT)

**Impact:** Brainstorming workflow requires human approval gates. In `--once` mode these are skipped.
**Severity:** Medium — Interactive REPL (v0.5) would fix it.

### Gap 4: No Dynamic Step Count / Plan-Driven Execution (SPECCED, NOT IMPLEMENTED)

**Impact:** Cannot read a plan file and dynamically generate pipeline steps. Static YAML.
**Severity:** Medium — self-modifying pipeline feature (SPEC S21) would address this.

### Gap 5: No Structured Output / JSON Schema Validation (SPECCED, NOT IMPLEMENTED)

**Impact:** `on_result` uses `contains:` (prose matching) — unreliable for branching on LLM output.
**Severity:** Medium — structured I/O schemas would make branching deterministic.

---

## Prompt Engineering: Improvements Over the Original Superpowers

The prompt files in `demo/superpowers/prompts/` are not direct copies of the obra/superpowers SKILL.md files. They've been rewritten using 2025-2026 prompting research to address known anti-patterns in the originals. If you're comparing these to the upstream superpowers, here's what changed and why.

### Problem: Persona Assignment Increases Hallucination Risk

The original superpowers use persona patterns extensively:

- *"You are a Socratic design facilitator"* (brainstorming)
- *"You are a senior code reviewer with expertise in software architecture"* (code review)
- *"You are an expert at creating detailed, actionable implementation plans"* (writing-plans)

2025 research (Frontiers in AI, Lakera) shows that **unstructured persona assignment without task grounding increases hallucination**. The model fills in implied expertise with fabricated details. The persona tells the model *who to be* but not *what to produce* — the model infers output expectations from the role, which introduces drift.

### Fix: Task-Grounded Prompting

Each prompt now opens with a concrete **Objective** statement that defines what the output must be, not who the model is:

| Original | Rewritten |
|---|---|
| "You are a Socratic design facilitator" | "Produce a design specification that turns the user's idea into a well-defined, implementable plan" |
| "You are a senior code reviewer" | "Identify defects, design issues, and improvement opportunities in the provided code changes" |
| "You are an expert at creating implementation plans" | "Produce an implementation plan that a skilled engineer can follow without guessing" |

The task objective grounds every subsequent instruction — the model knows what artifact it's producing, not what character it's playing.

### Fix: Semi-Formal Reasoning Structure

The code reviewer prompt now requires a structured reasoning template for every finding:

1. **Observation:** What the code actually does (cite file path and line)
2. **Expectation:** What it should do (cite requirement, pattern, or principle)
3. **Gap:** The specific discrepancy
4. **Recommendation:** A concrete, actionable fix

This is based on Meta's 2025 semi-formal reasoning research (VentureBeat, 2025), which showed that requiring explicit premises, execution paths, and formal conclusions before answers achieved 93% accuracy in code review tasks — significantly reducing hallucinated findings.

The brainstorming prompt uses a similar structure for approach evaluation:
- *Premise:* What problem does this approach solve?
- *Trade-offs:* What does it gain vs what does it cost?
- *Conclusion:* Why recommend or reject?

### Fix: Explicit Output Format

The original superpowers describe process steps but leave output format implicit — the model guesses what the output should look like based on the persona. Several prompts now include explicit output format specifications:

- **Code reviewer:** Required sections (Summary, Critical Issues, Important Issues, Suggestions, Verdict) with APPROVED/CHANGES REQUESTED
- **Brainstorming:** Required spec sections (Problem Statement, Chosen Approach, Component Breakdown, Data Flow, Edge Cases, Error Handling, Testing Strategy, Migration Plan)
- **Plan decomposition:** Required markdown template with Goal, Architecture, and numbered Tasks

### Fix: Constraint-First Hierarchy

Instructions are ordered with hard constraints and boundaries before process steps. This follows the 2025 system prompt architecture research (Surendran, 2025) which shows that models attend more strongly to early instructions. Constraints that appear after process steps are more likely to be violated.

### What Didn't Change

The two Category A discipline files (`tdd-discipline.md`, `verification-discipline.md`) were already well-structured — they use imperative constraints ("The Iron Law: NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST") rather than persona assignment. They received only minor additions: an **Objective** framing that explains *why* the constraints exist, which helps the model prioritize them appropriately in context.

### Sources

- Survey and analysis of hallucinations in large language models (Frontiers in AI, 2025)
- Meta's semi-formal reasoning structured prompting technique (VentureBeat, 2025)
- System Prompts vs User Prompts: Instruction Architecture (Surendran, 2025)
- The Ultimate Guide to Prompt Engineering in 2026 (Lakera, 2026)
- Concise Goal-Oriented Prompting for Code Generation (ECOOP 2025)

---

## Implementation Status

Working demos are in `demo/superpowers/`. See `demo/superpowers/README.md` for details on which pipelines are fully functional vs proposed.
