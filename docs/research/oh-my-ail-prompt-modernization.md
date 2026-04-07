# Modernizing Oh-My-AIL Agent Prompts: Prompt Engineering Analysis

**Date:** 2026-04-07
**Source:** [oh-my-opencode/oh-my-openagent](https://github.com/oh-my-opencode/oh-my-openagent) (upstream inspiration)
**Status:** Analysis complete; rewrites applied

---

## Summary

Oh My AIL is a multi-agent orchestration pipeline inspired by [oh-my-opencode](https://github.com/oh-my-opencode/oh-my-openagent). It implements an Intent Gate pattern with 9 specialized agents, each with a system prompt defining its behavior. This document analyzes the original prompt architecture and applies 2025-2026 prompting research to modernize all 9 prompts.

**Key finding:** All 9 prompts used persona assignment with mythological backstory as their primary framing technique. Research shows this increases hallucination risk by telling the model *who to be* rather than *what to produce*. The modernization replaces persona framing with task-grounded objectives while preserving the functional content that makes each prompt effective.

---

## The Problem: Persona Assignment

Every oh-my-ail prompt opened with a pattern like:

> "You are Sisyphus, named after the mythological figure condemned to roll his boulder endlessly uphill..."

> "You are Prometheus, named after the Titan who stole fire from the gods..."

> "You are Oracle, the rubber duck with decades of experience — a senior architect who has seen every pattern, every failure mode..."

2025 research (Frontiers in AI, Lakera) demonstrates that **unstructured persona assignment without task grounding increases hallucination**. The model fills in implied expertise with fabricated details. The persona tells the model *who to be* but not *what to produce* — the model infers output expectations from the role, which introduces drift.

This is the same problem found and fixed in the superpowers prompts (see `docs/research/superpowers-as-pipelines.md`). The oh-my-ail prompts have it more severely because:

1. **Mythological backstory consumes tokens** without functional value
2. **Motivational framing** ("A bad plan that passes your review is your failure", "the boulder does not stop") is theatrical rather than instructive
3. **Implicit expertise claims** ("decades of experience", "seen every pattern") encourage the model to fabricate confident-sounding advice

---

## Four Techniques Applied

### 1. Task-Grounded Objectives (replaces persona assignment)

Each prompt now opens with a concrete **Objective** statement that defines the expected output artifact:

| Agent | Original Opening | Modernized Objective |
|-------|-----------------|---------------------|
| Sisyphus | "You are Sisyphus, the primary orchestrator... Like the mythological figure condemned to roll his boulder endlessly uphill..." | "Classify each incoming user request into exactly one complexity category (TRIVIAL, EXPLICIT, EXPLORATORY, AMBIGUOUS) and route it to the correct agent sequence." |
| Prometheus | "You are Prometheus, named after the Titan who stole fire from the gods..." | "Produce an implementation plan that is complete, verifiable, and immediately actionable by Atlas." |
| Oracle | "You are Oracle, the rubber duck with decades of experience — a senior architect..." | "Provide architectural analysis and strategic recommendations grounded in the actual codebase." |
| Momus | "You are Momus, named after the Greek god of satire and fault-finding..." | "Evaluate Prometheus's implementation plan against four criteria and produce a verdict: APPROVED or NEEDS REVISION." |

The mythological names are retained as **titles** (human-readable agent identifiers), not role-play framing.

### 2. Constraint-First Hierarchy

In the original prompts, constraints appeared late — often after 30+ lines of process description. Research (Surendran 2025) shows models attend more strongly to early instructions. Constraints that appear after process steps are more likely to be violated.

**Before:** Title → Persona → Core Responsibility → Process → Constraints → Output Format
**After:** Title → Objective → Constraints → Process → Output Format

This ensures the model reads hard boundaries (e.g., "never write code", "never modify files", "read-only access only") before encountering process steps that might tempt violations.

### 3. Semi-Formal Reasoning Templates

Four agents make evaluative judgments where structured reasoning reduces hallucinated findings. Each gets a reasoning template:

| Agent | Reasoning Template | Rationale |
|-------|-------------------|-----------|
| **Sisyphus** | Signal → Complexity Indicator → Escalation Check → Decision | Classification decisions need explicit evidence trail |
| **Prometheus** | Options → Criteria → Trade-offs → Decision | Architectural choices need explicit alternatives considered |
| **Momus** | Location → Criterion Violated → Evidence → Impact | Plan review findings need grounded evidence (parallels superpowers code-reviewer) |
| **Oracle** | Observation → Principle → Assessment → Recommendation | Architectural advice needs codebase grounding |

Five agents do **not** get reasoning templates because they are procedural or retrieval agents, not evaluative ones:

| Agent | Why No Template |
|-------|----------------|
| Metis | Surfaces problems, doesn't evaluate competing options |
| Atlas | Procedural orchestrator (decompose, delegate, track) |
| Hephaestus | Executor — follows plans, doesn't make judgment calls |
| Explore | Retrieval agent — returns search results, doesn't interpret |
| Librarian | Retrieval agent — returns research findings, doesn't advise |

This distinction matters: adding reasoning templates to retrieval agents would slow them down and encourage unnecessary interpretation.

### 4. Consolidated Constraint Sections

Three agents (Oracle, Explore, Librarian) had constraints split across two sections: "What You Do NOT Do" and "Constraints". This fragmentation weakens both — the model may attend to one but not the other. The modernization merges them into a single `## Constraints` section.

---

## Per-Prompt Analysis

### Sisyphus (Intent Gate) — HIGH CHANGE

**Removed:**
- Full mythology paragraph (boulder of Sisyphus, never stops halfway)
- "The boulder does not stop" metaphor in Decision-Making Rules
- Narrative framing of "You are the Intent Gate"

**Added:**
- Task-grounded Objective defining the classification output
- Classification Reasoning template (Signal → Complexity Indicator → Escalation Check → Decision)

**Reordered:**
- Decision-Making Rules → Constraints, moved to position 2 (immediately after Objective)
- Agent roster moved after Constraints

**Preserved:** All four classification categories with examples, output format

### Metis (Pre-Planning) — LOW CHANGE

**Removed:**
- Titaness of wisdom persona sentence

**Added:**
- Task-grounded Objective defining the Ambiguity Report output

**Reordered:**
- Constraints moved to position 2; "never plan, never implement" merged into Constraints

**Preserved:** Five responsibilities, structured output format

### Prometheus (Strategic Planner) — MEDIUM CHANGE

**Removed:**
- Titan who stole fire mythology
- "You think like a senior engineer conducting a technical interview" persona framing

**Added:**
- Task-grounded Objective defining the implementation plan output
- Decision Reasoning template (Options → Criteria → Trade-offs → Decision)

**Reordered:**
- Constraints moved to position 2

**Preserved:** Process steps (understand before planning, challenge assumptions, plan to file level), output format, scope discipline constraint

### Momus (Plan Reviewer) — MEDIUM CHANGE

**Removed:**
- God of satire/fault-finding persona
- "A bad plan that passes your review is your failure" motivational framing

**Added:**
- Task-grounded Objective defining the review verdict output
- Finding Format template (Location → Criterion Violated → Evidence → Impact)

**Reordered:**
- Constraints moved to position 2

**Preserved:** Four review criteria (Clarity, Completeness, Correctness, Verifiability), "if the plan is sound, say so" constraint, output format

### Atlas (Todo Orchestrator) — LOW CHANGE

**Removed:**
- Titan holding up the sky persona
- "boulder rolled back" metaphor

**Added:**
- Task-grounded Objective defining the execution tracker output

**Reordered:**
- Constraints moved to position 2; Completion Discipline folded into Constraints

**Preserved:** Four responsibilities (Decomposition, Delegation, Tracking, Verification), task status notation, output format

### Hephaestus (Autonomous Deep Worker) — LOW CHANGE

**Removed:**
- God of the forge / smithy persona paragraph
- Theatrical "it is done when you say it is done" framing

**Added:**
- Task-grounded Objective defining the implementation + verification output

**Reordered:**
- Constraints moved to position 2; Minimal Footprint merged into Constraints

**Preserved:** All approach subsections (Research, Conventions, Complete Don't Stub, Verify), verification checklist, output format, codebase-specific rules

### Oracle (Strategic Consultant) — HIGH CHANGE

**Removed:**
- "rubber duck with decades of experience — senior architect who has seen every pattern" persona

**Added:**
- Task-grounded Objective defining the analysis output
- Reasoning Structure template (Observation → Principle → Assessment → Recommendation)

**Reordered:**
- "What You Do NOT Do" + "Constraints" merged into single Constraints section at position 2

**Preserved:** Scope list (architectural review, pattern recognition, second opinion, risk identification), process steps, output format

### Explore (Codebase Search Specialist) — MINIMAL CHANGE

**Removed:**
- "Speed and precision are your values" motivational framing

**Added:**
- Task-grounded Objective defining the structured search output

**Reordered:**
- "What You Do NOT Do" + "Constraints" merged into single Constraints section

**Preserved:** Five capabilities, four process steps, output format

### Librarian (Documentation and Research Specialist) — MINIMAL CHANGE

**Removed:**
- Persona opening sentence

**Added:**
- Task-grounded Objective defining the source-attributed research output

**Reordered:**
- "What You Do NOT Do" + "Constraints" merged into single Constraints section

**Preserved:** Five capabilities, four process steps, output format

---

## What Didn't Change

- **Mythological names as titles.** "Sisyphus — Intent Gate" is a human-readable identifier. The name helps users and developers distinguish agents quickly. What was removed is the narrative *about* the mythology.
- **Output format sections.** All 9 output format templates were already well-structured and explicit. They were preserved exactly.
- **Functional constraints.** The behavioral boundaries (tool access, read-only enforcement, scope discipline) were all good — they were moved earlier, not rewritten.
- **Pipeline YAML files.** No changes to any `.ail.yaml` files. The prompts are the only deliverable.
- **Codebase-specific rules in Hephaestus.** The convention list (no unwrap outside tests, no println in ail-core, etc.) is project-specific grounding that reduces hallucination — it stays.

---

## Sources

- Survey and analysis of hallucinations in large language models (Frontiers in AI, 2025)
- Meta's semi-formal reasoning structured prompting technique (VentureBeat, 2025)
- System Prompts vs User Prompts: Instruction Architecture (Surendran, 2025)
- The Ultimate Guide to Prompt Engineering in 2026 (Lakera, 2026)
- Concise Goal-Oriented Prompting for Code Generation (ECOOP 2025)
- Prior analysis: `docs/research/superpowers-as-pipelines.md` — Prompt Engineering Improvements section
