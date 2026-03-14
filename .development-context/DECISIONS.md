DECISIONS.md

<<<<<<< HEAD
<<<<<<< HEAD
=======
32.80 KB •559 lines•Formatting may be inconsistent from source
>>>>>>> cbba6f6... wip
=======
>>>>>>> ab1eba9... dontext: update decisions
# AIL — Pending Decisions

> A prioritized queue of design decisions that need to be actioned before or alongside spec updates and implementation work. Each entry has enough context to be actioned independently. Entries are grouped by theme and roughly ordered by dependency — decisions that block others come first.
>
<<<<<<< HEAD
<<<<<<< HEAD
> This document is a working artifact. Resolved entries are removed; their outcomes live in CHANGELOG.md and the spec.
=======
> This document is a working artifact. Entries move to `CHANGELOG.md` or inline spec notes once resolved.
>>>>>>> cbba6f6... wip
=======
> This document is a working artifact. Resolved entries are removed; their outcomes live in CHANGELOG.md and the spec.
>>>>>>> ab1eba9... dontext: update decisions

---

## How to Use This Document

Each decision has a **status**, a **blocks** field (what cannot proceed until this is resolved), and a **proposed resolution** where one exists. Where no resolution is proposed, the open questions are listed explicitly.

| Status | Meaning |
|---|---|
| 🔴 Blocking | Must be resolved before dependent work can proceed |
| 🟡 Active | In scope for current work, not yet written to spec or code |
<<<<<<< HEAD
<<<<<<< HEAD
=======
| 🟢 Resolved | Decision made, needs to be written into spec/code |
>>>>>>> cbba6f6... wip
=======
>>>>>>> ab1eba9... dontext: update decisions
| ⚪ Deferred | Out of scope for now, tracked so it isn't lost |

---

<<<<<<< HEAD
<<<<<<< HEAD
## Theme 1 — Play as a Foundational Concern

### D-005 — "Play by Default" as a Design Principle
**Status:** 🟡 Active
**Blocks:** D-006, D-007, README, Docker strategy

**Background:**
=======
## Theme 1 — Pipeline Language Redesign

### D-001 — Prompt and Context as First-Class Pipeline Members
**Status:** 🟢 Resolved — written to spec/core/s05-step-specification.md
**Blocks:** D-002, D-003, D-004, spec §5 rewrite

**Background:**  
The current spec treats `shell:` as a step type peer to `prompt:`. A late-session design conversation proposed a cleaner model: a pipeline is a sequence of **members**, each of which is one of two abstract types:

- `prompt` — invokes an LLM. Consumes context, produces a response. Costs tokens.
- `context` — gathers information deterministically. Zero-token. Feeds subsequent members.

Under this model, `shell:` is not a step type — it is a *source type* within a `context` member. MCP tool calls are also a source type within `context`. The distinction between "what runs the LLM" and "what gathers information for the LLM" becomes the top-level organizing principle of the language.

**Proposed resolution:**  
Rename the abstract container from "step" to "member" or retain "step" with clearer type discrimination. Redesign §5 around two primary member types. `shell:` as a standalone step type is removed; shell execution moves entirely into `context` members. `action:` (`pause_for_human`) and `pipeline:` (sub-pipeline calls) remain as additional member types.

**Open questions:**  
- Does `on_result` with `exit_code:` live on the `context` member as a whole, or on individual entries within it? Individual entries owning their own `on_result` is more granular but more complex to implement.
- Does a `context` member with multiple entries fail atomically (one entry fails, whole block fails) or per-entry?
- Does this model make per-step `context:` (the current "attach context to a specific step" pattern) redundant? If context members are positional in the pipeline, you achieve the same thing by placing the context member immediately before the step that consumes it.

---

### D-002 — `append_system_prompt:` Array Model, `skill:` Removal
**Status:** 🟢 Resolved — written to spec/core/s05 (§5.8) and spec/core/s06. `skill:` as primary field removed; `skill: <path>` map key within `append_system_prompt:` entries is the chosen form. Agent Skills compatibility preserved via SKILL.md format.
**Blocks:** spec §5.1, §6 rewrite
**Depends on:** D-001

**Background:**  
`skill:` as a primary field is syntactic sugar for `append_system_prompt:`. Making it a primary field creates a false distinction and introduces ordering ambiguity when combined with `append_system_prompt:`. The proposed resolution is:

- Remove `skill:` as a primary field entirely.
- `append_system_prompt:` becomes an ordered array. Each entry is either a plain string/file path, or a skill loader declared as `skill: <path>`.
- Order is explicit — declared order is execution order. No ambiguity.
- `system_prompt:` (full replacement) is compatible with `append_system_prompt:` — replacement sets the base, appends layer on top in declared order.
- All three (`system_prompt:`, `append_system_prompt:` with skill entries, `prompt:`) are composable. No mutual exclusivity except the logical one: you can't replace and also have nothing to replace (i.e., `system_prompt:` alone with no `prompt:` is valid — you're setting context for an LLM call triggered by `prompt:`).

**Proposed syntax:**
```yaml
- id: security_review
  system_prompt: ./prompts/base-system.md       # sets the base
  append_system_prompt:
    - skill: ./skills/security-reviewer/        # skill entry
    - skill: ail/dry-refactor                   # built-in skill
    - "Always flag hardcoded credentials."      # inline string
    - ./prompts/extra-context.md                # file, detected by path prefix
  prompt: "{{ step.invocation.response }}"
```

**Rule:** `prompt:` is required whenever any system context field is declared. A step with `system_prompt:` or `append_system_prompt:` but no `prompt:` is a parse error — you are configuring an LLM call with no user message.

**Open questions:**  
- Should `skill:` be kept as a convenience shorthand in `append_system_prompt:` entries, or replaced with a more YAML-native discriminated type? Current proposal: `skill: <path>` as a map key within the array entry — detected the same way `prompt:` detects file paths vs inline strings.
- Does removing `skill:` as a primary field break the Agent Skills compatibility story? Need to verify whether the Agent Skills spec requires a specific invocation mechanism or just requires the SKILL.md format.

---

### D-003 — `context:` Member Positioning and Scope
**Status:** 🟢 Resolved — written to spec/core/s03 (pipeline-level `context:` block) and spec/core/s05 (§5.4 inline `context:` step). Per-entry `on_result` chosen. Pipeline-level and inline context both accessed as `{{ context.<id> }}`.
**Blocks:** spec §5b rewrite
**Depends on:** D-001

**Background:**  
The current spec defines `context:` at two levels: pipeline-level (runs once before all steps) and per-step (attached to a specific step). A late-session conversation raised the question of whether `context:` should be a full pipeline citizen — positional in the sequence, not attached to a step.

**Proposed resolution:**  
Three scopes, all valid:

1. **Pipeline-level** — declared at top level, runs once before any member executes. Available as `{{ context.<id> }}` throughout the entire pipeline.
2. **Inline/positional** — declared as a member in the pipeline sequence. Runs at that position. Results available to all subsequent members. This makes per-step attachment redundant — just position the context member before the step that needs it.
3. **Pipeline-level remains** for context that needs to be available from the very first step (e.g. `cat CLAUDE.md`, `git log`).

If inline/positional context members are adopted (per D-001), the per-step `context:` attachment pattern from the current spec is likely redundant and should be removed to reduce cognitive surface area.

**Source types within a context member:**
```yaml
- context:
  - id: file_index
    shell: "find . -name '*.rs' | sort"
  - id: db_schema
    mcp:                          # planned — see D-MCP
      server: postgres
      tool: describe_schema
  - id: recent_issues
    http:                         # planned
      url: "{{ env.JIRA_URL }}/search"
```

---

### D-004 — `action:` and `pipeline:` Member Types Under New Model
**Status:** 🟢 Resolved — written to spec/core/s05 intro table. Four step types: `prompt:`, `context:`, `action:`, `pipeline:`.
**Blocks:** spec §5 rewrite
**Depends on:** D-001

**Background:**  
Under the prompt/context two-type model, `action:` (`pause_for_human`) and `pipeline:` (sub-pipeline calls) need a home. They are neither LLM calls nor context gathering.

**Proposed resolution:**  
Retain `action:` and `pipeline:` as distinct member types alongside `prompt:` and `context:`. The full member type table becomes:

| Member type | Purpose | Token cost |
|---|---|---|
| `prompt:` | LLM invocation | Yes |
| `context:` | Deterministic information gathering | No |
| `action:` | Non-LLM side effect (`pause_for_human`, etc.) | No |
| `pipeline:` | Sub-pipeline delegation | Delegated |

---

## Theme 2 — Play as a Foundational Concern
=======
## Theme 1 — Play as a Foundational Concern
>>>>>>> ab1eba9... dontext: update decisions

### D-005 — "Play by Default" as a Design Principle
**Status:** 🟡 Active
**Blocks:** D-006, D-007, README, Docker strategy

<<<<<<< HEAD
**Background:**  
>>>>>>> cbba6f6... wip
=======
**Background:**
>>>>>>> ab1eba9... dontext: update decisions
The current spec and implementation are primarily designed for developers who have already decided to use `ail`. The "play by default" principle asserts something stronger: **the ability to try `ail` with zero friction must be a non-negotiable design constraint that is evaluated for every feature decision.**

This is not a distribution concern — it is a product philosophy. Every feature that requires setup, credentials, or prior knowledge before it can be experienced is a feature that most potential users will never see.

The practical test: can someone go from "I just heard about ail" to "I just saw it do something" in under three minutes, with no API key, no Rust toolchain, no configuration file?

<<<<<<< HEAD
<<<<<<< HEAD
**Proposed resolution:**
=======
**Proposed resolution:**  
>>>>>>> cbba6f6... wip
=======
**Proposed resolution:**
>>>>>>> ab1eba9... dontext: update decisions
Establish "play by default" as a named, first-class design principle in the README and ARCHITECTURE.md alongside "Agent-First Design" and "The Two Layers." Every spec section and implementation PR is evaluated against: *does this make play harder or easier?*

Concrete implications:
- `--dry-run` must work with no API key and no `.ail.yaml` — the tool should be explorable before it is configurable.
- The Docker image must be the primary "play" artifact, not a secondary distribution option.
- The three-command quickstart (see D-006) is the README's opening paragraph, not a section buried after installation.
- Passthrough mode (no `.ail.yaml` found) should produce visible, friendly output that teaches the user what `ail` would do — not silent passthrough.

---

### D-006 — Docker Quickstart Series
<<<<<<< HEAD
<<<<<<< HEAD
**Status:** 🟡 Active
**Blocks:** README, debut
**Depends on:** D-005

**Background:**
=======
**Status:** 🟡 Active  
**Blocks:** README, debut  
**Depends on:** D-005

**Background:**  
>>>>>>> cbba6f6... wip
=======
**Status:** 🟡 Active
**Blocks:** README, debut
**Depends on:** D-005

**Background:**
>>>>>>> ab1eba9... dontext: update decisions
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
<<<<<<< HEAD
<<<<<<< HEAD
**Status:** 🟡 Active
**Blocks:** D-005
**Depends on:** D-005

**Background:**
The current spec says: if no `.ail.yaml` is found, `ail` runs in passthrough mode — the underlying agent behaves exactly as if `ail` were not present. This is the correct safe default, but it's a missed opportunity.

**Proposed resolution:**
=======
**Status:** 🟡 Active  
**Blocks:** D-005  
=======
**Status:** 🟡 Active
**Blocks:** D-005
>>>>>>> ab1eba9... dontext: update decisions
**Depends on:** D-005

**Background:**
The current spec says: if no `.ail.yaml` is found, `ail` runs in passthrough mode — the underlying agent behaves exactly as if `ail` were not present. This is the correct safe default, but it's a missed opportunity.

<<<<<<< HEAD
**Proposed resolution:**  
>>>>>>> cbba6f6... wip
=======
**Proposed resolution:**
>>>>>>> ab1eba9... dontext: update decisions
In passthrough mode, `ail` emits a brief, friendly message to stderr explaining what it would do if a pipeline were present. Something like:

```
ail: no pipeline found. Running in passthrough mode.
To add quality gates to this session, create a .ail.yaml file.
Run `ail init` to generate a starter pipeline.
```

`ail init` as a command deserves its own decision (D-008) but the principle here is: passthrough mode should teach, not silently disappear.

---

### D-008 — `ail init` — Starter Pipeline Generator
<<<<<<< HEAD
<<<<<<< HEAD
**Status:** ⚪ Deferred (post-debut)
**Depends on:** D-006, D-007

**Background:**
=======
**Status:** ⚪ Deferred (post-debut)  
**Depends on:** D-006, D-007

**Background:**  
>>>>>>> cbba6f6... wip
=======
**Status:** ⚪ Deferred (post-debut)
**Depends on:** D-006, D-007

**Background:**
>>>>>>> ab1eba9... dontext: update decisions
A command that generates a commented `.ail.yaml` starter file in the current directory. Could offer a selection of templates corresponding to the quickstart series. Reduces the barrier from "I want to try this" to "I have a working pipeline."

**Deferred reason:** Premature before the pipeline language is stable. Generating a starter file that becomes invalid in the next spec revision is worse than not generating one.

---

<<<<<<< HEAD
<<<<<<< HEAD
## Theme 2 — Debut Readiness

### D-010 — Killer Workflow: Generate → Test → Audit → Approve → Commit
**Status:** 🟡 Active
**Blocks:** debut, quickstart 02
=======
## Theme 3 — Reviewer Feedback (Third-Party)

*The following decisions are informed by a trusted third-party review. Each is assessed independently against the project's goals — they are not instructions, but they deserve honest engagement.*

### D-009 — Strategic Moat: Name the Defensible Core
**Status:** 🟢 Resolved — written to spec/core/s01. "The defensible core is the intervention point." framing added. README work still needed (D-016 depends on this).
**Blocks:** README, positioning

**Reviewer claim:** The pipeline language concept is becoming obvious infrastructure. The window may be limited unless it develops a clear moat or specialized use case.

**Assessment:** Agreed that the language alone is not a moat. Disagreed that the window is necessarily limited. The defensible core is not the YAML syntax — it is the **session-level execution guarantee**: the pipeline runs before control returns to the human, enforced at the process level, not as a convention. That guarantee is architectural and harder to bolt on than it appears. GitHub Actions, CI/CD hooks, and LangGraph all operate at the task or workflow level — they do not own the moment between agent output and human input. `ail` does.

**Proposed resolution:** Name this guarantee explicitly in the README and §1 of the spec as the moat. The framing: "Any tool can run a script after an agent finishes. Only `ail` ensures the pipeline ran *before the human saw the output*." Add this to ARCHITECTURE.md §1 as a first-class design principle alongside "Agent-First Design."

---

### D-010 — Killer Workflow: Generate → Test → Audit → Approve → Commit
**Status:** 🟡 Active  
**Blocks:** debut, quickstart 02  
>>>>>>> cbba6f6... wip
=======
## Theme 2 — Debut Readiness

### D-010 — Killer Workflow: Generate → Test → Audit → Approve → Commit
**Status:** 🟡 Active
**Blocks:** debut, quickstart 02
>>>>>>> ab1eba9... dontext: update decisions
**Depends on:** D-006

**Reviewer claim:** Ship one killer workflow that feels better than hand-rolled scripts and lighter than LangGraph.

**Assessment:** Correct. This is the debut demo. The suggested workflow maps directly to `ail`'s core capabilities and is expressible today with the current spec. It should be quickstart `02-quality-gates` and the primary demo in the README.

**Proposed pipeline:**
```yaml
version: "0.0.1"

<<<<<<< HEAD
<<<<<<< HEAD
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
=======
context:
=======
pipeline:
>>>>>>> ab1eba9... dontext: update decisions
  - id: claude_md
    context:
      shell: "cat CLAUDE.md || echo ''"

  - id: implement
    append_system_prompt:
      - "{{ step.claude_md.result }}"
    prompt: "{{ step.invocation.prompt }}"

  - id: lint
<<<<<<< HEAD
    shell: "cargo clippy -- -D warnings 2>&1"
>>>>>>> cbba6f6... wip
=======
    context:
      shell: "cargo clippy -- -D warnings"
>>>>>>> ab1eba9... dontext: update decisions
    on_result:
      - exit_code: 0
        action: continue
      - match: always
        action: pause_for_human
        message: "Lint failures. Review before continuing."

  - id: test
<<<<<<< HEAD
<<<<<<< HEAD
    context:
      shell: "cargo test --quiet"
=======
    shell: "cargo test --quiet 2>&1"
>>>>>>> cbba6f6... wip
=======
    context:
      shell: "cargo test --quiet"
>>>>>>> ab1eba9... dontext: update decisions
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
<<<<<<< HEAD
<<<<<<< HEAD
=======
>>>>>>> ab1eba9... dontext: update decisions
    prompt: "Pipeline passed all checks. Summarize what was done and await approval to commit."
    on_result:
      match: always
      action: pause_for_human
      message: "Approve to commit."
<<<<<<< HEAD
```

---

### D-012 — Second Runner: Which One and When
**Status:** 🟡 Active
**Blocks:** portability story, debut credibility
=======
    action: pause_for_human
    message: "Pipeline passed. Approve to commit."
=======
>>>>>>> ab1eba9... dontext: update decisions
```

---

### D-012 — Second Runner: Which One and When
<<<<<<< HEAD
**Status:** 🟡 Active  
**Blocks:** portability story, debut credibility  
>>>>>>> cbba6f6... wip
=======
**Status:** 🟡 Active
**Blocks:** portability story, debut credibility
>>>>>>> ab1eba9... dontext: update decisions
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

<<<<<<< HEAD
<<<<<<< HEAD
## Theme 3 — Positioning Against Alternatives

### D-016 — "Why Not Just Use Claude Hooks?" — README Must Answer This Directly
**Status:** 🔴 Blocking
**Blocks:** README, debut credibility

**The challenge:**
=======
### D-013 — Define the Non-Goal: Do Not Become a General Multi-Agent Framework
**Status:** 🟢 Resolved — written to spec/core/s01 "What `ail` Is Not" section. README work still needed.
**Blocks:** feature prioritization, spec scope discipline

**Reviewer claim:** Define the non-goal. Do not become a general multi-agent framework for generalized QA automation.

**Assessment:** This is the most important strategic input in the review. It directly applies pressure to D-001 (the prompt/context redesign). The risk is real: MCP sources in context members, HTTP fetching, parallel agent fleets, worker pools — each of these individually is a reasonable feature, but together they are LangGraph. The spec already has a "15-factor compliance," OTEL tracing, safety guardrails, and benchmarking sections. The surface area is growing.

**Proposed resolution:** Add a "Non-Goals" section to the README and §1 of the spec. Draft:

> **What `ail` is not:**
> - A general multi-agent orchestration framework
> - A replacement for LangGraph, CrewAI, or AutoGen
> - A workflow engine for arbitrary automation
> - An agent — `ail` does not have goals of its own
>
> `ail` does one thing: it ensures a declared pipeline runs after every agent invocation, before the human sees the output. Everything else is in service of that guarantee.

**Feature filter:** Any proposed feature should be evaluated against "does this serve the post-invocation pipeline guarantee, or does it serve general orchestration?" If the honest answer is the latter, it belongs in §22 or not at all.

**Specific tension:** D-001 (prompt/context redesign) and D-003 (context member positioning) move `ail` toward general pipeline orchestration. They are worth doing for language clarity, but the scope should be bounded: context members exist to *feed prompt members*, not to replace general-purpose automation tools.

---

## Theme 4 — Implementation Sequencing

### D-014 — Order of Operations for Spec Updates
**Status:** 🟢 Resolved — D-001 through D-004, D-009, D-011, D-013 written to spec. Remaining active: D-005–D-007 (play/Docker), D-010 (killer workflow demo), D-012 (NativeRestRunner), D-016/D-017 (README).
**Blocks:** all spec edits

**The current spec is based on the original uploaded SPEC.md, not the version with §5a (dry_run), §5b (context), §5.8 (shell), and the system prompt redesign. Those changes exist in session artifacts but have not been consolidated into a single canonical document.**

**Proposed sequence:**
1. Consolidate all session edits into a single canonical SPEC.md (the uploaded file + all additions from this session)
2. Action D-001 through D-004 (pipeline language redesign) as a single spec rewrite of §5
3. Action D-011 (observability demotion) — isolated change, low risk
4. Action D-009 and D-013 (non-goals, moat framing) — README and §1 additions
5. Action D-005 through D-007 (play by default) — README and Docker work
6. Action D-010 (killer workflow) — quickstart + demo pipeline
7. Action D-012 (second runner) — depends on Docker work

---

### D-015 — CA/PKI Niche: Park, Don't Pursue
**Status:** ⚪ Deferred

**Reviewer suggestion:** Consider Attestable CA Operations as a specialized vertical.

**Assessment:** The audit trail / structured evidence angle is genuinely valuable and already implied by the pipeline run log. The CA/PKI vertical specifically requires domain credibility, conservative community buy-in, and compliance certifications that are not achievable at this stage. The right move is to build the structured evidence foundation well (D-011) and let vertical applications emerge from users rather than pursuing them top-down.

**Deferred:** Revisit if a CA operator shows up wanting to use `ail` and is willing to collaborate on the vertical requirements.

---

## Theme 5 — Positioning Against Alternatives (from Max's Feedback)

*Max is a FAANG-scale developer infra engineer. His team's primary metric is reducing human interventions in agent loops. His feedback deserves serious engagement — he is describing a legitimate alternative path that will occur to most technically sophisticated users when they first encounter `ail`.*

---
=======
## Theme 3 — Positioning Against Alternatives
>>>>>>> ab1eba9... dontext: update decisions

### D-016 — "Why Not Just Use Claude Hooks?" — README Must Answer This Directly
**Status:** 🔴 Blocking
**Blocks:** README, debut credibility

<<<<<<< HEAD
**The challenge:**  
>>>>>>> cbba6f6... wip
=======
**The challenge:**
>>>>>>> ab1eba9... dontext: update decisions
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

<<<<<<< HEAD
<<<<<<< HEAD
**The honest caveat that must also appear:**
=======
**The honest caveat that must also appear:**  
>>>>>>> cbba6f6... wip
=======
**The honest caveat that must also appear:**
>>>>>>> ab1eba9... dontext: update decisions
For simple post-invocation quality checks — run a linter, run tests, format code — a well-written hook is probably sufficient and has less setup overhead. `ail` is the right tool when you want: reproducibility across a team, a structured audit trail, HITL gates that pause rather than fail, portability across runners, and a pipeline that can evolve over time. If you just need to run `cargo fmt` after every completion, use a hook.

**Proposed README section:** "Why not just use hooks?" — a short, honest comparison that acknowledges hooks, states the gap clearly, and does not oversell. Tone: confident, not defensive.

<<<<<<< HEAD
<<<<<<< HEAD
**Relationship to D-009 (named moat — now in spec/core/s01):** The moat answer and the hooks answer are the same answer stated differently. D-009 names the architectural guarantee; D-016 explains why hooks can't provide it. They should be written together.
=======
**Relationship to D-009 (naming the moat):** The moat answer and the hooks answer are the same answer stated differently. D-009 names the architectural guarantee; D-016 explains why hooks can't provide it. They should be written together.
>>>>>>> cbba6f6... wip
=======
**Relationship to D-009 (named moat — now in spec/core/s01):** The moat answer and the hooks answer are the same answer stated differently. D-009 names the architectural guarantee; D-016 explains why hooks can't provide it. They should be written together.
>>>>>>> ab1eba9... dontext: update decisions

---

### D-017 — "That's What CI Is For" — The Intervention Point Argument
<<<<<<< HEAD
<<<<<<< HEAD
**Status:** 🟡 Active
**Blocks:** README, positioning
**Depends on:** D-016

**The challenge:**
Max's second point: "on the job, that's what CI is for." At FAANG scale, CI is the quality gate. This is true and should be acknowledged.

**The response:**
=======
**Status:** 🟡 Active  
**Blocks:** README, positioning  
=======
**Status:** 🟡 Active
**Blocks:** README, positioning
>>>>>>> ab1eba9... dontext: update decisions
**Depends on:** D-016

**The challenge:**
Max's second point: "on the job, that's what CI is for." At FAANG scale, CI is the quality gate. This is true and should be acknowledged.

<<<<<<< HEAD
**The response:**  
>>>>>>> cbba6f6... wip
=======
**The response:**
>>>>>>> ab1eba9... dontext: update decisions
CI runs after a PR is opened — it catches problems *after* the human has already seen the output, formed an opinion, and often moved on. `ail`'s intervention point is earlier: the pipeline runs before the human sees the result. That is a fundamentally different moment.

The analogy: CI is the net. `ail` is the guardrail before the cliff. Both are useful; they operate at different points in the feedback loop.

Additional distinctions:
- CI requires a commit and a PR. Much agentic work happens within a session and never becomes a PR — or produces a series of small completions that each deserve quality gates before the next prompt is issued.
- CI is asynchronous — you wait for it. `ail` is synchronous — the human cannot proceed until the pipeline completes or explicitly overrides.
- CI cannot pause for human input mid-run. `ail`'s HITL model is a first-class feature.

<<<<<<< HEAD
<<<<<<< HEAD
**The honest caveat:**
=======
**The honest caveat:**  
>>>>>>> cbba6f6... wip
=======
**The honest caveat:**
>>>>>>> ab1eba9... dontext: update decisions
Max's point is correct at his scale and for his use case. For individual developers and small teams doing session-level agentic work, CI is often overkill for within-session quality gates. `ail` fills the gap between "I manually reviewed this" and "CI caught it after I opened a PR."

**Note:** This is not an argument that `ail` replaces CI. It complements it. The killer workflow (D-010) should include a note: "ail runs before you commit; CI runs after. Both matter."

---

<<<<<<< HEAD
<<<<<<< HEAD
=======
>>>>>>> ab1eba9... dontext: update decisions
## Theme 4 — Deferred Vision

### D-015 — CA/PKI Niche: Park, Don't Pursue
**Status:** ⚪ Deferred

**Reviewer suggestion:** Consider Attestable CA Operations as a specialized vertical.

**Assessment:** The audit trail / structured evidence angle is genuinely valuable and already implied by the pipeline run log. The CA/PKI vertical specifically requires domain credibility, conservative community buy-in, and compliance certifications that are not achievable at this stage. The right move is to build the structured evidence foundation well and let vertical applications emerge from users rather than pursuing them top-down.

**Deferred:** Revisit if a CA operator shows up wanting to use `ail` and is willing to collaborate on the vertical requirements.

---

<<<<<<< HEAD
### D-018 — Skill Token Optimization: Formally Park This
**Status:** ⚪ Deferred (explicitly — do not revisit before post-POC)

**Background:**
An early design idea was to detect which steps don't need skills loaded and call Claude with `--no-skill` to save tokens.

**Assessment:**
- One miss where a skill doesn't load and the agent does something unexpected = more expensive in human intervention cost than the tokens saved
- Inconsistent skill loading likely breaks prompt caching, making the economics worse
- At FAANG scale, human interventions are the primary cost metric — not token costs

**Formal decision:** This feature is parked. It does not appear in any roadmap, BUILD-PROMPT phase, or spec section. If token economics become a primary concern at scale, revisit with fresh eyes at that time — the answer may be a different architecture entirely (e.g., prompt caching strategy) rather than selective skill loading.

=======
=======
>>>>>>> ab1eba9... dontext: update decisions
### D-018 — Skill Token Optimization: Formally Park This
**Status:** ⚪ Deferred (explicitly — do not revisit before post-POC)

**Background:**
An early design idea was to detect which steps don't need skills loaded and call Claude with `--no-skill` to save tokens.

**Assessment:**
- One miss where a skill doesn't load and the agent does something unexpected = more expensive in human intervention cost than the tokens saved
- Inconsistent skill loading likely breaks prompt caching, making the economics worse
- At FAANG scale, human interventions are the primary cost metric — not token costs

**Formal decision:** This feature is parked. It does not appear in any roadmap, BUILD-PROMPT phase, or spec section. If token economics become a primary concern at scale, revisit with fresh eyes at that time — the answer may be a different architecture entirely (e.g., prompt caching strategy) rather than selective skill loading.

<<<<<<< HEAD
**Note for LEARNINGS.md:** This is worth a `[UNDOC]` entry — the reasoning for *not* building a seemingly clever optimization is as valuable as the reasoning for building features.

>>>>>>> cbba6f6... wip
=======
>>>>>>> ab1eba9... dontext: update decisions
---

### D-019 — Self-Modifying Pipelines: Preserve the Vision, Don't Build It Yet
**Status:** ⚪ Deferred (post-POC, significant design work required)

<<<<<<< HEAD
<<<<<<< HEAD
**Background:**
The most novel part of the `ail` vision is the self-modifying pipeline: a pipeline that can analyze the delta between frontier and commodity model outputs, generate improvements to its own YAML, test those improvements, optionally get human approval, and hot-reload. Over time the pipeline encodes the specific knowledge of your codebase, your org's style, your team's preferences — not as a memory file, but as executable, testable, version-controlled YAML steps.
=======
**Background:**  
The most novel part of the `ail` vision — articulated in the Slack conversation — is the self-modifying pipeline: a pipeline that can analyze the delta between frontier and commodity model outputs, generate improvements to its own YAML, test those improvements, optionally get human approval, and hot-reload. Over time the pipeline encodes the specific knowledge of your codebase, your org's style, your team's preferences — not as a memory file, but as executable, testable, version-controlled YAML steps.
>>>>>>> cbba6f6... wip
=======
**Background:**
The most novel part of the `ail` vision is the self-modifying pipeline: a pipeline that can analyze the delta between frontier and commodity model outputs, generate improvements to its own YAML, test those improvements, optionally get human approval, and hot-reload. Over time the pipeline encodes the specific knowledge of your codebase, your org's style, your team's preferences — not as a memory file, but as executable, testable, version-controlled YAML steps.
>>>>>>> ab1eba9... dontext: update decisions

This is distinct from and more powerful than:
- `CLAUDE.md` / memory files — those are static context, not executable logic
- Hooks — those cannot modify themselves
- LangGraph / CrewAI — those do not have the concept of a pipeline as a version-controlled, human-reviewable, self-improving artifact

<<<<<<< HEAD
<<<<<<< HEAD
<<<<<<< HEAD
=======
>>>>>>> 21dc1f1... wip:
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
=======
**Why this is genuinely novel:**  
The combination of: declarative pipeline as a first-class artifact + human-in-the-loop approval + version control + self-modification loop is not something any current tool provides. The pipeline is not just an orchestration mechanism — it is the accumulated operational intelligence of a team, expressed in a format that is readable, diffable, and improvable by both humans and agents.

**Why it cannot be built yet:**  
It requires: stable pipeline execution (not yet), hot reload (open question in spec), multi-runner parallel execution (not yet), structured output from comparison steps (not yet), and a HITL approval flow for pipeline modifications specifically (not yet designed). Building this before the foundation is solid would produce something fragile and unmaintainable.

**What to do now:**  
Preserve the vision explicitly in the spec (§22 planned extensions) and README so it is on record as the trajectory. The foundational decisions (D-001 through D-014) are all in service of this end state — they are not just "make a better hooks system." Every spec decision should be evaluated against: does this make the self-modifying pipeline easier or harder to build later?
>>>>>>> cbba6f6... wip
=======
**Why it cannot be built yet:**
It requires: stable pipeline execution (not yet), hot reload (open question in spec), multi-runner parallel execution (not yet), structured output from comparison steps (not yet), and a HITL approval flow for pipeline modifications specifically (not yet designed).

**What to do now:**
Preserve the vision explicitly in the spec (§22 planned extensions) and README so it is on record as the trajectory. The foundational decisions are all in service of this end state — they are not just "make a better hooks system." Every spec decision should be evaluated against: does this make the self-modifying pipeline easier or harder to build later?
>>>>>>> ab1eba9... dontext: update decisions

**Specific design seeds to capture for later:**
- The `on_result` model with structured branching is the primitive that will power the comparison step
- The pipeline run log is the evidence record that a self-modification loop would read to make decisions
- `FROM` inheritance is how a self-modification could layer changes without overwriting the base
- The HITL approval flow is how a human stays in control of pipeline evolution
- Version control of `.ail.yaml` is how changes are auditable and reversible

<<<<<<< HEAD
<<<<<<< HEAD
**Relationship to D-013 (non-goal — now in spec/core/s01):** Self-modifying pipelines are not general multi-agent orchestration. They are a specific, bounded application of the pipeline model to its own improvement.
=======
**Relationship to D-013 (non-goal):** Self-modifying pipelines are not general multi-agent orchestration. They are a specific, bounded application of the pipeline model to its own improvement. The non-goal is "become LangGraph." The goal is "build the foundation that makes self-improvement possible." These are compatible.
>>>>>>> cbba6f6... wip
=======
**Relationship to D-013 (non-goal — now in spec/core/s01):** Self-modifying pipelines are not general multi-agent orchestration. They are a specific, bounded application of the pipeline model to its own improvement.
>>>>>>> ab1eba9... dontext: update decisions

---

### D-020 — Parallel Sampling and Quality Comparison: Design Seed
<<<<<<< HEAD
<<<<<<< HEAD
**Status:** ⚪ Deferred (post-POC)
**Depends on:** D-019, multi-runner parallel execution

**Background:**
From the Slack conversation: the concept of sending an identical prompt to two runners (frontier + commodity), collecting both responses, and passing them to a quality comparison step. The comparison step identifies what the frontier did better and generates prompt improvements that could achieve similar results from the commodity runner.

**Why this matters beyond cost optimization:**
This is not primarily about saving money. It is about building a feedback loop that systematically improves the pipeline's ability to get good results from any runner. The commodity runner improves over time not because the model improves, but because the pipeline's prompts and skills get better at directing it.

=======
**Status:** ⚪ Deferred (post-POC)  
=======
**Status:** ⚪ Deferred (post-POC)
>>>>>>> ab1eba9... dontext: update decisions
**Depends on:** D-019, multi-runner parallel execution

**Background:**
From the Slack conversation: the concept of sending an identical prompt to two runners (frontier + commodity), collecting both responses, and passing them to a quality comparison step. The comparison step identifies what the frontier did better and generates prompt improvements that could achieve similar results from the commodity runner.

**Why this matters beyond cost optimization:**
This is not primarily about saving money. It is about building a feedback loop that systematically improves the pipeline's ability to get good results from any runner. The commodity runner improves over time not because the model improves, but because the pipeline's prompts and skills get better at directing it.

<<<<<<< HEAD
At scale, this becomes a mechanism for: benchmarking provider quality on your specific workloads, identifying which steps genuinely need frontier models vs which can be handled by cheaper runners, and generating the evidence trail for those decisions.

>>>>>>> cbba6f6... wip
=======
>>>>>>> ab1eba9... dontext: update decisions
**Design seed (not a spec yet):**
```yaml
- id: implement_frontier
  prompt: "{{ step.invocation.prompt }}"
  provider: frontier

- id: implement_commodity
  prompt: "{{ step.invocation.prompt }}"
  provider: commodity
<<<<<<< HEAD
<<<<<<< HEAD
  # runs in parallel with implement_frontier — requires parallel execution (planned)
=======
  # runs in parallel with implement_frontier — requires D-parallel
>>>>>>> cbba6f6... wip
=======
  # runs in parallel with implement_frontier — requires parallel execution (planned)
>>>>>>> ab1eba9... dontext: update decisions

- id: quality_compare
  prompt: |
    Compare these two implementations:
    Frontier: {{ step.implement_frontier.response }}
    Commodity: {{ step.implement_commodity.response }}
    What did the frontier do better? What prompt changes could
    achieve similar results from the commodity?
  on_result:
<<<<<<< HEAD
<<<<<<< HEAD
    match: always
    action: pause_for_human
    message: "Review quality comparison. Approve pipeline update?"
=======
    always:
      action: pause_for_human
      message: "Review quality comparison. Approve pipeline update?"
>>>>>>> cbba6f6... wip
=======
    match: always
    action: pause_for_human
    message: "Review quality comparison. Approve pipeline update?"
>>>>>>> ab1eba9... dontext: update decisions
```

**Deferred:** Requires parallel execution primitives that don't exist yet. Capture the concept, don't design it yet.

---

<<<<<<< HEAD
<<<<<<< HEAD
*Last updated: 2026-03-25. Active: D-005–D-007 (play by default), D-010 (killer workflow demo), D-012 (NativeRestRunner), D-016–D-017 (README hooks/CI copy). Blocking: D-016. Deferred: D-008, D-015, D-018–D-020.*
=======
*Last updated: 2026-03-25. D-001 through D-004, D-009, D-011, D-013, D-014 resolved and written to spec. Next actions: D-012 (NativeRestRunner — needed for Docker quickstart), then D-005/D-006/D-007/D-010 (play by default, killer workflow demo). D-016/D-017 (README hooks/CI copy) can be written anytime.*
>>>>>>> cbba6f6... wip
=======
*Last updated: 2026-03-25. Active: D-005–D-007 (play by default), D-010 (killer workflow demo), D-012 (NativeRestRunner), D-016–D-017 (README hooks/CI copy). Blocking: D-016. Deferred: D-008, D-015, D-018–D-020.*
>>>>>>> ab1eba9... dontext: update decisions
