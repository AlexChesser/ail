# `ail` TUI Enhancement — Requirements & Planning Document

> **Purpose:** This document is a planning input for Claude Code. The instruction is:
> *"Read this document, then write a plan to build it against the existing `ail` Rust codebase."*

---

## 1. Context

`ail` (Alexander's Impressive Loops) is a pipeline orchestrator for LLM-powered CLI agents. It wraps tools like Claude CLI, Aider, and OpenCode, executing follow-up prompt chains after each human invocation. The codebase is Rust. The architecture enforces a hard boundary between `ail-core` (domain model, pipeline executor, runner adapters) and `ail` (the interactive TUI binary).

The current TUI (`tui.rs`, built on ratatui) provides minimal passthrough output — it streams runner events to the terminal and returns control to the human after the pipeline completes. The current execution model spawns one subprocess per pipeline step using `--resume <session_id>` to maintain session continuity.

This document specifies an enhanced TUI that makes pipeline execution **observable**, **interruptible**, and **navigable** — transforming `ail` from a pipeline executor you watch scroll by into an orchestration cockpit you operate.

### 1.1 Reference Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Human                             │
└────────────────────────┬────────────────────────────┘
                         │ prompt / interrupt / navigation
                         ▼
┌─────────────────────────────────────────────────────┐
│                   ail (TUI binary)                  │
│                                                     │
│  ┌─────────────┐    ┌──────────────────────────┐   │
│  │ Pipeline     │    │ TUI Renderer             │   │
│  │ Sidebar      │    │ - agent output viewport  │   │
│  │ (§3.2)       │    │ - prompt input area      │   │
│  │              │    │ - status bar              │   │
│  ├─────────────┤    │ - session navigator       │   │
│  │ Agent        │    └──────────┬───────────────┘   │
│  │ Sidebar      │               │                   │
│  │ (§4.2)       │               │                   │
│  └─────────────┘    ┌──────────┴───────────────┐   │
│                     │ ail-core                  │   │
│                     │ (unchanged — domain model,│   │
│                     │  pipeline executor,       │   │
│                     │  runner adapters)          │   │
│                     └──────────┬───────────────┘   │
└───────────────────────────────┬┼───────────────────┘
                                ││ stdin/stdout (NDJSON)
                                ▼▼
┌─────────────────────────────────────────────────────┐
│             Underlying Agent (runner)               │
│          e.g. Claude CLI, Aider, OpenCode           │
└─────────────────────────────────────────────────────┘
```

### 1.2 Architectural Constraints (Non-Negotiable)

These constraints protect the existing system and must not be violated by any phase of this work.

1. **`ail-core` is untouched.** All TUI enhancements live in the `ail` crate. If the core needs new event types, those are additive — never breaking changes to existing `RunnerEvent` variants or the `Runner` trait.
2. **Headless mode and `ail serve` are unaffected.** The `--headless` flag and future server mode consume the same `ail-core` event stream. TUI work must not introduce side effects that leak into non-interactive modes.
3. **The Runner trait and runner contract are unchanged.** The TUI is a *consumer* of `RunnerEvent`s, not a modifier of the runner interface.
4. **Session continuity uses `--resume <session_id>`, not persistent subprocesses.** Each pipeline step is a discrete subprocess invocation. The TUI makes this *feel* continuous by streaming output in real-time, but the underlying execution model is unchanged.
5. **Single-agent is the default.** Multi-agent features are opt-in and introduced in later phases.

### 1.3 Architecture Principles

Derived from the agentic orchestration research and adapted for `ail`:

- **The agent proposes, the human decides.** HITL is not an inconvenience to be minimized — it is the product's core value proposition. Every point where the pipeline pauses for human input should be a well-designed interaction, not a raw stdin prompt.
- **Glanceable state over verbose logs.** A developer should be able to assess pipeline health with peripheral vision. Visual indicators (glyphs, colors, status labels) replace wall-of-text parsing.
- **Progressive disclosure.** Show the minimum information needed at each zoom level. The pipeline sidebar shows step status at a glance. Focusing on a step reveals its output. Drilling into a session reveals the full transcript.
- **Observability is a first-class UX concern, not a debugging afterthought.** Cost, token usage, step duration, and agent state are surfaced in the primary interface, not buried in log files.
- **The interface is decoupled from the execution.** The TUI renders events. It does not control execution flow. This means every TUI feature is also expressible via the `ail serve` SSE stream for the future web UI — no TUI-only capabilities that can't be replicated elsewhere.

---

## 2. Visual Language Specification (All Phases)

The visual language is defined upfront because it governs the design of every component. It is an MVP deliverable.

### 2.1 Pipeline Step Glyphs

Rendered in the left sidebar. Each glyph is a single fixed-width character or character pair.

| State | Glyph | Description |
|---|---|---|
| Not yet reached | `○` | Hollow circle. Step exists but pipeline hasn't reached it. |
| Active / Running | `●` | Filled circle. Step is currently executing. |
| Completed (success) | `✓` | Check mark. Step finished with exit code 0. |
| Completed (skipped) | `⊘` | Circle with slash. Step's `condition:` evaluated false; intentionally skipped. |
| Disabled (user) | `⊖` | Circle with minus. User manually disabled this step for the current session via `Space`. |
| Failed | `✗` | X mark. Step exited with non-zero code or timed out. |
| Paused (HITL gate) | `◉` | Bullseye / double circle. Step is blocked waiting for human input. |
| Branched (on_result) | `◇` | Diamond. Step completed and triggered a branch via `on_result`. |

**Color coding** (where terminal supports it):

| State | Color |
|---|---|
| Not yet reached | Dim / grey |
| Active | Bright white or cyan (bold) |
| Completed (success) | Green |
| Skipped (condition) | Dim yellow |
| Disabled (user) | Dim magenta / strikethrough if terminal supports it |
| Failed | Red |
| Paused (HITL) | Bright yellow (pulsing if terminal supports it) |
| Branched | Blue |

### 2.2 Agent Status Indicators

Rendered in the right sidebar (multi-agent phases) or status bar (single-agent MVP).

| State | Indicator | Description |
|---|---|---|
| Running | `▶ AGENT_NAME` | Agent subprocess is active and streaming output. |
| Awaiting response | `⏳ AGENT_NAME` | Agent has been invoked; waiting for LLM response to begin streaming. |
| Blocked (HITL) | `⏸ AGENT_NAME` | Agent is paused, awaiting human permission or input. |
| Idle | `○ AGENT_NAME` | Agent session exists but no active step is running. |
| Completed | `✓ AGENT_NAME` | Agent has finished all assigned work. |
| Error | `✗ AGENT_NAME` | Agent subprocess exited with an error. |

### 2.3 Layout Semantics

The TUI uses a three-region layout:

```
┌──────────────┬─────────────────────────────────┬──────────────┐
│  PIPELINE    │                                  │  AGENTS      │
│  SIDEBAR     │     MAIN VIEWPORT                │  SIDEBAR     │
│              │                                  │  (Phase 2+)  │
│  Step list   │  Active agent output stream      │              │
│  with glyphs │  or session history browser      │  Agent list  │
│              │  or step detail HUD (§3.2.2)     │  with status  │
│              │                                  │              │
├──────────────┴─────────────────────────────────┴──────────────┤
│  STATUS BAR — agent name | step N/M | cost | tokens | elapsed │
├───────────────────────────────────────────────────────────────┤
│  > PROMPT INPUT AREA                                          │
└───────────────────────────────────────────────────────────────┘
```

**MVP (Phase 1):** Pipeline sidebar + Main viewport + Status bar + Prompt input. No right sidebar.

**Phase 2+:** Right sidebar added for agent list. Main viewport gains split/tab capability.

### 2.4 Terminal Width Degradation

| Terminal width | Behavior |
|---|---|
| ≥ 120 cols | Full layout with pipeline sidebar |
| 100–119 cols | Pipeline sidebar collapses to glyph-only (no step names) |
| 80–99 cols | Pipeline sidebar hidden; step status shown in status bar only |
| < 80 cols | Minimal mode — output stream + prompt only (current behavior) |

---

## 3. Phase 1 — MVP: Observable, Interruptible Single-Agent TUI

**Goal:** A single-agent pipeline runner with a persistent REPL, live pipeline visualization, and user-initiated interrupt capability. This is the "I can use this every day" milestone.

### 3.1 Core REPL Loop

**Current behavior:** Human enters prompt → runner executes → pipeline steps fire sequentially → control returns to human.

**Enhanced behavior:** The same execution model, but the TUI remains active and interactive throughout. The human sees:

1. Their prompt echoed in the main viewport.
2. The runner's response streaming in real-time (text deltas, tool calls, thinking traces if extended compliance runner).
3. The pipeline sidebar updating step glyphs as each step begins, completes, or is skipped.
4. The status bar showing the active step name, cost accumulator, token count, and elapsed time.
5. The prompt input area re-activating when the pipeline completes or when a HITL gate opens.

**The REPL does not exit between invocations.** The human can issue multiple prompts in a session. Each prompt triggers a fresh pipeline traversal. Session history scrolls upward in the main viewport (like a chat log, but with interleaved pipeline step outputs).

### 3.2 Pipeline Sidebar

The left panel displays the pipeline step list using the visual language from §2.

```
  ail runner
  ○ lint
  ○ security_audit
  ○ test_writer
  ○ commit_review
```

During execution:

```
  ail runner
  ✓ lint
  ● security_audit    ← currently active, highlighted
  ○ test_writer
  ○ commit_review
```

After completion:

```
  ail runner
  ✓ lint
  ✓ security_audit
  ⊘ test_writer       ← skipped (condition was false)
  ✓ commit_review
```

**Focus model:** The sidebar accepts focus via `Tab` (cycle between sidebar and prompt input) or by pressing `j`/`k`/`↑`/`↓` when the prompt input is empty. When focused, the currently highlighted step is indicated by a cursor or reverse-video highlight. Navigation within the sidebar uses `j`/`↓` (next step) and `k`/`↑` (previous step) interchangeably. Pressing `q` or `Tab` returns focus to the main viewport / prompt input.

**Reset behavior:** When a new human prompt is entered, all step glyphs reset to `○` before the pipeline begins again — except user-disabled steps, which retain their `⊖` state across invocations within the same session.

#### 3.2.1 Space-to-Disable (Session-Level Step Toggle)

**Key:** `Space` (while a step is highlighted in the sidebar)

Pressing `Space` on a not-yet-reached step toggles it between enabled (`○`) and user-disabled (`⊖`). This is a **session-level** flag — it does not modify the `.ail.yaml` file on disk. The flag persists across pipeline invocations within the same `ail` session but is lost when `ail` exits.

```
  ail runner
  ○ lint
  ○ security_audit
  ⊖ test_writer       ← user disabled this step for this session
  ○ commit_review
```

**Behavior:**
- When the pipeline executor reaches a user-disabled step, it skips it identically to a step whose `condition:` evaluated false. The glyph remains `⊖` (not `⊘`) so the user can distinguish "I turned this off" from "the pipeline logic skipped this."
- `Space` is a toggle. Pressing it again on a `⊖` step re-enables it to `○`.
- `Space` on a completed (`✓`), failed (`✗`), or actively running (`●`) step is a no-op. You cannot disable a step that has already executed or is currently executing.
- `Space` on a HITL-paused (`◉`) step is a no-op.
- The status bar briefly flashes a confirmation: `step "test_writer" disabled for this session` or `step "test_writer" re-enabled`.

**Use case:** During rapid development iteration, a developer may not need the security audit or commit review on every cycle. Rather than editing the pipeline file, they press `Space` to skip expensive steps while in flow state, and re-enable them when ready for a full run.

**Non-goal:** This is not an editing mechanism. The `.ail.yaml` file is never modified. If the user wants to permanently remove a step, they edit the file. This toggle is the pipeline equivalent of commenting out a line temporarily.

#### 3.2.2 Step Detail HUD (Inspection Mode)

When the sidebar is focused and a step is highlighted, pressing `Enter` (or simply dwelling on a step for > 300ms, implementation-dependent) opens the **Step Detail HUD** in the main viewport. This overlays the agent output stream — it does not replace it permanently. Pressing `q`, `Escape`, or moving the sidebar highlight dismisses the HUD and returns the viewport to the agent output stream.

The HUD displays the step's configuration as parsed from `.ail.yaml`, rendered as a readable summary — not raw YAML. The content varies by step type:

**For a `prompt:` step:**

```
┌─ Step: security_audit ────────────────────────────────┐
│                                                        │
│  Type:     prompt                                      │
│  Provider: claude (default)                            │
│  Model:    (default)                                   │
│  Timeout:  120s                                        │
│  Resume:   true                                        │
│                                                        │
│  Prompt:                                               │
│  ┌────────────────────────────────────────────────┐    │
│  │ Review the code changes for security            │    │
│  │ vulnerabilities. Check for SQL injection,       │    │
│  │ XSS, and credential exposure. If none found,   │    │
│  │ respond CLEAN.                                  │    │
│  └────────────────────────────────────────────────┘    │
│                                                        │
│  on_result:                                            │
│    contains "CLEAN" → continue                         │
│    else → pause_for_human                              │
│                                                        │
│  Tools:                                                │
│    allowed: Read, Grep, Glob                           │
│    denied:  Bash, Write                                │
│                                                        │
│  Status: ○ not yet reached                             │
│                                                        │
│  [Space] disable  [q/Esc] close  [↑↓] browse steps    │
└────────────────────────────────────────────────────────┘
```

**For a `skill:` step:**

```
┌─ Step: commit ────────────────────────────────────────┐
│                                                        │
│  Type:     skill                                       │
│  Skill:    ./skills/commit/                            │
│  Timeout:  120s                                        │
│                                                        │
│  Skill body (from SKILL.md):                           │
│  ┌────────────────────────────────────────────────┐    │
│  │ Review staged changes and generate a            │    │
│  │ conventional commit message. Follow the          │    │
│  │ project's commit conventions in CLAUDE.md.       │    │
│  └────────────────────────────────────────────────┘    │
│                                                        │
│  Status: ○ not yet reached                             │
│                                                        │
│  [Space] disable  [q/Esc] close  [↑↓] browse steps    │
└────────────────────────────────────────────────────────┘
```

**For a `context:` step (shell):**

```
┌─ Step: gather_deps ───────────────────────────────────┐
│                                                        │
│  Type:     context (shell)                             │
│  Command:  cargo tree --depth 1                        │
│  Timeout:  30s                                         │
│                                                        │
│  Status: ○ not yet reached                             │
│                                                        │
│  [Space] disable  [q/Esc] close  [↑↓] browse steps    │
└────────────────────────────────────────────────────────┘
```

**For a completed step:** The HUD additionally shows:
- Duration (wall clock)
- Token usage (input / output)
- Cost
- A `[view output]` hint — pressing `Enter` again switches to the step's output transcript in the viewport (same as `Ctrl+P`/`Ctrl+N` navigation)

**Template variables:** If the prompt contains `{{ }}` template expressions, they are shown as-is (unexpanded) before execution, and shown expanded (with the resolved values) after execution. This lets the user understand both what the template *says* and what it *produced*.

**Long prompts:** If the prompt or skill body exceeds the viewport height, it is scrollable within the HUD box using arrow keys. The HUD footer shows `[↑↓ scroll]` when content overflows.

**Read-only:** The HUD is strictly a viewer. No editing of step configuration is supported. This is a deliberate non-goal for all phases — if editing is ever added, it will be a separate feature with its own confirmation and validation flow.

### 3.3 Status Bar

A single-line bar between the main viewport and the prompt input area.

Format:
```
▶ claude | step 2/4: security_audit | $0.0032 | 1,847 tok | 12.4s
```

Fields:
- Runner name (from `Runner::name()`)
- Current step index and name (or "idle" when awaiting human input)
- Cumulative cost for this invocation (from `cost_usd` in runner result events)
- Cumulative token count
- Wall-clock elapsed time since the human prompt was submitted

When idle (awaiting human input):
```
○ claude | idle | session: a1b2c3 | last run: $0.0128 | 4 steps
```

### 3.4 Prompt Input Area

A multi-line input area at the bottom of the screen.

**Features (MVP):**
- Standard text editing (cursor movement, backspace, delete, word-jump with Ctrl+Left/Right)
- History recall with Up/Down arrows (previous prompts in this session)
- `/` prefix triggers skill discovery (list available skills with arrow-key selection) — matches existing `ail` REPL behavior per §6 of the spec
- `@` prefix triggers file path completion — matches the convention established by Claude Code and Opencode
- `Enter` submits the prompt; `Shift+Enter` or `Alt+Enter` inserts a newline for multi-line input
- Input is disabled while a pipeline step is actively running (re-enabled on HITL gate, pipeline completion, or `Escape` interrupt — see §3.5)

### 3.5 User-Initiated Interrupt

**Key:** `Escape`

The interrupt system is the TUI's most critical interaction. It must be instant, forgiving, and safe. Pressing `Escape` at any point during pipeline execution — including mid-stream text output, mid-tool-call, or mid-OODA reasoning loop — immediately pauses the pipeline and opens an **interrupt modal**.

#### 3.5.1 Immediate Pause Behavior

1. The TUI sends a pause signal to the pipeline executor (not SIGINT — a cooperative pause).
2. The runner subprocess is **not killed**. It remains alive but its output is buffered rather than streamed to the viewport. This is critical: the agent's in-flight work is preserved, not discarded.
3. The main viewport freezes at the last-rendered line. A visible interrupt banner appears:

```
────────────── ⏸ PAUSED ──────────────
  Escape: resume unchanged (oops!)
  Type guidance + Enter: inject and resume
  Ctrl+K: kill this step
──────────────────────────────────────
```

4. The prompt input area activates below the banner.

#### 3.5.2 Three Paths Out of an Interrupt

**Path A — "Oops" / Resume Unchanged** (`Escape` again)

The user hit `Escape` by accident, or looked at the output and decided everything is fine. A second press of `Escape` instantly dismisses the interrupt modal, un-buffers any accumulated runner output, and resumes live streaming exactly where it left off. Zero side effects. The pipeline state is identical to what it would have been if the user had never pressed `Escape`.

This must be the lowest-friction path. One keypress, no confirmation, instant return to the live stream. Accidental interrupts cost the user less than one second.

**Path B — Inject Guidance and Resume** (type text + `Enter`)

The user sees something they want to steer — a wrong direction in the OODA loop, a tool call targeting the wrong file, a reasoning path that's about to waste tokens. They type guidance into the prompt input area and press `Enter`.

The guidance is handled depending on the runner's state at pause time:

- **If the runner is between tool calls (reasoning / generating text):** The guidance is injected as a user-turn message appended to the session. The runner's next inference cycle sees it as new context. The session continues — it is not restarted.
- **If the runner is mid-tool-call (e.g., executing a shell command, reading a file):** The tool call is allowed to complete (to avoid corrupted state), and the guidance is injected immediately after the tool result returns, before the next reasoning cycle begins.
- **If the runner does not support mid-session injection:** The guidance is prepended to the *next* step's prompt as appended system context, and the current step is allowed to complete unmodified. The status bar indicates: `guidance queued for next step`.

After injection, the interrupt modal dismisses and live streaming resumes. The viewport shows a visible marker in the output stream: `── ✎ user guidance injected ──` so the user can see exactly where their intervention landed in the transcript.

**Path C — Kill Step** (`Ctrl+K`)

The user decides the current step is unsalvageable. `Ctrl+K` during the interrupt modal kills the runner subprocess and advances the pipeline to `on_error` handling. This is the destructive path.

#### 3.5.3 Destructive Action Warning

When the user's input during an interrupt (Path B) or at the REPL prompt would **discard significant in-progress work**, the TUI must warn before proceeding. "Significant" is defined as:

- The current pipeline run has completed 2+ steps with non-trivial output (> 500 tokens of accumulated response), **and**
- The user's action would reset or fork the pipeline (e.g., entering a new top-level prompt while steps are still running, or injecting guidance that contradicts the active step's instructions to the degree that a restart is implied).

The warning is presented as a confirmation modal:

```
────────────── ⚠ WORK IN PROGRESS ──────────────
  This pipeline run has completed 3 steps ($0.0089).
  Your input will restart the pipeline from step 1.

  Enter: confirm and restart
  Escape: cancel and return to interrupt modal
────────────────────────────────────────────────
```

The warning does **not** fire for:
- Simple guidance injections that steer without resetting (Path B, normal case)
- Aborting a single step via `Ctrl+K` (Path C — the user already chose the destructive action explicitly)
- Interrupts where less than 500 tokens of work have been produced (not enough to worry about)

The token threshold (500) and step threshold (2) are configurable in `~/.config/ail/config.yaml` under an `interrupt:` key, with sensible defaults.

#### 3.5.4 Interrupt During Tool Permission Prompts

If the runner emits a `PermissionRequest` (asking the user to approve a tool call), `Escape` still works. The interrupt modal appears *on top of* the permission prompt. The three paths behave identically. If the user resumes unchanged (Path A), the permission prompt re-presents itself. This ensures the interrupt key always works, regardless of what the runner is waiting for.

#### 3.5.5 Implementation Notes

- **Cooperative pause signal.** The TUI signals `PauseRequested` to `ail-core` via a shared atomic flag or async channel. The pipeline executor checks this flag between event emissions and enters a paused state. This is an additive change to `ail-core` — it does not modify the `Runner` trait.
- **Output buffering during pause.** While paused, the runner subprocess continues executing (it doesn't know it's paused — it's a separate process). The TUI buffers all `RunnerEvent`s received during the pause. On resume (Path A), the buffer is flushed to the viewport. On guidance injection (Path B), the buffer is flushed and then the injection occurs. On abort (Path C, `Ctrl+K`), the buffer is discarded.
- **Mid-session injection.** This depends on the runner supporting appended user turns to a live session. The Claude CLI supports this via `--resume` with additional input. Runners that don't support this degrade to "guidance queued for next step" behavior. The `Runner` trait does not need to change — the adapter handles injection mechanics internally.
- **The interrupt modal is a TUI-only concern.** In headless mode and `ail serve`, the equivalent is an API endpoint for injecting guidance into a paused step. The `ail-core` pause/resume mechanism is shared; only the presentation differs.

### 3.6 Session Navigation

**Keys:** `Ctrl+P` (previous session/step), `Ctrl+N` (next session/step)

In Phase 1, this navigates between **pipeline steps within the current invocation**. The main viewport shows the output of the selected step. The active/live step is always the default view.

- `Ctrl+P` moves focus to the previous step's output (scrollable history).
- `Ctrl+N` moves focus to the next step's output, or back to the live stream if at the most recent step.
- The pipeline sidebar highlights the currently focused step.
- A `[viewing: step_name]` indicator appears in the status bar when viewing historical step output (not the live stream).

### 3.7 HITL Gate Presentation

When the pipeline encounters a `pause_for_human` gate or the runner emits an `AskUserQuestion` payload:

1. The step glyph changes to `◉` (bullseye).
2. The status bar updates to `⏸ HITL — step: security_audit — "Security issues detected. Review before proceeding."`
3. The gate's message is rendered prominently in the main viewport, visually distinct from agent output (e.g., bordered box, different color).
4. The prompt input area activates with a contextual hint: `> respond to gate (Enter to approve, type to provide feedback):`
5. Pressing `Enter` with no input approves / continues.
6. Typing a response and pressing `Enter` sends the response as feedback to the runner.

### 3.8 MVP Keybinding Summary

| Key | Context | Action |
|---|---|---|
| `Enter` | Prompt input | Submit prompt / approve HITL gate |
| `Enter` | Interrupt modal (text entered) | Inject guidance and resume (Path B) |
| `Enter` | Destructive action warning | Confirm and proceed |
| `Enter` | Sidebar focused (step highlighted) | Open Step Detail HUD for highlighted step |
| `Shift+Enter` | Prompt input | Insert newline |
| `Escape` | During step execution | Interrupt — pause and open interrupt modal |
| `Escape` | Interrupt modal | "Oops" — resume unchanged instantly (Path A) |
| `Escape` | Destructive action warning | Cancel — return to interrupt modal |
| `Escape` | Step Detail HUD | Close HUD, return to agent output stream |
| `Space` | Sidebar focused (step highlighted) | Toggle step disabled/enabled for this session |
| `Tab` | Any | Cycle focus: prompt input → sidebar → prompt input |
| `Ctrl+K` | Interrupt modal | Kill current step — hard kill (Path C) |
| `Ctrl+C` | Idle / prompt input | Exit `ail` |
| `Ctrl+P` | Any (not in modal/HUD) | Navigate to previous step output |
| `Ctrl+N` | Any (not in modal/HUD) | Navigate to next step output |
| `Up` / `Down` | Prompt input | Prompt history recall |
| `Up` / `Down` | Step Detail HUD | Scroll long prompt/skill body content |
| `j` / `k` / `↑` / `↓` | Sidebar focused | Scroll step list (also moves HUD if open) |
| `/` | Prompt input (first char) | Skill discovery picker |
| `@` | Prompt input | File path completion |
| `q` | Sidebar focused / HUD open | Return focus to main viewport |

---

## 4. Phase 2 — Multi-Agent Awareness

**Prerequisite:** Phase 1 is stable and in daily use.

**Goal:** Surface multi-agent orchestration (Claude Code sub-agents, Agent Teams, or multiple runners in a pipeline) in the TUI without requiring the user to manage tmux panes.

### 4.1 Multi-Agent Execution Model

When a pipeline step spawns sub-agents (either via Claude Code's `/agents` system or via `ail`'s own future parallel step support), `ail-core` emits agent lifecycle events:

- `agent_spawned { agent_id, agent_name, parent_step_id }`
- `agent_output { agent_id, event: RunnerEvent }`
- `agent_completed { agent_id, summary, cost_usd }`
- `agent_error { agent_id, error }`

**Additive `ail-core` change:** New `RunnerEvent` variants for agent lifecycle. These are emitted by extended-compliance runners that support sub-agent telemetry. Runners that don't support this simply never emit these events — the TUI degrades to Phase 1 behavior.

### 4.2 Agent Sidebar (Right Panel)

The right panel appears only when multi-agent events are detected.

```
  AGENTS
  ▶ lead-agent
  ⏳ security-auditor
  ▶ test-writer
  ○ code-reviewer
```

**Interaction:**
- `Ctrl+P` / `Ctrl+N` now cycle through agents (in addition to steps within an agent). The navigation order is: agents first (if multiple), then steps within the focused agent.
- Alternatively (to be determined during implementation): `Alt+1` / `Alt+2` / `Alt+N` for direct agent selection, `Ctrl+P` / `Ctrl+N` for step navigation within the selected agent.
- Selecting an agent switches the main viewport to that agent's output stream.
- The pipeline sidebar updates to show the steps relevant to the selected agent.

### 4.3 1-Up Display (Default)

The main viewport shows one agent's output at a time. The agent sidebar indicates which agent is focused. Switching agents with `Ctrl+P`/`Ctrl+N` swaps the viewport content.

This is the default and recommended mode. It avoids the terminal-width problem of side-by-side splits and aligns with the Agent of Empires "zoom in / zoom out" interaction pattern.

### 4.4 2-Up Display (Optional)

Activated via a keybinding (e.g., `Ctrl+W, 2` or a toggle command). Splits the main viewport vertically, showing two agent streams side-by-side.

**Terminal width gate:** Only available when terminal width ≥ 160 columns. Below that threshold, the keybinding is a no-op and the status bar shows a brief message: `"terminal too narrow for split view"`.

### 4.5 Agent Session History

When an agent completes, its output remains browsable. `Ctrl+P`/`Ctrl+N` can navigate to completed agents. Their sidebar indicator shows `✓` and the main viewport displays their full session transcript (scrollable).

This is the foundation of the historical log browsing capability requested in the original vision.

---

## 5. Phase 3 — Historical Session Browser & Observability

**Prerequisite:** Phase 2 is stable.

**Goal:** Allow the user to browse past pipeline runs, not just the current one. Surface cost and performance telemetry as a primary UX concern.

### 5.1 Run Log Integration

`ail-core` already writes NDJSON run logs (per §4.4 of the spec). The TUI gains a "run history" mode that reads these logs and renders them using the same viewport, sidebar, and glyph system used for live execution.

**Entry point:** A keybinding (e.g., `Ctrl+H` for "history") opens a run list. The user selects a past run, and the TUI replays it as a read-only view — pipeline sidebar shows final step states, main viewport shows the transcript, status bar shows final cost/tokens/duration.

### 5.2 Cost & Token Dashboard

An overlay or dedicated view (toggled via a keybinding, e.g., `Ctrl+T` for "telemetry") showing:

- Cost breakdown per step for the current/selected run
- Token usage per step (input vs. output)
- Cumulative cost across recent runs (today, this week)
- Cost comparison between runs (useful for detecting prompt regressions)

This addresses the "cost anxiety" identified in the Autonomy analysis as a primary adoption barrier. Making cost a glanceable, always-available metric — not a post-hoc log analysis task — is the goal.

### 5.3 Step Execution Timeline

A horizontal timeline visualization (within the terminal, using box-drawing characters) showing step durations as proportional bars. Useful for identifying slow steps and optimizing pipeline performance.

```
lint           ████░░░░░░░░░░░░░░░░░░░░░░░░░░  2.1s
security_audit ████████████████████░░░░░░░░░░░  8.4s
test_writer    ⊘ skipped
commit_review  █████████░░░░░░░░░░░░░░░░░░░░░  4.2s
                                        total: 14.7s
```

---

## 6. Phase 4 — Advanced Pipeline Visualization

**Prerequisite:** Phase 3 is stable.

**Goal:** Represent complex pipeline topologies (branching, sub-pipelines, parallel steps) in the sidebar.

### 6.1 Branch Visualization

When `on_result` triggers a branch, the sidebar renders the branch path using box-drawing characters:

```
  ✓ lint
  ◇ security_audit
  │ ├─ ✓ fix_vulnerabilities    ← branch taken (if_false path)
  │ └─ ⊘ continue               ← branch not taken (if_true path)
  ✓ test_writer
  ✓ commit_review
```

### 6.2 Sub-Pipeline Visualization

When a step invokes a sub-pipeline (per §9 of the spec), the sidebar renders it as a collapsible tree:

```
  ✓ lint
  ✓ quality_gates              ← sub-pipeline step
  │ ├─ ✓ type_check
  │ ├─ ✓ coverage_check
  │ └─ ✓ style_check
  ✓ deploy
```

### 6.3 Parallel Step Visualization

When future parallel step support lands in `ail-core`, the sidebar renders concurrent steps:

```
  ✓ lint
  ┬ parallel_review            ← parallel group
  ├─ ✓ security_audit          ← ran concurrently
  ├─ ✓ perf_audit              ← ran concurrently
  └─ ✓ accessibility_audit     ← ran concurrently
  ✓ deploy
```

---

## 7. Phase 5 — Ambient & Decoupled Operation

**Prerequisite:** `ail serve` (v0.2+) is stable.

**Goal:** Decouple the observation interface from the execution environment, enabling ambient monitoring.

### 7.1 Remote TUI Attachment

`ail` gains the ability to attach to a running `ail serve` instance:

```bash
ail attach --server localhost:7823 --session abc123
```

The TUI renders identically to a local session, but all events come via the SSE stream instead of local subprocess I/O. This allows a developer to start a long-running pipeline on a powerful machine and monitor it from a laptop.

### 7.2 Notification Integration

When running in detached/server mode, `ail` can push HITL gate notifications to external channels:

- Desktop notifications (via `notify-send` or equivalent)
- Webhook (configurable URL for Slack, Discord, Signal bots)

The human responds via the TUI (local or attached) or via the `ail serve` API.

### 7.3 Mobile / Web Monitoring

The `ail serve` Phase 2 web UI subscribes to the same SSE stream and renders the same visual language (adapted from ASCII to browser-native glyphs and colors). This is a separate deliverable but shares the design language defined in §2.

---

## 8. Non-Goals (Explicitly Out of Scope)

These items have been considered and deliberately excluded from all phases:

- **On-the-fly step editing.** The Step Detail HUD (§3.2.2) is read-only. Editing step prompts, timeouts, tool permissions, or other config fields from within the TUI is explicitly deferred. If this capability is needed, it will be a separate feature with its own confirmation, validation, and undo flow. The current design philosophy is: the `.ail.yaml` file is the source of truth; the TUI inspects and toggles, it does not edit.
- **Pixel art / gamified visualization.** The TUI uses structured ASCII/Unicode glyphs, not animated sprites. Gamified visualization is a separate product concern (see Pixel Agents, Star-Office-UI).
- **Voice input.** OctoAlly-style Whisper STT integration is outside `ail`'s scope.
- **Provider-agnostic runner implementation.** Already handled by the runner contract and adapter architecture. The TUI does not care which runner is active.
- **Persistent runner subprocesses.** Session continuity is achieved via `--resume <session_id>`. The TUI creates the *illusion* of persistence by streaming output continuously; the underlying model is still one-subprocess-per-step.
- **Web UI.** Covered by `ail serve` roadmap (§12 of ARCHITECTURE.md). The TUI and web UI share the same event model but are separate codebases.
- **Direct modification of `ail-core` domain model for TUI concerns.** The TUI adapts to `ail-core`, not the reverse. Additive event types (Phase 2 agent lifecycle events) are acceptable; changes to existing types are not.

---

## 9. Implementation Sequence & Dependencies

### Phase 1 (MVP) — Target: First Usable Build

**No dependency on any unbuilt `ail-core` feature except the cooperative pause signal.**

| Order | Component | Depends On | Estimated Complexity |
|---|---|---|---|
| 1 | Visual language constants (glyphs, colors) | Nothing | Low |
| 2 | Layout manager (three-region, width-responsive) | ratatui basics | Medium |
| 3 | Pipeline sidebar (static rendering from parsed .ail.yaml) | Config parsing (exists) | Low |
| 4 | Status bar (static fields) | Nothing | Low |
| 5 | Prompt input area (basic editing, Enter to submit) | Nothing | Medium |
| 6 | Main viewport (streaming runner output) | Runner adapter (exists) | Medium |
| 7 | Pipeline sidebar live updates (glyph state transitions) | Pipeline executor events (exists) | Medium |
| 7a | Sidebar focus model (Tab to cycle, j/k/arrows to navigate) | §3 sidebar + §5 prompt input | Medium |
| 7b | Space-to-disable (session-level step toggle) | §7a + pipeline executor skip logic | Medium |
| 7c | Step Detail HUD (read-only step config overlay) | §7a + config parsing (exists) | Medium-High |
| 7d | HUD: template variable display (raw vs. expanded) | §7c + template resolution (exists in spec) | Low |
| 7e | HUD: completed step metadata (duration, cost, tokens) | §7c + runner result events | Low |
| 8 | Status bar live updates (cost, tokens, elapsed) | Runner result events (exists) | Low |
| 9 | Session navigation (Ctrl+P / Ctrl+N between steps) | Step output buffering | Medium |
| 10 | HITL gate presentation | HITL gate events (exists in spec, partial impl) | Medium |
| 11a | Interrupt: cooperative pause signal in `ail-core` | ail-core control channel (additive) | Medium |
| 11b | Interrupt: output buffering during pause | §11a + main viewport buffer | Medium |
| 11c | Interrupt: Path A — "oops" resume (Escape→Escape) | §11b (flush buffer on resume) | Low |
| 11d | Interrupt: Path B — guidance injection and resume | §11b + runner adapter injection support | High |
| 11e | Interrupt: Path C — kill step (Ctrl+K in modal) | §11a (subprocess kill + on_error) | Medium |
| 11f | Interrupt: destructive action warning modal | §11d + token/step accounting | Medium |
| 11g | Interrupt: `── ✎ user guidance injected ──` marker in viewport | §11d | Low |
| 12 | Prompt history, `/` skill picker, `@` file completion | Skill discovery (exists in spec) | Medium |

### Phase 2 — After MVP Stabilization

| Component | Depends On |
|---|---|
| Agent lifecycle event types in `ail-core` | Core team agreement on event schema |
| Agent sidebar (right panel) | Agent lifecycle events |
| Agent-scoped Ctrl+P / Ctrl+N navigation | Agent sidebar |
| 2-up split view (optional) | Terminal width detection |

### Phase 3 — After Phase 2 Stabilization

| Component | Depends On |
|---|---|
| Run log reader | NDJSON run log format (exists in spec §4.4) |
| Historical session browser (Ctrl+H) | Run log reader |
| Cost/token dashboard overlay | Run log reader + live cost events |
| Step execution timeline | Step duration data in run logs |

### Phase 4 — After Phase 3 Stabilization

| Component | Depends On |
|---|---|
| Branch visualization in sidebar | `on_result` implementation in `ail-core` |
| Sub-pipeline tree rendering | §9 sub-pipeline support in `ail-core` |
| Parallel step rendering | Future parallel step support in `ail-core` |

### Phase 5 — After `ail serve` v0.2

| Component | Depends On |
|---|---|
| Remote TUI attachment | `ail serve` SSE stream |
| Notification integration | Webhook/notification infrastructure |
| Shared design language export for web UI | §2 visual language formalized |

---

## 10. Success Criteria

### Phase 1 (MVP)
- A developer can run `ail` and see their pipeline steps visualized with glyphs in real-time.
- A developer can press `Escape` mid-step — including mid-tool-call — to pause execution instantly.
- An accidental `Escape` is recoverable with a second `Escape` in under one second, with zero side effects.
- A developer can type guidance during an interrupt and have it injected into the active OODA loop (or queued for the next step if the runner doesn't support mid-session injection).
- Injected guidance is visibly marked in the output stream so the user can see where their intervention landed.
- A destructive action warning fires before the user discards significant in-progress work (configurable thresholds).
- A developer can browse pipeline steps in the sidebar and press `Enter` to see a step's full configuration (prompt text, tools, timeout, on_result rules) in a HUD overlay.
- A developer can press `Space` on a step to disable it for the current session — skipping it on subsequent pipeline runs without editing the `.ail.yaml` file.
- User-disabled steps are visually distinct from condition-skipped steps (`⊖` vs `⊘`).
- Disabled steps persist across invocations within a session but do not survive `ail` exit.
- A developer can use `Ctrl+P` / `Ctrl+N` to review what each step produced.
- HITL gates render as structured, prominent interactions — not raw text prompts.
- The status bar shows cost and token count at all times.
- The TUI gracefully degrades at narrow terminal widths.
- Headless mode (`--headless`) is completely unaffected.

### Phase 2
- When a runner emits sub-agent events, the agent sidebar appears automatically.
- A developer can switch between agent output streams without leaving the TUI.
- Completed agent sessions remain browsable.

### Phase 3
- A developer can browse past runs and inspect what each step produced.
- Cost trends are visible across runs.

### Phase 4
- Complex pipelines with branches and sub-pipelines render as readable tree structures.

### Phase 5
- A developer can start a pipeline on machine A and monitor it from machine B.