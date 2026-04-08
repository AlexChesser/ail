# Oh My AIL

A multi-agent orchestration pipeline that mirrors the architecture of [oh-my-opencode](https://github.com/oh-my-opencode/oh-my-openagent), expressed as AIL pipelines.

## What This Is

Oh My AIL implements an **Intent Gate** pattern: every user request is first classified by Sisyphus into one of four complexity tiers, then routed through a matching agent chain. Simpler requests get fewer agents; ambiguous or exploratory requests get a full planning and review cycle before any implementation begins.

## Architecture

```
.ail.yaml                    ← Main entry point (Sisyphus intent gate)
├── prompts/                 ← System prompt files for each agent
│   ├── sisyphus.md          ← Intent Gate classification rules
│   ├── metis.md             ← Pre-planning consultant
│   ├── prometheus.md        ← Strategic planner
│   ├── momus.md             ← Plan reviewer
│   ├── atlas.md             ← Todo orchestrator
│   ├── hephaestus.md        ← Autonomous deep worker
│   ├── oracle.md            ← Read-only strategic consultant
│   ├── explore.md           ← Codebase search specialist
│   └── librarian.md         ← Documentation research specialist
├── agents/                  ← Standalone agent pipelines
│   ├── hephaestus.ail.yaml
│   ├── oracle.ail.yaml
│   ├── explore.ail.yaml
│   ├── momus.ail.yaml
│   ├── atlas.ail.yaml
│   ├── prometheus.ail.yaml
│   ├── metis.ail.yaml
│   └── librarian.ail.yaml
└── workflows/               ← Composed agent chains by complexity tier
    ├── trivial.ail.yaml     ← TRIVIAL: Hephaestus
    ├── explicit.ail.yaml    ← EXPLICIT: Explore → Atlas
    ├── exploratory.ail.yaml ← EXPLORATORY: Explore → Prometheus → Momus → Atlas
    └── ambiguous.ail.yaml   ← AMBIGUOUS: Metis → Prometheus → Momus → Atlas
```

## Intent Classification

| Classification | Trigger | Agent Chain |
|---|---|---|
| `TRIVIAL` | Single-file change, typo, obvious bug fix | Hephaestus |
| `EXPLICIT` | Clear actionable request, multi-file, well-specified | Explore → Atlas |
| `EXPLORATORY` | Clear goal, unclear implementation path | Explore → Prometheus → Momus → Atlas |
| `AMBIGUOUS` | Unclear requirements, undefined scope | Metis → Prometheus → Momus → Atlas |

## Agents

| Agent | Role | Tool Access |
|---|---|---|
| **Sisyphus** | Intent Gate — classifies and routes | Read, Glob, Grep |
| **Metis** | Pre-planning — surfaces hidden ambiguity | Read, Glob, Grep, Bash |
| **Prometheus** | Strategic planner — produces implementation plans | Read, Glob, Grep, Bash |
| **Momus** | Plan reviewer — catches flaws before execution | Read, Glob, Grep |
| **Atlas** | Todo orchestrator — decomposes and tracks execution | Read, Edit, Write, Bash, Glob, Grep |
| **Hephaestus** | Autonomous implementer — full codebase access | Read, Edit, Write, Bash, Glob, Grep |
| **Oracle** | Read-only strategic consultant | Read, Glob, Grep (no writes) |
| **Explore** | Codebase search specialist | Glob, Grep, Read (no writes) |
| **Librarian** | External documentation researcher | Read, Glob, Grep, WebFetch, WebSearch |

## Running

```bash
# From the repo root — requires a release build and the claude CLI
cd /path/to/your/project
/path/to/ail/target/release/ail --once "your request here" --pipeline /path/to/ail/demo/oh-my-ail/.ail.yaml

# Validate the pipeline structure
cargo run -- validate --pipeline demo/oh-my-ail/.ail.yaml

# Inspect the resolved pipeline YAML
cargo run -- materialize --pipeline demo/oh-my-ail/.ail.yaml

# Run individual agents for testing
cargo run -- validate --pipeline demo/oh-my-ail/agents/hephaestus.ail.yaml
```

## Design Principles

1. **Each agent is a standalone pipeline.** Any agent can be called independently for testing or direct use.

2. **Workflows compose agents.** The four workflow files chain agents via `pipeline:` steps — the sub-pipeline isolation model means each agent sees only the final output of its predecessor.

3. **Tool permissions enforce role boundaries.** Momus and Oracle cannot write files. Explore cannot modify anything. Hephaestus gets full access. Permissions are architectural, not suggestions.

4. **Sisyphus routes via `on_result` branching.** The classification token on the first line of Sisyphus's response triggers a `contains:` match that fires the appropriate workflow sub-pipeline.

## Differences from Oh My OpenCode

Oh My AIL is inspired by [oh-my-opencode](https://github.com/oh-my-opencode/oh-my-openagent) but diverges in prompt architecture based on 2025-2026 prompting research:

### Task-Grounded Objectives (not persona assignment)

The original oh-my-opencode agents use persona assignment ("You are X, a senior architect with decades of experience..."). Research (Frontiers in AI 2025, Lakera 2026) shows this increases hallucination — the model fills in implied expertise with fabricated details. Oh My AIL prompts open with concrete **Objective** statements that define what output artifact to produce, not who the model should pretend to be.

| Technique | oh-my-opencode | Oh My AIL |
|-----------|---------------|-----------|
| Opening | "You are X, the..." | "Classify / Produce / Evaluate..." |
| Framing | Role identity | Output artifact |
| Risk | Hallucinated expertise | Grounded on deliverable |

The mythological agent names (Sisyphus, Prometheus, etc.) are retained as human-readable identifiers — the names help users distinguish agents. What was removed is the narrative framing that tells the model *who to be*.

### Constraint-First Ordering

Constraints appear immediately after the Objective, before any process steps. Research (Surendran 2025) shows models attend more strongly to early instructions — constraints buried after 30+ lines of process description are more likely to be violated.

### Semi-Formal Reasoning Templates

Four evaluative agents (Sisyphus, Prometheus, Momus, Oracle) include structured reasoning templates that require explicit evidence trails for each judgment. This is based on Meta's 2025 research showing semi-formal reasoning achieves 93% accuracy in evaluative tasks by preventing hallucinated findings.

Procedural and retrieval agents (Metis, Atlas, Hephaestus, Explore, Librarian) do not have reasoning templates — adding them to non-evaluative agents would slow them down and encourage unnecessary interpretation.

### Explicit Output Formats

Every agent specifies its required output structure. The original oh-my-opencode prompts describe process steps but leave output format implicit, letting the model guess based on persona inference.

For detailed analysis, see `docs/research/oh-my-ail-prompt-modernization.md`.

## Known Limitations (v0.1)

- **Sub-pipeline context passing is limited.** Each workflow step passes only the final output of the previous agent (`{{ step.<id>.response }}`). In a future version, richer context threading between agents would improve coherence.
