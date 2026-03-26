DECISIONS.md

# AIL — Pending Decisions

> A prioritized queue of design decisions that need to be actioned before or alongside spec updates and implementation work. Each entry has enough context to be actioned independently. Entries are grouped by theme and roughly ordered by dependency — decisions that block others come first.
>
> This document is a working artifact. Resolved entries are removed; their outcomes live in CHANGELOG.md and the spec.

---

## How to Use This Document

Each decision has a **status**, a **blocks** field (what cannot proceed until this is resolved), and a **proposed resolution** where one exists. Where no resolution is proposed, the open questions are listed explicitly.

| Status | Meaning |
|---|---|
| 🔴 Blocking | Must be resolved before dependent work can proceed |
| 🟡 Active | In scope for current work, not yet written to spec or code |
| ⚪ Deferred | Out of scope for now, tracked so it isn't lost |

---

## Theme 1 — Play as a Foundational Concern

### D-005 — "Play by Default" as a Design Principle
**Status:** 🟡 Active
**Blocks:** D-006, D-007, README, Docker strategy

**Background:**
The current spec and implementation are primarily designed for developers who have already decided to use `ail`. The "play by default" principle asserts something stronger: **the ability to try `ail` with zero friction must be a non-negotiable design constraint that is evaluated for every feature decision.**

This is not a distribution concern — it is a product philosophy. Every feature that requires setup, credentials, or prior knowledge before it can be experienced is a feature that most potential users will never see.

The practical test: can someone go from "I just heard about ail" to "I just saw it do something" in under three minutes, with no API key, no Rust toolchain, no configuration file?

**Proposed resolution:**
Establish "play by default" as a named, first-class design principle in the README and ARCHITECTURE.md alongside "Agent-First Design" and "The Two Layers." Every spec section and implementation PR is evaluated against: *does this make play harder or easier?*

Concrete implications:
- `--dry-run` must work with no API key and no `.ail.yaml` — the tool should be explorable before it is configurable.
- The Docker image must be the primary "play" artifact, not a secondary distribution option.
- The three-command quickstart (see D-006) is the README's opening paragraph, not a section buried after installation.
- Passthrough mode (no `.ail.yaml` found) should produce visible, friendly output that teaches the user what `ail` would do — not silent passthrough.

---

### D-006 — Docker Quickstart Series
**Status:** 🟡 Active
**Blocks:** README, debut
**Depends on:** D-005

**Background:**
Docker is the primary "play with it" distribution path. The current plan has a single Docker image. The proposal is a **series of quickstart configurations** — progressively more complex, each demonstrating a distinct capability — so users can explore the value of `ail` before writing their own pipelines.

**Proposed quickstart series:**

```
quickstarts/
  00-hello/           # --dry-run, no API key required. Shows pipeline resolution.
  01-once/            # --once with a simple prompt. Requires API key.
  02-quality-gates/   # generate → lint → security → human approval
  03-verify-loop/     # generate → test → fix loop (Boris #9)
  04-claude-md/       # CLAUDE.md injected as context, institutional memory demo
  05-multi-provider/  # same pipeline, two providers, cost comparison
```

Each quickstart is:
- A single directory with a `docker-compose.yml`, a `.ail.yaml`, and a `README.md`
- Runnable with `docker compose up` (or a single `docker run` command)
- Self-contained — no external dependencies beyond the API key (where required)
- Accompanied by a one-paragraph explanation of what it demonstrates and why it matters

**The three-command README anchor** (no API key required):
```bash
# See what ail would do — no API key needed
docker run ghcr.io/alexchesser/ail --dry-run --once "refactor this for clarity"

# Run with your API key
docker run -e ANTHROPIC_API_KEY=... ghcr.io/alexchesser/ail --once "refactor this for clarity"

# Use a proxy (LiteLLM, etc.)
docker run -e ANTHROPIC_API_KEY=... -e ANTHROPIC_BASE_URL=https://your-proxy/... \
  ghcr.io/alexchesser/ail --once "refactor this for clarity"
```

**Open questions:**
- Should quickstarts live in the main repo under `quickstarts/` or in a separate `ail-quickstarts` repo? Separate repo keeps the main repo clean but adds maintenance overhead.
- Does the `00-hello` dry-run quickstart need a pre-baked `.ail.yaml` in the image, or should it generate a minimal example pipeline on first run?
- ANTHROPIC_BASE_URL against LiteLLM — needs verification spike before this can be documented as working.

---

### D-007 — Passthrough Mode as a Teaching Moment
**Status:** 🟡 Active
**Blocks:** D-005
**Depends on:** D-005

**Background:**
The current spec says: if no `.ail.yaml` is found, `ail` runs in passthrough mode — the underlying agent behaves exactly as if `ail` were not present. This is the correct safe default, but it's a missed opportunity.

**Proposed resolution:**
In passthrough mode, `ail` emits a brief, friendly message to stderr explaining what it would do if a pipeline were present. Something like:

```
ail: no pipeline found. Running in passthrough mode.
To add quality gates to this session, create a .ail.yaml file.
Run `ail init` to generate a starter pipeline.
```

`ail init` as a command deserves its own decision (D-008) but the principle here is: passthrough mode should teach, not silently disappear.

---

### D-008 — `ail init` — Starter Pipeline Generator
**Status:** ⚪ Deferred (post-debut)
**Depends on:** D-006, D-007

**Background:**
A command that generates a commented `.ail.yaml` starter file in the current directory. Could offer a selection of templates corresponding to the quickstart series. Reduces the barrier from "I want to try this" to "I have a working pipeline."

**Deferred reason:** Premature before the pipeline language is stable. Generating a starter file that becomes invalid in the next spec revision is worse than not generating one.

---

## Theme 2 — Debut Readiness

### D-010 — Killer Workflow: Generate → Test → Audit → Approve → Commit
**Status:** 🟡 Active
**Blocks:** debut, quickstart 02
**Depends on:** D-006

**Reviewer claim:** Ship one killer workflow that feels better than hand-rolled scripts and lighter than LangGraph.

**Assessment:** Correct. This is the debut demo. The suggested workflow maps directly to `ail`'s core capabilities and is expressible today with the current spec. It should be quickstart `02-quality-gates` and the primary demo in the README.

**Proposed pipeline:**
```yaml
version: "0.0.1"

pipeline:
  - id: claude_md
    context:
      shell: "cat CLAUDE.md || echo ''"

  - id: implement
    append_system_prompt:
      - "{{ step.claude_md.result }}"
    prompt: "{{ step.invocation.prompt }}"

  - id: lint
    context:
      shell: "cargo clippy -- -D warnings"
    on_result:
      - exit_code: 0
        action: continue
      - match: always
        action: pause_for_human
        message: "Lint failures. Review before continuing."

  - id: test
    context:
      shell: "cargo test --quiet"
    on_result:
      - exit_code: 0
        action: continue
      - match: always
        action: pause_for_human
        message: "Tests failing. Review output."

  - id: security_audit
    prompt: |
      Review the implementation for security issues.
      Answer CLEAN or describe findings.
    on_result:
      contains: "CLEAN"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Security findings require review."

  - id: approve_and_commit
    prompt: "Pipeline passed all checks. Summarize what was done and await approval to commit."
    on_result:
      match: always
      action: pause_for_human
      message: "Approve to commit."
```

---

### D-012 — Second Runner: Which One and When
**Status:** 🟡 Active
**Blocks:** portability story, debut credibility
**Depends on:** D-006

**Reviewer claim:** Support at least 2–3 runners, otherwise the portability story stays theoretical.

**Assessment:** Correct. Single-runner portability is a claim without evidence. The question is which second runner to prioritize.

**Candidates:**

| Runner | Effort | Value |
|---|---|---|
| `StubRunner` | Trivial | Already planned. Enables `--dry-run` without API key. Validates runner abstraction. |
| `NativeRestRunner` (Anthropic API direct) | Medium | Solves Docker demo problem. No Claude CLI install required. Enables `ANTHROPIC_BASE_URL` proxy support. |
| `AiderRunner` | Medium | Expands to a second real agent. Validates multi-runner story. Requires Aider install in Docker image. |
| `GeminiCliRunner` | Medium | Expands to a second vendor. Validates the "any CLI agent" claim. |

**Proposed resolution:** `StubRunner` first (already in BUILD-PROMPT.md Phase 8). `NativeRestRunner` second — it solves the Docker play problem (no Claude CLI in the image), enables `ANTHROPIC_BASE_URL` proxy compatibility, and makes the three-command quickstart possible without bundling a full agent. Validate `ANTHROPIC_BASE_URL` against LiteLLM before committing.

---

## Theme 3 — Positioning Against Alternatives

### D-016 — "Why Not Just Use Claude Hooks?" — README Must Answer This Directly
**Status:** 🔴 Blocking
**Blocks:** README, debut credibility

**The challenge:**
Max's first reaction to `ail` was "I would just use a Claude hook for that." This will be the first reaction of most experienced Claude Code users. If the README does not answer this question in the first two paragraphs, `ail` loses those users before they understand what it is.

Hooks are real, they work today, and for simple post-invocation scripts they are a valid alternative. Dismissing this is dishonest and will undermine credibility with exactly the audience `ail` needs.

**The honest comparison:**

| | Claude Hooks | `ail` |
|---|---|---|
| **What triggers it** | Completion event | Completion event |
| **What runs** | A script | A declarative pipeline |
| **HITL model** | None — scripts don't pause for humans | First-class — `pause_for_human`, `on_result` |
| **Audit trail** | Whatever your script writes | Structured pipeline run log, always |
| **Inheritance** | None | `FROM` chains, org-level base pipelines |
| **Dry run / preview** | Not built in | `--dry-run`, per-step `dry_run:` |
| **Multi-runner** | Claude-only | Claude CLI, Aider, Gemini CLI, native REST |
| **Shareable / composable** | Shell scripts in a repo | YAML pipelines, `FROM` inheritance, skill packages |
| **Self-modifying** | No | Planned — pipeline as evolvable artifact |

**The honest caveat that must also appear:**
For simple post-invocation quality checks — run a linter, run tests, format code — a well-written hook is probably sufficient and has less setup overhead. `ail` is the right tool when you want: reproducibility across a team, a structured audit trail, HITL gates that pause rather than fail, portability across runners, and a pipeline that can evolve over time. If you just need to run `cargo fmt` after every completion, use a hook.

**Proposed README section:** "Why not just use hooks?" — a short, honest comparison that acknowledges hooks, states the gap clearly, and does not oversell. Tone: confident, not defensive.

**Relationship to D-009 (named moat — now in spec/core/s01):** The moat answer and the hooks answer are the same answer stated differently. D-009 names the architectural guarantee; D-016 explains why hooks can't provide it. They should be written together.

---

### D-017 — "That's What CI Is For" — The Intervention Point Argument
**Status:** 🟡 Active
**Blocks:** README, positioning
**Depends on:** D-016

**The challenge:**
Max's second point: "on the job, that's what CI is for." At FAANG scale, CI is the quality gate. This is true and should be acknowledged.

**The response:**
CI runs after a PR is opened — it catches problems *after* the human has already seen the output, formed an opinion, and often moved on. `ail`'s intervention point is earlier: the pipeline runs before the human sees the result. That is a fundamentally different moment.

The analogy: CI is the net. `ail` is the guardrail before the cliff. Both are useful; they operate at different points in the feedback loop.

Additional distinctions:
- CI requires a commit and a PR. Much agentic work happens within a session and never becomes a PR — or produces a series of small completions that each deserve quality gates before the next prompt is issued.
- CI is asynchronous — you wait for it. `ail` is synchronous — the human cannot proceed until the pipeline completes or explicitly overrides.
- CI cannot pause for human input mid-run. `ail`'s HITL model is a first-class feature.

**The honest caveat:**
Max's point is correct at his scale and for his use case. For individual developers and small teams doing session-level agentic work, CI is often overkill for within-session quality gates. `ail` fills the gap between "I manually reviewed this" and "CI caught it after I opened a PR."

**Note:** This is not an argument that `ail` replaces CI. It complements it. The killer workflow (D-010) should include a note: "ail runs before you commit; CI runs after. Both matter."

---

## Theme 4 — Deferred Vision

### D-015 — CA/PKI Niche: Park, Don't Pursue
**Status:** ⚪ Deferred

**Reviewer suggestion:** Consider Attestable CA Operations as a specialized vertical.

**Assessment:** The audit trail / structured evidence angle is genuinely valuable and already implied by the pipeline run log. The CA/PKI vertical specifically requires domain credibility, conservative community buy-in, and compliance certifications that are not achievable at this stage. The right move is to build the structured evidence foundation well and let vertical applications emerge from users rather than pursuing them top-down.

**Deferred:** Revisit if a CA operator shows up wanting to use `ail` and is willing to collaborate on the vertical requirements.

---

### D-018 — Skill Token Optimization: Formally Park This
**Status:** ⚪ Deferred (explicitly — do not revisit before post-POC)

**Background:**
An early design idea was to detect which steps don't need skills loaded and call Claude with `--no-skill` to save tokens.

**Assessment:**
- One miss where a skill doesn't load and the agent does something unexpected = more expensive in human intervention cost than the tokens saved
- Inconsistent skill loading likely breaks prompt caching, making the economics worse
- At FAANG scale, human interventions are the primary cost metric — not token costs

**Formal decision:** This feature is parked. It does not appear in any roadmap, BUILD-PROMPT phase, or spec section. If token economics become a primary concern at scale, revisit with fresh eyes at that time — the answer may be a different architecture entirely (e.g., prompt caching strategy) rather than selective skill loading.

---

### D-019 — Self-Modifying Pipelines: Preserve the Vision, Don't Build It Yet
**Status:** ⚪ Deferred (post-POC, significant design work required)

**Background:**
The most novel part of the `ail` vision is the self-modifying pipeline: a pipeline that can analyze the delta between frontier and commodity model outputs, generate improvements to its own YAML, test those improvements, optionally get human approval, and hot-reload. Over time the pipeline encodes the specific knowledge of your codebase, your org's style, your team's preferences — not as a memory file, but as executable, testable, version-controlled YAML steps.

This is distinct from and more powerful than:
- `CLAUDE.md` / memory files — those are static context, not executable logic
- Hooks — those cannot modify themselves
- LangGraph / CrewAI — those do not have the concept of a pipeline as a version-controlled, human-reviewable, self-improving artifact

**The supervisory attentional layer — ACT-R and GOAP in scope:**
Anderson's ACT-R production system is the computational model of Schank and Abelson's "intelligence is knowing which script to run." Given a goal buffer and current context state, the production system selects which rule fires. Applied to `ail`: given the current task, which pipeline activates?

Norman and Shallice's Supervisory Attentional System is the two-layer model:
- **Contention scheduling** — executes the active schema. This is what `ail` does today.
- **SAS** — intervenes when the task is novel, selects or modifies the schema. This is where `ail` is going.

GOAP-for-pipeline-selection is therefore in scope: given a goal state, plan the sequence of pipeline invocations that reaches it. This is categorically distinct from GOAP-as-general-task-orchestrator (which belongs to the agent layer). The filter: `ail` governs pipelines; the agent executes tasks.

**Why it cannot be built yet:**
It requires: stable pipeline execution (not yet), hot reload (open question in spec), multi-runner parallel execution (not yet), structured output from comparison steps (not yet), and a HITL approval flow for pipeline modifications specifically (not yet designed).

**What to do now:**
Preserve the vision explicitly in the spec (§22 planned extensions) and README so it is on record as the trajectory. The foundational decisions are all in service of this end state — they are not just "make a better hooks system." Every spec decision should be evaluated against: does this make the self-modifying pipeline easier or harder to build later?

**Specific design seeds to capture for later:**
- The `on_result` model with structured branching is the primitive that will power the comparison step
- The pipeline run log is the evidence record that a self-modification loop would read to make decisions
- `FROM` inheritance is how a self-modification could layer changes without overwriting the base
- The HITL approval flow is how a human stays in control of pipeline evolution
- Version control of `.ail.yaml` is how changes are auditable and reversible

**Relationship to D-013 (non-goal — now in spec/core/s01):** Self-modifying pipelines are not general multi-agent orchestration. They are a specific, bounded application of the pipeline model to its own improvement.

---

### D-020 — Parallel Sampling and Quality Comparison: Design Seed
**Status:** ⚪ Deferred (post-POC)
**Depends on:** D-019, multi-runner parallel execution

**Background:**
From the Slack conversation: the concept of sending an identical prompt to two runners (frontier + commodity), collecting both responses, and passing them to a quality comparison step. The comparison step identifies what the frontier did better and generates prompt improvements that could achieve similar results from the commodity runner.

**Why this matters beyond cost optimization:**
This is not primarily about saving money. It is about building a feedback loop that systematically improves the pipeline's ability to get good results from any runner. The commodity runner improves over time not because the model improves, but because the pipeline's prompts and skills get better at directing it.

**Design seed (not a spec yet):**
```yaml
- id: implement_frontier
  prompt: "{{ step.invocation.prompt }}"
  provider: frontier

- id: implement_commodity
  prompt: "{{ step.invocation.prompt }}"
  provider: commodity
  # runs in parallel with implement_frontier — requires parallel execution (planned)

- id: quality_compare
  prompt: |
    Compare these two implementations:
    Frontier: {{ step.implement_frontier.response }}
    Commodity: {{ step.implement_commodity.response }}
    What did the frontier do better? What prompt changes could
    achieve similar results from the commodity?
  on_result:
    match: always
    action: pause_for_human
    message: "Review quality comparison. Approve pipeline update?"
```

**Deferred:** Requires parallel execution primitives that don't exist yet. Capture the concept, don't design it yet.

---

*Last updated: 2026-03-25. Active: D-005–D-007 (play by default), D-010 (killer workflow demo), D-012 (NativeRestRunner), D-016–D-017 (README hooks/CI copy). Blocking: D-016. Deferred: D-008, D-015, D-018–D-020.*
