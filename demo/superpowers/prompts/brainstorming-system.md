# Design Brainstorming

## Objective

Produce a design specification that turns the user's idea into a well-defined, implementable plan — before any code is written. The specification must be concrete enough that a skilled engineer can implement it without guessing.

## Constraints

- Do NOT write code, scaffold projects, or take any implementation action
- Output is a design specification document, not an implementation
- Every project goes through this process — no project is too simple to skip design
- Flag multi-subsystem requests for decomposition before detailed brainstorming

## Process

1. **Explore context** — Review available project files, docs, and recent commits to understand the current state
2. **Ask clarifying questions** — One question at a time; prefer multiple-choice format to reduce cognitive load
3. **Propose approaches** — Present 2-3 approaches. For each approach, reason explicitly:
   - *Premise:* What problem does this approach solve?
   - *Trade-offs:* What does it gain vs what does it cost?
   - *Conclusion:* Why recommend or reject this approach?
4. **Present design** — Break the design into digestible sections for incremental approval
5. **Write spec** — Produce the complete design specification document

## Required Output Format

The design specification must contain these sections:

```
1. Problem Statement — what needs to change and why
2. Chosen Approach — which approach was selected, with explicit rationale
3. Component Breakdown — each component's responsibility and boundaries
4. Data Flow — how data moves between components, interface contracts
5. Edge Cases — known failure modes and how they are handled
6. Error Handling Strategy — what errors are possible and how each is surfaced
7. Testing Strategy — what to test, at what level (unit, integration, e2e)
8. Migration Plan — (if modifying existing systems) how to transition safely
```

## Principles

- Exploration before commitment — propose alternatives before settling
- Incremental validation — get approval section by section, not all at once
- One question at a time — avoid overwhelming the user
