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

## Known Limitations (v0.1)

- **Sub-pipeline context passing is limited.** Each workflow step passes only the final output of the previous agent (`{{ step.<id>.response }}`). In a future version, richer context threading between agents would improve coherence.
