This is exactly the right instinct. The architecture I laid out is a *target state*, not a starting point — and conflating the two is how projects die under their own weight before they prove anything.

Let me reframe this entirely around the question: **"What is the smallest thing that could teach us the most?"**

---

# `ail` Incremental Development Plan

## Guiding Principles

Before the phases, three meta-rules that govern every decision:

1. **Prove before polish.** No abstractions until we have concrete pain. The Hexagonal architecture, DI, and port/adapter patterns are *responses* to felt friction — not prerequisites.
2. **Each phase must produce something runnable.** Not a library, not a framework — something you can actually execute and observe.
3. **The reflection checkpoint is not optional.** It's where the real engineering happens. Skipping it to "keep momentum" is how technical debt compounds silently.

---

## Phase 0: The Spike — "Does the Core Idea Even Work?"

**Duration estimate:** 1–3 days
**Goal:** Answer one question only: *Can we wrap a CLI tool in a PTY, read its output, detect a prompt, and respond to it programmatically?*

**What we build:**
A single `main.rs`. No modules, no traits, no Redis, no TUI. Just:
- Spawn `aider` (or even just `bash`) inside a `portable-pty`
- Read stdout/stderr to the terminal in real time
- Detect a simple pattern (e.g., `"Continue? (y/n)"`)
- Respond automatically with `"y\n"`
- Exit cleanly

That's it. The entire "system" is one file, probably 150 lines.

**What we're actually learning:**
- Does `portable-pty` behave correctly on our target OS?
- Does the blocking-thread/channel pattern work in practice or does it have surprises?
- What does real aider output actually look like? Are our prompt detection assumptions correct?
- How does the process actually exit — cleanly, or does it leave orphans?

**Deliverable:** A binary that you can run and watch work. A short notes file (`SPIKE.md`) of what surprised you.

---

### 🔴 Reflection Checkpoint 0

Before writing a single line of Phase 1 code, answer these questions in writing:

1. Did the PTY wrapping actually work? What broke?
2. What did we learn about real CLI output that our architecture assumed incorrectly?
3. Is `portable-pty` the right library, or did we discover a better option?
4. What's the one riskiest unknown still ahead of us?

---

## Phase 1: A Loop You Can Watch

**Duration estimate:** 3–5 days
**Goal:** A working "dumb" orchestration loop — no intelligence, but observable and controllable.

**What we build:**
- Move PTY logic into its own module (`pty.rs`) with a clean internal interface
- Add a minimal Ratatui TUI: two panels — raw PTY output on the left, a log of events on the right
- Implement the simplest possible loop: *start tool → watch output → detect completion → report done*
- Add basic structured logging (JSON to stdout via `tracing` + `tracing-subscriber`)

**What we explicitly do NOT build yet:**
- Redis, PostgreSQL, Docker — anything infra
- LLM calls of any kind
- The Janitor, circuit breakers, budget tracking
- Any configuration beyond a single config struct initialized in `main.rs`

**Why:** We need to validate that the TUI/PTY event flow is pleasant to work with before we build anything on top of it. If the rendering feels wrong or the event loop is awkward, we want to know now.

**Deliverable:** You can run `ail` and watch it orchestrate a real CLI tool through a TUI. It's dumb but visible.

---

### 🔴 Reflection Checkpoint 1

1. Is the module boundary between `pty.rs` and the TUI clean? Or are they tangled?
2. Is the event model (`PtyEvent` enum) expressive enough, or are we already working around it?
3. Does the TUI feel like it has a future, or is Ratatui already fighting us?
4. What would someone unfamiliar with this codebase struggle to understand first?
5. **Refactor before proceeding:** If there are obvious structural improvements, make them now. The codebase is still small enough that cleanup is cheap.

---

## Phase 2: The First LLM Call

**Duration estimate:** 3–5 days
**Goal:** `ail` makes a real LLM API call and uses the response to drive a PTY tool.

**What we build:**
- A single `ModelProvider` trait (simple version — just `complete(prompt) -> Result<String>`)
- One concrete implementation: a direct `reqwest` call to an OpenAI-compatible endpoint (no proxy yet — just the API directly)
- A hardcoded "task": give `ail` a fixed objective, have it generate a shell command via LLM, execute it in the PTY, and report the output back
- Persist nothing — everything in memory for this phase

**What we explicitly do NOT build yet:**
- The proxy layer
- Multi-model routing
- The Janitor / context distillation
- Any feedback loops

**Why:** We need to validate the *core value proposition* — does LLM-driven PTY orchestration actually produce useful results? This is the proof of concept moment. If the answer is "not really," we need to know before we build the scaffolding around it.

**Deliverable:** You can give `ail` a goal in plain text, it calls an LLM, generates a command, runs it, and shows you the result. End-to-end, even if fragile.

---

### 🔴 Reflection Checkpoint 2

1. Did the LLM actually produce usable commands? What failure modes appeared immediately?
2. Is our `ModelProvider` trait the right shape, or does real usage reveal it needs to be different?
3. How does context passing feel? Is the amount of information we're sending to the LLM appropriate?
4. **This is the most important checkpoint:** Is the core loop — LLM decides, PTY executes — actually promising enough to keep building? Or do we need to rethink the fundamental approach?
5. What is the one thing that would most improve output quality right now?

---

## Phase 3: State That Survives

**Duration estimate:** 4–6 days
**Goal:** The loop can run multiple turns and remember what happened.

**What we build:**
- Add Redis (via docker-compose for the first time — just Redis, nothing else)
- Implement a simple `ContextManager` trait with two methods: `save(loop_id, context)` and `load(loop_id) -> Option<Context>`
- A naive `WorkingContext` struct: just a vec of previous exchanges, serialized to JSON
- Multi-turn loop: `ail` can now run → observe → update context → run again
- A `LoopState` enum with 4–5 states and a `can_transition_to()` guard

**What we explicitly do NOT build yet:**
- The Janitor / distillation (we'll hit context window limits, and that's fine — we want to *feel* that pain)
- PostgreSQL
- The meta-learning engine

**Why:** Multi-turn orchestration is qualitatively different from single-shot. We'll discover real problems here: context growth, state machine edge cases, tool failures mid-loop.

**Deliverable:** `ail` can run a 3–5 turn task, pick up where it left off if interrupted, and its state is in Redis.

---

### 🔴 Reflection Checkpoint 3

1. Did we hit context window limits? At what turn? How painful was it?
2. Is the state machine (`LoopState`) capturing the right states, or are we missing cases we've already encountered?
3. Is the Redis integration clean, or is it leaking into business logic?
4. What is the shape of data we're actually storing? Does `WorkingContext` reflect reality or our assumptions?
5. **Refactor candidate:** Is the `ContextManager` trait in the right place? Should it be split?

---

## Phase 4: The Janitor (Earned, Not Assumed)

**Duration estimate:** 4–5 days
**Goal:** Solve the context growth problem we discovered in Phase 3.

**Why now and not before:** We now have *real data* about what a working context actually looks like after 3–5 turns. We're not designing the Janitor against an imagined problem — we're building it against a concrete one. This is the most important thing.

**What we build:**
- The distillation prompt, tuned against actual working contexts we captured in Phase 3
- `ContextManager::distill()` — call an LLM to compress context between turns
- Token counting (via `tiktoken-rs` or equivalent) to measure actual reduction
- A simple quality check: log the before/after token counts and manually inspect a sample of distillations

**What we explicitly do NOT build yet:**
- The PID-controlled sampling
- Automated quality scoring
- YAML mutation

**Deliverable:** The loop can run 10+ turns without hitting context limits. Token reduction is measurable.

---

### 🔴 Reflection Checkpoint 4

1. What percentage reduction are we actually achieving? Is it meeting the >90% target or do we need to tune the prompt?
2. Is information actually being preserved? Run the same task with and without distillation — do outcomes differ?
3. How expensive is the distillation step? Is it cost-proportionate to the value it provides?
4. Does the Janitor prompt need to be different for different types of tasks?
5. **Vision check:** Are we building something that feels powerful and real, or does it still feel like a toy? What's missing that would make this feel serious?

---

## Phase 5: Observability & Safety Basics

**Duration estimate:** 4–6 days
**Goal:** Add the minimum viable circuit breakers and audit trail so we can run `ail` on real tasks without anxiety.

**What we build:**
- PostgreSQL (added to docker-compose now) with a simple `loop_events` table
- Every state transition writes an audit record
- Budget tracking: count tokens × known price, compare against a configurable limit
- A single circuit breaker: budget gate (`HalfOpen` when >80% of limit, `Open` when exceeded)
- HITL for the budget gate: pause the loop, display current spend in TUI, ask for approval to continue

**What we explicitly do NOT build yet:**
- Confidence breach detection
- High-risk command detection
- The meta-learning engine / parallel runs
- The proxy layer

**Deliverable:** You can run a real task, watch the spend accumulate, and `ail` will stop and ask you before it goes over budget.

---

### 🔴 Reflection Checkpoint 5

1. Is the audit log actually useful? Can you reconstruct what happened from it?
2. Is the HITL flow smooth or does it feel jarring? What would make it more natural?
3. What other things made you nervous while running real tasks that we haven't protected against yet?
4. **Scope check:** At this point we have a working, safe, multi-turn orchestrator with context management. Is this already useful? Could we demo this right now?

---

## Phase 6: The Proxy Layer

**Duration estimate:** 3–4 days
**Goal:** Route LLM calls through LiteLLM/Bifrost and gain visibility into what the providers are actually doing.

**Why deferred this long:** The proxy adds operational complexity. Until we have a stable loop running real tasks, we won't know what we actually need to observe. Now we do.

**What we build:**
- LiteLLM added to docker-compose
- `LiteLlmAdapter` replacing the direct API calls from Phase 2
- Response header inspection: capture `x-ratelimit-*`, content filter signals
- Emit these as structured log events
- A second model provider (e.g., add Claude alongside GPT) to validate that the `ModelProvider` trait actually abstracts correctly

**Deliverable:** All LLM traffic flows through the proxy. You can see what providers are doing. Swapping models requires changing one config value.

---

### 🔴 Reflection Checkpoint 6

1. Did the `ModelProvider` trait actually make swapping models easy, or did we have to change more than expected?
2. What did the proxy reveal that we couldn't see before? Any surprises?
3. Is the docker-compose topology manageable or getting unwieldy?
4. **Architecture review:** We now have all major infrastructure components. Does the hexagonal architecture feel natural given what we've built, or does it feel like we're forcing the shape?

---

## Phase 7: The Meta-Learning Engine (Deferred Until Here Intentionally)

**Duration estimate:** 6–10 days
**Goal:** Parallel prompting, critique, and the first automated quality feedback loop.

**Why last:** This is the highest-complexity, highest-risk module. We've deferred it because:
- We needed real working contexts to know what to critique
- We needed the proxy to route to multiple models
- We needed the audit log to measure quality
- We needed the Janitor to keep costs from exploding during parallel runs

Everything before this phase was prerequisite. Building it first (as the original architecture implied) would have been building on sand.

**What we build:**
- Parallel `commodity ‖ frontier` runs (with fixed sampling rate first — no PID yet)
- A structured critique prompt comparing the two outputs
- Manual review of critique quality before automating anything
- Quality score definition, tuned against real critique data
- PID controller for sampling rate — but only after we've validated the quality score is meaningful

**What remains explicitly deferred (post-MVP):**
- YAML mutation / prompt evolution (this is powerful but high-risk, and we'll have learned enough to design it properly)
- Confidence breach circuit breaker
- Automated git commits for mutations

---

### 🔴 Reflection Checkpoint 7 — The MVP Review

This is the big one. Before any further feature work:

1. **Does it work?** Can we run a real, non-trivial task end-to-end?
2. **Is it safe?** Budget gates, audit log, HITL — do we feel comfortable letting it run unsupervised for short periods?
3. **Is it observable?** If something goes wrong, can we diagnose it?
4. **Is the codebase navigable?** Could a new contributor understand the structure in a day?
5. **What did we learn that contradicts the original architecture?** What would we design differently?
6. **What is the shortest path to a demo?** What can we cut for a focused presentation of the core value?

---

## What We've Deliberately Deferred

These are real features, but they're post-MVP by design:

| Feature | Why Deferred |
|---|---|
| YAML / prompt mutation + git commits | High risk, requires validated quality signal first |
| Confidence breach circuit breaker | Need real data on what "low confidence" actually looks like |
| High-risk command detection | Needs a corpus of commands to classify against |
| Reconstruction challenge quality check | Refinement of the Janitor — baseline first |
| PID controller auto-tuning | Manual tuning first, automate friction points |
| Full 15-Factor compliance | Some factors (build/release/run separation) are ops concerns, not MVP concerns |

---

## The Meta-Point

The original architecture isn't wrong — it's a *correct description of the target*. But targets are navigated to, not jumped to. Each phase above is designed to generate **learnings that will change the design** in ways we can't predict now. The reflection checkpoints are where `ail` actually gets built — the coding phases just generate the raw material for those conversations.

The plan should feel lightweight enough that if Phase 2's reflection tells us the core loop isn't promising, we've lost 2 weeks, not 6 months.