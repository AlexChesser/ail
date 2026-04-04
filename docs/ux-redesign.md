# AIL UX Redesign — From First Principles

## Context

AIL currently presents itself as a pipeline orchestration tool that developers must consciously engage with. This design treats the human as the primary audience — who must reason about steps, monitor execution, and interpret results. The problem: this makes AIL *feel* like overhead rather than a hidden layer that just works better.

**The reframing:** AIL should be so transparent that developers forget it exists. It's not a tool you use; it's a better way your agent works. Like spark plugs in a car — the driver never thinks about them.

---

## Core Design Principles (DevX First)

### 1. **Zero-Knowledge Default**
- A developer with no `.ail.yaml` should have a *better experience*, not a degraded one
- AIL achieves this through sensible defaults and fallback behaviors
- Explicit configuration is opt-in for power users, not required for adoption

### 2. **Feedback is Ambient, Not Central**
- Status and progress should be visible in the margins — not dominating the output
- The primary output remains the agent's response to the human prompt
- Pipeline execution details are available for inspection but not forced

### 3. **Consumption > Configuration**
- Developers spend 1000x more time *using* a pipeline than writing it
- The CLI surface must optimize for execution, not authoring
- Pipeline editing should be rare, casual, and forgiving

### 4. **Work Backwards from the Agent Session**
- The Claude Code session is the user's primary interface
- `ail` exists to improve that session's behavior
- Every design decision should ask: "Does this make the session better or just add UI?"

### 5. **Parallel Execution Must Be Observable Without Overwhelming**
- When sub-agents or parallel steps run, the developer sees work happening
- But they don't have to manage it — they can dismiss details and trust the system
- Observability is on-demand, not intrusive

---

## User Personas & Scenarios

### Persona 1: **The Pragmatist** (60% of users)
*"I just want my agent to fix bugs better without me asking five times."*

**Scenario:** Developer runs Claude on a buggy feature. Agent produces a fix. Pipeline automatically:
- Runs tests
- Reviews the output against the original problem
- Re-prompts for a better approach if tests fail
- Returns the final result

**Current pain:** Doesn't know this is happening; can't tell if the pipeline helped or hurt
**Needed:** Transparent feedback that makes the pipeline invisible but its value obvious

---

### Persona 2: **The Optimizer** (30% of users)
*"I have specific workflows my team always does. I want to encode them once and forget about them."*

**Scenario:** Team has a standard pattern: write code → lint → security audit → ship
They configure a pipeline once. Every agent invocation automatically applies it.

**Current pain:** Lots of YAML maintenance; hard to share; tricky to customize per-project
**Needed:** Simple, shareable, composable pipeline definitions that fit naturally into team workflows

---

### Persona 3: **The Experimenter** (10% of users)
*"I want to see what happens when I wire up complex agent behaviors — maybe parallel work, maybe feedback loops."*

**Scenario:** Builds intricate pipelines with conditional branching, sub-agents, and feedback mechanisms
Wants to see and tweak every detail in real-time

**Current pain:** Hard to visualize pipeline structure; monitoring is scattered across logs and live output
**Needed:** Rich observability, live visualization, and instant feedback on pipeline changes

---

## Part 1: CLI UX Redesign

### Proposed CLI Behaviors

#### **Mode 1: Integrated with Claude CLI (Long-term)**
When `claude` CLI officially supports agent composition, `ail` becomes a first-class plugin:

```bash
# Developer's normal usage (no change)
$ claude "refactor this function"

# Behind the scenes:
# - Claude CLI detects .ail.yaml in CWD
# - Invokes ail as an after-step hook
# - Pipeline runs, results stream back
# - Developer sees unified output
```

#### **Mode 2: Transparent Wrapping (Immediate)**
A shell alias or direnv hook that makes `ail` invocation automatic:

```bash
# In .envrc or .bashrc (opt-in)
alias claude='ail-wrap claude'

# Now every claude invocation implicitly runs the pipeline
$ claude "refactor this function"
# → runs claude → runs .ail.yaml steps → returns unified output
```

#### **Mode 3: Explicit but Frictionless (Now)**
Keep explicit `ail --once` but make the experience dramatically better:

```bash
# What developers type (minimal overhead)
$ ail "refactor this function"  # discovers .ail.yaml automatically
# instead of: ail --once "refactor this function" --pipeline .ail.yaml
```

### Feedback Loop Redesign

#### **Level 1: Lean Mode (Default)**
Show only:
- The invocation response (main output)
- A one-line footer indicating pipeline executed (optional)

```
Here's the refactored code...

[ail: 2 steps in 3.2s]  ← optional, subtle
```

#### **Level 2: Summary Mode (--show-work)**
Shows what the pipeline *did*, not implementation details:

```
Here's the refactored code...

[pipeline summary]
✓ review  — Found 3 issues; requested fixes
✓ fix     — Applied suggestions
[ail: 2 steps in 3.2s, $0.47]
```

#### **Level 3: Watch Mode (--watch)**
Real-time streaming output as pipeline executes:

```
> Running review...
  [Thinking...] Found unused imports, poor naming...
  ✓ review (2.1s)

> Running fix...
  [Response...] Refactored imports, renamed variables...
  ✓ fix (1.8s)

Final response:
Here's the refactored code...
```

#### **Level 4: Parallel Execution (Future, requires --experimental)**
When parallel sub-agents are enabled, show a live panel:

```
[Main task]
Refactoring function... (60%)

[Background workers]
├─ lint     [████░░░░░░] 40%
├─ security [░░████░░░░] 20%
└─ review   [░░░░░░░░░░] waiting

Final response will appear here when complete...
```

### Pipeline Configuration Improvements

#### **Profile-Based Configuration**

```yaml
# .ail.yaml
profile: code-review  # Built-in profile: runs review, lint, test steps

customizations:
  review:
    prompt: "Check for edge cases in error handling"
  test:
    enabled: false    # Skip test step
```

**Profiles live in:**
```
~/.config/ail/profiles/
├── code-review.yaml      (review + lint + test)
├── security-audit.yaml   (security + supply-chain checks)
├── docs.yaml             (ensure docs match code)
└── custom.yaml           (per-organization)
```

#### **Composition & Inheritance**

```yaml
# .ail.yaml (project-level)
profile: code-review
# implicitly extends ~/.config/ail/profiles/code-review.yaml

customizations:
  review:
    prompt: "Check for edge cases..."
  
pipeline:
  - id: custom_security_check
    prompt: "Check for hardcoded secrets"
```

### Parallel Execution UX

```yaml
pipeline:
  - id: review
    prompt: "Review code quality"
  
  - id: security
    prompt: "Check for security issues"
    parallel_with: [review]  # Run alongside review, not after
    depends_on: invocation
  
  - id: tests
    prompt: "Are tests passing?"
    depends_on: [review, security]  # Wait for both, then run
```

### CLI User Stories

#### **US-1: Transparent Passthrough (MVP, v0.2)**
**As a** developer with a `.ail.yaml` file  
**I want** `ail "my prompt"` to work exactly like `ail --once "my prompt" --pipeline .ail.yaml`  
**So that** I don't have to type boilerplate flags every time

**Acceptance Criteria:**
- `ail <prompt>` discovers `.ail.yaml` and uses it
- Output is clean: just the final response + optional one-line footer
- `--show-work` flag reveals pipeline summary
- Default behavior is indistinguishable from plain Claude for a single-step pipeline

**Estimated Scope:** 2–4 hours

---

#### **US-2: Lean Feedback Modes (v0.2)**
**As a** developer iterating on a pipeline  
**I want** to choose how much pipeline detail I see (lean/summary/watch)  
**So that** I can work at the abstraction level that makes sense for my task

**Acceptance Criteria:**
- Default output shows only the final response
- `--show-work` shows a 2–3 line summary of what the pipeline did
- `--watch` streams real-time step output
- Step summary includes: outcome (pass/fail), decision made, next action

**Estimated Scope:** 3–6 hours

---

#### **US-3: Built-in Profile Support (v0.3)**
**As a** team lead  
**I want** to define a standard pipeline once and have all projects inherit it  
**So that** individual developers focus on project specifics, not pipeline engineering

**Acceptance Criteria:**
- `profile:` field in `.ail.yaml` references a named profile
- Profiles live in `~/.config/ail/profiles/` (XDG-compliant)
- A project `.ail.yaml` can override/extend inherited steps
- Built-in profiles: `code-review`, `security-audit`, `docs`, `unit-test`
- `ail materialize` shows the resolved pipeline with inheritance expanded

**Estimated Scope:** 4–8 hours

---

#### **US-4: Parallel Step Support (v0.4, Experimental)**
**As a** power user optimizing pipeline runtime  
**I want** to mark steps that can run in parallel  
**So that** the pipeline completes faster without forcing sequential execution

**Acceptance Criteria:**
- `parallel_with: [step_ids]` field allows declaring parallelism
- Executor validates dependency graph (detect cycles, missing dependencies)
- Steps run concurrently; step isolation is enforced
- Feedback shows all running steps in a unified view
- `--watch` mode shows a progress bar per parallel step

**Estimated Scope:** 8–16 hours

---

#### **US-5: Integrated Debugging (v0.4)**
**As a** developer with a failing pipeline  
**I want** to inspect a run with a structured viewer  
**So that** I can understand why a step failed and what happened

**Acceptance Criteria:**
- `ail debug <run_id>` opens interactive run viewer
- Shows: timeline, template variable resolutions, condition evaluations, costs
- Can navigate between steps and see their inputs/outputs
- Exports relevant details for bug reports
- Integrates with `ail logs --follow` for live debugging

**Estimated Scope:** 6–12 hours

---

#### **US-6: Shell Alias / Wrapper (v0.3, Optional)**
**As a** developer who wants `ail` to be truly invisible  
**I want** an optional `ail-wrap` shell alias that automatically integrates with `claude`  
**So that** I never have to think about typing `ail` explicitly

**Acceptance Criteria:**
- `ail-wrap` is a shell script (bash, zsh, fish)
- It detects if `.ail.yaml` exists in CWD
- If yes: runs `ail --once "<prompt>"` and shows unified output
- If no: passes through to the real `claude` command
- Optional config flag to disable the footer completely

**Estimated Scope:** 2–4 hours

---

## Part 2: VS Code Extension UX Redesign

### Core Principle: Code-Centric, Not Tool-Centric

The IDE should feel like a native feature, not a bolted-on tool. The daily driver should be:

```
Developer types code / encounters problem
       ↓
(single keystroke or right-click context menu)
       ↓
Invoke AIL with implicit context from editor
       ↓
Real-time feedback in editor margin (status, progress, results)
       ↓
Final pipeline result appears inline or in editor
       ↓
Developer can see what the pipeline did (optional breakdown)
       ↓
Developer continues coding
```

### Four UX Layers

#### **Layer 1: Zero-Click Invocation (Implicit)**

**A. Automatically Apply Pipeline to Current Session**

```
Developer (in terminal): $ claude "refactor this function"
[Claude responds with refactoring...]

Developer (switches to VS Code editor):
[Keybind: Ctrl+Alt+]]
"Applying pipeline to session abc123..."
[0.5s later]
[Editor shows inline diff of what the pipeline added]
```

**B. Context-Aware Auto-Trigger (Optional, Opt-In)**

```json
{
  "ail.autoTriggerOn": [
    "fileSave",           // Run pipeline whenever user saves a file
    "linterError",        // Run if file has linter warnings
    "testFailure",        // Run if tests are failing
    "claude-session-end"  // Run after Claude completes a task
  ]
}
```

#### **Layer 2: Feedback is Spatial, Not Modal**

**A. Inline Gutter Decorations (Status & Results)**

```
 1  │ function refactor() {      [✓ passed] ← review step passed
 2  │   const x = 1;
 3  │   return x * 2;            [⏳ running] ← test step in progress
 4  │ }                          [$ 0.012] ← cost of this run
```

**Features:**
- Green checkmark: Step passed
- Yellow spinner: Step running
- Red X: Step failed
- Dollar amount: Cost of the pipeline run
- Hover → tooltip with step summary
- Click → expand inline detail (peek box)
- Right-click → context menu (view logs, re-run, dismiss)

**B. Inline Diffs (When Changes Were Made)**

```
 1  │ function refactor() {
 2  │   const x = 1;       [changed by: fix-step]
 3  │ - const y = x * 2;
 4  │ + const y = x * 2;   [changed by: lint-step]
 5  │ }
```

**Features:**
- Colors indicate which step made the change
- Hover to see the step's response explaining the change
- Accept/reject individual changes
- View full diff in side-by-side editor view

**C. Peek Boxes for Rich Details**

```
 1  │ function refactor() {
 2  │ ╭─────────────────────────────────────────
 3  │ │ [Pipeline Summary]
 4  │ │ ✓ review (1.2s, 456→234 tokens)
 5  │ │   "Code is clean. Applied formatting."
 6  │ │
 7  │ │ ⏳ test (0.5s elapsed)
 8  │ │   Running unit tests...
 9  │ ├─────────────────────────────────────────
10  │ │ Cost: $0.012  │ Tokens: 690→234
11  │ │
12  │ │ [Dismiss] [View Full Logs] [Re-run]
13  │ ╰─────────────────────────────────────────
14  │   const x = 1;
```

#### **Layer 3: Sidebar Remains, But Refocused**

**A. Smart Tab Switching (Context-Aware Sidebar)**

**When developer is idle:**
```
[Chat View] ← Prominent, ready for new prompt
[Active Pipeline] ← Shows current .ail.yaml
[Recent Runs] ← Quick access to re-run
```

**When a pipeline is running:**
```
[Live Monitor] ← Real-time step updates
[Steps] ← Current step status
[Logs] ← Raw event stream (debug view)
```

**When developer is browsing past runs:**
```
[History] ← List of recent runs with cost, status, duration
[Pipeline Inspector] ← Shows the pipeline config for selected run
[Step Details] ← Dive into a specific step
```

**B. Consolidated Chat View (Redesigned)**

```
┌──────────────────────────────┐
│ 🤖 Extend Current Session     │  ← title
├──────────────────────────────┤
│ Pipeline: code-review        │  ← current active pipeline
│                              │
│ [Textarea for new prompt]    │  ← input area
│                              │
│ > Ctrl+Enter to run          │  ← help text
│ > Shift+Enter for multiline  │
│ > Ctrl+L to clear pipeline   │
│ > Ctrl+Alt+P to pick profile │
└──────────────────────────────┘
```

**C. Live Step Monitor (When Running)**

```
┌────────────────────────────────┐
│ Running: code-review           │  ← run title + timing
│ 2s elapsed                     │
├────────────────────────────────┤
│ ✓ review                       │  ← step status
│   Checked for issues           │
│   2.1s, 456→234 tokens         │
│                                │
│ ⏳ test (running...)           │
│   ${progress_percentage}%      │
│   0.5s elapsed...              │
│                                │
│ ⭘ fix (waiting)               │
│   (depends on: test)           │
│                                │
├────────────────────────────────┤
│ [Pause] [Cancel] [View Detail] │
└────────────────────────────────┘
```

#### **Layer 4: Deep Observability (Optional, Power User)**

**A. Pipeline Visualization Panel**

```
[Dependency Graph]

┌─────────────┐
│ invocation  │
└──────┬──────┘
       │
       ├─────────┬──────────┐
       ▼         ▼          ▼
    ┌─────┐  ┌──────┐  ┌──────────┐
    │review   │lint  │  │security  │
    └────┬────┘└──┬──┘  └────┬─────┘
         │        │          │
         └────────┼──────────┘
                  ▼
              ┌────────┐
              │  test  │
              └───┬────┘
                  ▼
              ┌────────┐
              │ commit │
              └────────┘

[Click a node to see details]
```

**B. Cost Attribution & Metrics**

```
[Cost Breakdown]

Total: $0.047 (1234→890 tokens)

By Step:
├─ review: $0.012 (456→234)
├─ test: $0.008 (280→123)
├─ security: $0.015 (342→289)
└─ commit: $0.012 (156→244)

Trend (last 7 days):
Average cost: $0.038
Most common profile: code-review (12 runs)
```

**C. Turn Log Inspector**

```
[Run Logs: abc123-def456]

[Filter: ________] [Export] [Copy]

[00:00] step_started: invocation
[00:12] stream_delta (234 chars)
[00:45] tool_use: execute...
[01:23] cost_update: $0.005
[02:10] step_completed: review (cost: $0.012)
[02:15] step_started: test
...

[Click line to expand] [View raw JSONL]
```

### IDE User Stories

#### **US-7: One-Keystroke Pipeline Invocation (v0.2, IDE)**
**As a** developer writing code  
**I want** to press a keyboard shortcut to invoke the pipeline  
**So that** I never switch contexts from the editor

**Acceptance Criteria:**
- `Ctrl+Alt+]` runs pipeline against current/previous prompt
- Keybinding is configurable in VS Code settings
- Works even if no active Claude session (uses default pipeline)
- Visual feedback appears within 100ms

**Estimated Scope:** 2–4 hours

---

#### **US-8: Gutter Decorations for Pipeline Status (v0.3, IDE)**
**As a** developer monitoring a pipeline  
**I want** inline visual indicators in the code editor  
**So that** I see results without leaving the editor

**Acceptance Criteria:**
- Checkmark, spinner, X icons appear in gutter based on step status
- Hover shows step summary (name, duration, result)
- Click expands a peek box with detailed info
- Gutter decorations are per-line (if code was modified by pipeline)
- Right-click context menu allows accept/reject/re-run

**Estimated Scope:** 4–8 hours

---

#### **US-9: Context-Aware Sidebar Views (v0.3, IDE)**
**As a** developer at different stages of a workflow  
**I want** the sidebar to show the most relevant view  
**So that** I never scroll to find what I need

**Acceptance Criteria:**
- When idle: Chat view is prominent
- When running: Live monitor is prominent
- When finished: Cost summary and logs are visible
- Smooth transitions (fade/slide, not jarring switches)
- All views are still accessible via tabs

**Estimated Scope:** 3–6 hours

---

#### **US-10: Pipeline Visualization (v0.4, IDE)**
**As a** power user designing pipelines  
**I want** to see the pipeline as a dependency graph  
**So that** I understand control flow and can edit it

**Acceptance Criteria:**
- New "Pipeline Graph" view in sidebar
- Nodes represent steps; edges show dependencies
- Node colors reflect execution status
- Click node to see/edit step config
- Right-click to disable, reorder, or delete steps
- Visualization updates in real-time during run

**Estimated Scope:** 6–12 hours

---

#### **US-11: Metrics & Cost Dashboard (v0.4, IDE)**
**As a** developer optimizing costs  
**I want** to see cost attribution and trends  
**So that** I understand which steps are expensive and can optimize

**Acceptance Criteria:**
- New "Metrics" view showing cost breakdown by step
- Comparison to historical averages
- Alert if cost exceeds threshold
- Trend over time (7-day rolling average)
- Export capability for budgeting

**Estimated Scope:** 3–6 hours

---

#### **US-12: Inline Diff View for Code Changes (v0.4, IDE)**
**As a** developer whose code was modified by the pipeline  
**I want** to see exactly what changed and why  
**So that** I can approve, reject, or understand the modification

**Acceptance Criteria:**
- Inline diff (strikethrough for deleted, underline for added)
- Diff color-coded by step
- Hover to see the step's explanation
- Stage/unstage individual changes
- Open side-by-side diff view for detailed comparison

**Estimated Scope:** 4–8 hours

---

## Implementation Phases

### **Phase 1: Feedback & Lean Modes (v0.2, ~1 week)**
- Implement transparent passthrough (`ail <prompt>`)
- Add `--show-work` and `--watch` modes
- Refactor text output formatting
- Preserve existing JSON output for integrations

### **Phase 2: Configuration Improvements (v0.3, ~2 weeks)**
- Implement profile-based configuration
- Add inheritance/extension support
- Build standard profiles (code-review, security, docs)
- Add `ail-wrap` shell alias

### **Phase 3: Advanced Execution (v0.4, ~4 weeks)**
- Parallel step support with dependency validation
- Live progress visualization for parallel tasks
- Integrated debugging viewer
- Cost attribution per step
- Live tail with filtering

### **Phase 4: Polish & Ecosystem (v0.5+)**
- TUI mode enhancements (live pipeline editor, visualization)
- Profile marketplace / community sharing
- IDE integrations (VS Code extension, JetBrains plugin)
- Anthropic official integration (if supported)

### **Phase 5: IDE-First (v0.2–0.5, parallel with CLI)**
- Phase 1 (CLI): One-keystroke invocation + basic status
- Phase 2 (IDE): Gutter decorations + peek boxes
- Phase 3 (IDE): Sidebar refactor + visualization
- Phase 4 (IDE): Metrics + advanced features

---

## Architecture: IDE-Centric Layers

```
┌──────────────────────────────────────────┐
│  Code Editor (Gutter, Inline Decoration) │
│  ├─ Gutter icons (status)               │
│  ├─ Inline diffs (changes)              │
│  └─ Peek boxes (details)                │
├──────────────────────────────────────────┤
│  Sidebar Views (Dynamic, Context-Aware)  │
│  ├─ Chat (input + profile selector)     │
│  ├─ Live Monitor (real-time steps)      │
│  ├─ History (past runs)                 │
│  ├─ Pipeline Graph (dependency viz)     │
│  └─ Metrics (cost + trends)             │
├──────────────────────────────────────────┤
│  Status Bar (Always Visible)             │
│  ├─ "Running..." indicator              │
│  ├─ Cost summary                        │
│  └─ Step count + duration               │
├──────────────────────────────────────────┤
│  Event Bus (Pub/Sub)                     │
│  ├─ Subscribes to RunnerService events  │
│  ├─ Broadcasts to all views             │
│  └─ Handles real-time updates           │
├──────────────────────────────────────────┤
│  RunnerService (Business Logic)          │
│  ├─ Command handlers                    │
│  ├─ Pipeline execution                  │
│  └─ Session management                  │
├──────────────────────────────────────────┤
│  AilProcess (Infrastructure)             │
│  ├─ Spawns ail binary                   │
│  ├─ NDJSON parsing                      │
│  └─ Process lifecycle                   │
└──────────────────────────────────────────┘
```

---

## Design Principles Applied

| Principle | How it's reflected |
|---|---|
| **Zero-Knowledge Default** | New users get working pipelines via profiles; no YAML expertise needed |
| **Feedback is Ambient** | Status confined to footers and optional flags; main output is unchanged |
| **Consumption > Configuration** | Both CLI and IDE optimized around running pipelines, not editing them |
| **Work Backwards from Agent** | Every feature asks "does this improve the Claude session?" |
| **Parallel is Observable** | Real-time progress shown clearly without overwhelming the user |

---

## Success Metrics

### CLI
1. **Adoption**: 70%+ of users with a `.ail.yaml` file use profiles (vs. custom YAML)
2. **Clarity**: Developers report they "forget ail is there" (qualitative)
3. **Engagement**: `--show-work` and `--watch` flags are used on ~40% of invocations (median)
4. **Performance**: Parallel pipelines reduce P50 wall-clock time by 30%+
5. **Trust**: Correct pipeline execution reported in >95% of cases (per turn log audit)

### IDE
1. **Adoption**: 80%+ of extension users use one-keystroke invocation within 2 weeks
2. **Time-to-Result**: Median time from "want to run pipeline" to "result visible" < 1 second
3. **Context Retention**: Developers spend <10% of time managing UI, 90% focused on code
4. **Visibility**: Developers can explain what each pipeline step did (vs. "ail just ran something")
5. **Preference**: 70%+ prefer IDE-based pipeline runs over CLI

---

## Open Questions for Design Review

### CLI
1. **Wrapper Adoption**: Should `ail-wrap` be the recommended way to use ail, or keep explicit `ail` command?
2. **Profile Naming**: Should profiles be more opinionated (e.g., `code-review-strict`) or more flexible?
3. **Parallelism**: Is implicit parallelization (auto-detect safe steps) safer than explicit declaration?
4. **Error Handling**: When a pipeline step fails, should it abort or ask the human in `--watch` mode?
5. **Cost Visibility**: How much cost detail should the default footer show?

### IDE
1. **Daemon Mode**: Should we invest in a background daemon for faster response times, or is the current process-per-run acceptable?
2. **Parallel Step Animation**: How should we visualize parallel steps without overwhelming the user?
3. **Auto-Trigger Scope**: Should auto-triggers (save, test failure) be per-profile, per-project, or global settings?
4. **Inline Diff Staging**: Should changes be auto-accepted or require explicit approval?
5. **Accessibility**: Are gutter decorations sufficient, or should we also support a text-based "result summary" view?
6. **Cost Display**: Should we warn users before running an expensive profile, or only retroactively?
7. **IDE Extension Variants**: Should we build extensions for JetBrains (IntelliJ, PyCharm) as well, or focus on VS Code first?

---

## Document History

- **2026-04-04**: Initial comprehensive UX redesign from first principles (CLI + IDE)
