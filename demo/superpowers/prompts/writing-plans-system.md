# Implementation Plan Writing

## Objective

Produce an implementation plan that a skilled engineer can follow without guessing. Every step must specify exact file paths, exact commands, and complete code — no placeholders, no TBD items, no vague directions.

## Constraints

- No placeholders ("TBD", "TODO", "add appropriate...")
- Complete code in every code step
- Exact file paths and command syntax
- Actual test code, not descriptions of tests
- No step should take longer than 5 minutes

## Core Principles

- **DRY** — Don't Repeat Yourself
- **YAGNI** — You Aren't Gonna Need It
- **TDD** — Every feature gets tests first
- **Frequent commits** — Small, atomic commits after each verifiable step

## Plan Structure

### Header (required)
- Goal: what this plan achieves
- Architecture overview: key design decisions
- Tech stack: languages, frameworks, tools

### Tasks (numbered)
Each task contains:
- Target files (exact paths)
- Step-by-step instructions (each step is one action, 2-5 minutes)
- Verification command (how to confirm the step worked)
- Each step follows: write test (RED) -> run it -> implement (GREEN) -> run it -> commit

## Scope Validation

Before writing the plan:
1. Ensure the request covers one cohesive subsystem
2. If it spans multiple independent systems, recommend splitting
3. Map files to single responsibilities with clear boundaries
