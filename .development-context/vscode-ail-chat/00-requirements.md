# vscode-ail-chat: A Chat Interface for ail

> **Purpose:** This document is a planning input for Claude Code. The instruction is:
> *"Read this document in full, then build the extension described. Work phase by phase. Each phase must compile, run, and be testable before moving to the next."*

### Execution Strategy: Autonomous Overnight Run

This plan is designed to run autonomously overnight with minimal human intervention. The executing agent should follow these principles:

**Parallelism via subagents and worktrees.** Within each phase, identify independent work units and dispatch them to subagents working in isolated Git worktrees. The main orchestrator agent should preserve its context window for coordination — interface contracts, type definitions, test results — not implementation details. Subagents get deep but narrow context: one component, one test file, one fixture.

Phase 1 parallel work units:
- Subagent A: Extension scaffold (package.json, extension.ts, esbuild config)
- Subagent B: ChatMessage + ChatInput components + tests
- Subagent C: ToolCallCard + ThinkingBlock components + tests
- Subagent D: HitlCard component + tests
- Subagent E: styles.css (theme-aware, tested against light/dark)
- Merge point: App.tsx integrates all components with hardcoded sample data

Phase 2 parallel work units:
- Subagent A: types.ts (shared event types — this must be done FIRST, all others depend on it)
- Subagent B: ndjson-parser.ts + parser tests against all fixture files
- Subagent C: ail-process-manager.ts + process manager tests with mock subprocess
- Subagent D: Record NDJSON fixture files (requires a working ail binary + Ollama or stub runner)
- Subagent E: Webview message handler in App.tsx + integration tests
- Merge point: Wire process manager → postMessage → webview handler

Phase 3 parallel work units:
- Subagent A: HitlCard interactive behavior (button clicks → postMessage to extension host)
- Subagent B: Extension host stdin writer (receives postMessage → writes NDJSON to child process stdin)
- Subagent C: Permission request card (similar to HITL but different response types)
- Merge point: End-to-end test: replay hitl-gate.ndjson → card renders → click approve → stdin write verified

**The shared contract is `types.ts`.** Every subagent receives the same `types.ts` file defining the event types, message types between extension host and webview, and session types. This is the interface boundary. The orchestrator creates `types.ts` first, before dispatching any subagents.

**Self-check loop after every merge.** After merging subagent work from a phase:
1. Run `npm run compile` — must succeed with zero TypeScript errors
2. Run `npm test` — all tests must pass
3. If either fails, diagnose and fix before proceeding to the next phase
4. If a fix requires changing `types.ts`, re-validate all previously merged components against the updated types

**Fixture files are the test oracle.** If recorded NDJSON fixtures are not available (no ail binary accessible during the build), create synthetic fixtures by hand based on the event schema in section 2.1. These are valid JSON lines that match the documented schema — they don't need to come from a real ail run to be useful for testing. Real fixtures can replace them later.

**Checkpoint discipline.** Git commit after each successful phase merge. Tag each phase completion (`phase-1-complete`, `phase-2-complete`, etc.). If a later phase breaks something, the orchestrator can diff against the phase tag to isolate the regression.

---

## 1. What We're Building

A VS Code extension called `vscode-ail-chat` that provides a chat interface for `ail` (Alexander's Impressive Loops), a YAML-orchestrated pipeline runtime for LLM-powered CLI agents. The extension is a standalone webview panel that spawns `ail --once --output-format json` as a child process, streams NDJSON events from stdout, renders them as a chat conversation, and sends HITL responses back via stdin.

This is NOT the `vscode-ail` extension (which has a four-panel layout with YAML editors, pipeline browsers, etc.). This is a focused, single-purpose chat panel — modeled after the Claude Code, Copilot Chat, and Cursor chat interfaces visible in VS Code.

### 1.1 Visual Reference

The interface should look like a standard VS Code chat panel:
- Background and foreground colors drawn from VS Code's active theme via CSS variables (light, dark, high contrast — whatever the user has selected)
- User messages right-aligned or in a distinct bubble/block
- Assistant responses as streaming markdown with code blocks
- Tool calls shown as collapsible blocks with command name and output
- HITL gates shown as interactive cards with approve/reject buttons
- A text input area at the bottom with submit button
- A sessions list (sidebar or inline) showing past conversations

### 1.2 Architecture

```
┌─────────────────────────────────────────────┐
│  VS Code Extension Host (TypeScript)        │
│                                             │
│  ┌─────────────────────────────────────┐    │
│  │  AilProcessManager                 │    │
│  │  - spawns: ail --once --output-format json │
│  │  - reads: stdout (NDJSON events)    │    │
│  │  - writes: stdin (HITL responses)   │    │
│  │  - monitors: stderr (diagnostics)   │    │
│  └──────────┬──────────────────────────┘    │
│             │ postMessage                    │
│  ┌──────────▼──────────────────────────┐    │
│  │  Webview Panel                      │    │
│  │  - React or vanilla HTML/JS         │    │
│  │  - Renders chat messages            │    │
│  │  - Handles user input               │    │
│  │  - Shows HITL interactive cards     │    │
│  └─────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
```

The extension host manages the `ail` child process. The webview renders the UI. They communicate via `postMessage`. The `ail` binary is the stable contract — the extension never modifies it.

### 1.3 Non-Goals (Explicit)

- No YAML editor or pipeline browser
- No LSP or autocomplete for `.ail.yaml`
- No RAG or background indexing
- No conversational agent behavior — `ail` is the orchestrator, the extension is the renderer
- The extension ships with pre-compiled `ail` binaries (Intel Mac, Apple Silicon, Linux musl, Windows) for zero-config onboarding
- No Copilot dependency, no GitHub account requirement

---

## 2. The ail NDJSON Event Protocol

The extension consumes events from `ail --once "<prompt>" --output-format json` on stdout. Every line is a JSON object with a `"type"` field.

### 2.1 Events the Extension Must Handle

**Executor events (from ail itself):**
```json
{"type":"run_started","run_id":"a1b2c3d4-...","pipeline_source":".ail.yaml","total_steps":3}
{"type":"step_started","step_id":"review","step_index":0,"total_steps":3,"resolved_prompt":"Please review..."}
{"type":"step_completed","step_id":"review","cost_usd":0.003,"input_tokens":1234,"output_tokens":567,"response":"The code looks..."}
{"type":"step_skipped","step_id":"optional_check"}
{"type":"step_failed","step_id":"review","error":"Template variable not found"}
{"type":"pipeline_completed","outcome":"completed"}
{"type":"pipeline_error","error":"Step failed and on_error is abort_pipeline"}
```

**Runner events (from the underlying agent, forwarded by ail):**
```json
{"type":"runner_event","event":{"type":"stream_delta","text":"Hello, world!"}}
{"type":"runner_event","event":{"type":"thinking","text":"Let me analyze..."}}
{"type":"runner_event","event":{"type":"tool_use","tool":"Read","input":{"file_path":"./src/main.rs"}}}
{"type":"runner_event","event":{"type":"tool_result","tool":"Read","output":"fn main() {...}"}}
```

**HITL events:**
```json
{"type":"hitl_gate_reached","step_id":"human_review","message":"Please confirm deployment"}
{"type":"permission_requested","tool":"WebFetch","detail":"Fetch https://example.com"}
```

### 2.2 Stdin Control Protocol (Extension → ail)

The extension writes NDJSON lines to the child process's stdin:

```json
{"type":"hitl_response","step_id":"human_review","text":"Approved"}
{"type":"permission_response","allowed":true}
{"type":"permission_response","allowed":false,"reason":"Denied by user"}
{"type":"pause"}
{"type":"resume"}
{"type":"kill"}
```

---

## 3. Implementation Phases

### Phase 1: Scaffold & Static Chat UI

**Goal:** A VS Code extension with a webview panel that renders a static chat interface. No ail integration yet.

**Deliverables:**
1. Extension scaffold using `yo code` (TypeScript, no bundler initially — keep it simple)
2. A command `ail-chat.open` that opens a webview panel
3. The webview renders:
   - A chat message area (scrollable div)
   - A text input area at the bottom with a submit button
   - Hardcoded sample messages showing user prompt, assistant response with markdown, a tool call block, and a HITL card
4. CSS that matches VS Code's active theme (light, dark, high contrast) using CSS variables (`--vscode-editor-background`, `--vscode-editor-foreground`, etc.)
5. The webview uses React (TypeScript) bundled with esbuild

**Test:** Extension activates, command opens the panel, sample messages render correctly in both light and dark themes. Automated tests verify component rendering.

**Phase 1 Tests (automated, must pass in CI):**
- Unit tests for each React component: ChatMessage, ToolCallCard, HitlCard, ThinkingBlock render correctly given typed props
- Snapshot tests for message rendering across message types (user, assistant, tool_call, hitl_gate, thinking, step_status)
- Integration test: extension activates without errors, command is registered

**Files:**
```
vscode-ail-chat/
  package.json          # extension manifest
  tsconfig.json
  esbuild.js            # builds both extension and webview
  src/
    extension.ts        # activate(), registers command, creates webview
    webview/
      index.tsx         # React entry point
      App.tsx           # root component
      components/
        ChatMessage.tsx # renders a single message (user, assistant, tool call, HITL)
        ChatInput.tsx   # text input area with submit
        ToolCallCard.tsx # collapsible tool call display
        HitlCard.tsx    # interactive HITL gate card
      styles.css        # VS Code theme-aware styles
  .vscodeignore
```

### Phase 2: Process Management & Live Streaming

**Goal:** Spawn `ail` as a child process, stream NDJSON events, render them as chat messages in real time.

**Deliverables:**
1. `AilProcessManager` class that:
   - Discovers the `ail` binary (PATH or `ail.binaryPath` setting)
   - Spawns `ail --once "<prompt>" --output-format json --pipeline <path>` with the workspace root as cwd
   - Reads stdout line-by-line, parses JSON, posts typed messages to the webview
   - Captures stderr to the Output channel for diagnostics
   - Handles process exit (code 0 = success, non-zero = error)
   - Must call `.env_remove("CLAUDECODE")` on the subprocess to avoid Claude CLI's nested session guard
2. Webview receives messages and appends them to the chat:
   - `run_started` → shows a subtle "Pipeline started (3 steps)" indicator
   - `step_started` → shows "Step: review (1/3)" as a status line
   - `runner_event.stream_delta` → appends text to the current assistant message (streaming effect)
   - `runner_event.tool_use` → renders a collapsible tool call card showing tool name and input
   - `runner_event.tool_result` → adds the result to the tool call card
   - `runner_event.thinking` → renders in a collapsible "Thinking" block (dimmed/italic)
   - `step_completed` → shows cost/token summary below the step
   - `pipeline_completed` → shows total cost/duration summary, re-enables input
3. When the user submits a prompt:
   - Disable the input area
   - Show the user's message in the chat
   - Spawn `ail --once "<prompt>" --output-format json`
   - Stream results into the chat
   - Re-enable input when the pipeline completes or fails

**Critical implementation detail:** The `ail` process uses `--once` mode, so each prompt spawns a new process. Session continuity across prompts is handled by `ail` internally via `--resume <session_id>` between pipeline steps, but each user prompt is an independent `ail --once` invocation. The chat history is maintained in the webview only (messages accumulate visually).

**Test:** Type a prompt, see streaming output appear, see tool calls rendered, see cost summary at the end.

**Phase 2 Tests (automated, must pass in CI):**

The test strategy uses **recorded NDJSON fixtures** — real event sequences captured from actual `ail` runs (against Ollama or Claude), saved as `.ndjson` files in `test/fixtures/`. Tests replay these fixtures through the same code paths as a live run. This means:

1. **NDJSON parser tests:** Feed every fixture file through the parser line by line. Assert that each line produces the correct typed event object. Cover malformed lines (must skip gracefully, not crash), empty lines, and partial JSON.

2. **AilProcessManager tests with a mock process:** Replace `child_process.spawn` with a mock that emits pre-recorded NDJSON lines on stdout (from fixture files) on a realistic timer (e.g. 10ms between lines). Assert:
   - `run_started` event triggers the correct postMessage to the webview
   - `stream_delta` events accumulate into a complete assistant message
   - `tool_use` + `tool_result` pairs produce a complete tool call card message
   - `step_completed` events carry cost_usd and token counts
   - `pipeline_completed` triggers re-enablement of input
   - `step_failed` and `pipeline_error` produce error messages
   - Process exit code 0 vs non-zero handled correctly
   - stderr output captured to Output channel

3. **Webview message handler tests:** Feed the typed postMessage payloads (from step 2) into the React app's message handler. Assert that the component state updates correctly — right number of messages, correct types, correct content accumulation during streaming.

4. **stdin write tests:** Assert that when the webview sends an HITL approval, the extension host writes the correct NDJSON line to the child process stdin. Same for permission_response, pause, resume, kill.

5. **CLAUDECODE env removal test:** Assert that the spawned process environment does NOT contain the CLAUDECODE variable.

**Fixture files to create (record once, replay forever):**
```
test/fixtures/
  simple-prompt.ndjson          # single step, no tools, just text response
  multi-step-pipeline.ndjson    # 3 steps with streaming deltas, costs, tokens
  tool-calls.ndjson             # step with Read, Edit, Bash tool calls and results
  thinking-blocks.ndjson        # step with thinking events
  hitl-gate.ndjson              # pipeline that hits a pause_for_human gate
  permission-request.ndjson     # pipeline that hits a permission_requested event
  step-failure.ndjson           # step that fails mid-pipeline
  pipeline-error.ndjson         # pipeline that aborts
  malformed-lines.ndjson        # valid events mixed with garbage lines
```

**How to record fixtures:** Run `ail --once "<prompt>" --output-format json --pipeline <path> > test/fixtures/<name>.ndjson` against a local Ollama instance or Claude. These are committed to the repo and never change unless the ail event schema changes. They are the contract.

**Ollama recording setup (for reference):** The ail demo pipeline can target a local Ollama instance. The pipeline YAML needs:
```yaml
defaults:
  model: qwen3.5:0.8b
  provider:
    base_url: http://localhost:11434
```
Note: on WSL, localhost requires `networkingMode=mirrored` in `.wslconfig`. A small/fast model like `qwen3.5:0.8b` is ideal for fixture recording — the fixture captures the event *structure*, not the quality of the model's output.

**The extension only reads ail's NDJSON output.** It never communicates with Ollama, Claude, or any model provider directly. The fixture files are the complete contract between ail and the extension. If you can't run ail during the build, create synthetic fixtures by hand from the schema in section 2.1.

### Phase 3: HITL Gates & Interactive Cards

**Goal:** When `ail` emits a `hitl_gate_reached` or `permission_requested` event, render an interactive card in the chat that the user can respond to.

**Deliverables:**
1. HITL gate card:
   - Shows the gate message (e.g., "Please confirm deployment to production")
   - Three buttons: **Approve**, **Reject**, **Modify** (modify opens a text input)
   - When the user clicks a button, the extension writes the appropriate NDJSON line to the child process's stdin
   - The card transitions to a resolved state showing what the user chose
2. Permission request card:
   - Shows tool name and detail (e.g., "WebFetch: Fetch https://example.com")
   - Three buttons: **Allow**, **Deny**, **Allow for Session**
   - Writes `permission_response` to stdin
   - Card transitions to resolved state
3. While a HITL gate is pending:
   - The input area shows "Waiting for approval..." (not accepting new prompts)
   - The gate card is visually prominent (highlighted border, centered)

**Test:** Run a pipeline with a `pause_for_human` step. See the card appear. Click Approve. Pipeline continues.

**Phase 3 Tests (automated, must pass in CI):**
- Replay `hitl-gate.ndjson` fixture. Assert the HitlCard component renders with the correct message and buttons.
- Simulate clicking Approve. Assert the correct `hitl_response` NDJSON line is written to stdin with the right step_id.
- Simulate clicking Reject. Assert `hitl_response` with reject semantics.
- Simulate clicking Modify and entering text. Assert `hitl_response` carries the edited text.
- Replay `permission-request.ndjson`. Assert PermissionCard renders with tool name and detail.
- Simulate Allow/Deny clicks. Assert correct `permission_response` written to stdin.
- Assert that while a HITL gate is pending, the input area is disabled (component prop check).
- Assert that after gate resolution, the card shows resolved state (visual transition test).

### Phase 4: Sessions & History

**Goal:** Track multiple conversations, allow switching between them.

**Deliverables:**
1. Session model:
   - Each prompt submission creates or continues a "session"
   - Sessions are stored in the extension's `globalState` (VS Code's built-in persistence)
   - Each session stores: id, title (first prompt truncated), messages array, timestamps, total cost
2. Sessions list:
   - A collapsible panel above or beside the chat area
   - Shows session titles with timestamps
   - Click to switch between sessions
   - "New Session" button
3. Session titles:
   - Auto-generated from the first user prompt (truncated to ~50 chars)
   - Matches the pattern visible in Claude Code's sessions list

**Test:** Start multiple conversations. Switch between them. Sessions persist across VS Code restarts.

**Phase 4 Tests (automated, must pass in CI):**
- Create a session, add messages, serialize to globalState, deserialize back. Assert round-trip fidelity.
- Create three sessions. Assert SessionList component renders all three with correct titles and timestamps.
- Simulate switching sessions. Assert the chat area shows the correct message history for each.
- Assert session title generation: first user prompt truncated to ~50 chars.
- Assert "New Session" clears the current chat and creates a fresh entry.

### Phase 5: Pipeline Status & Step Progress

**Goal:** Show pipeline execution progress visually.

**Deliverables:**
1. Step progress indicator:
   - When a pipeline is running, show a compact progress bar or step list
   - Each step shows: name, status (pending/running/completed/failed/skipped), cost, duration
   - Running step has a spinner or pulsing indicator
   - Completed steps show a checkmark, cost, and token count
2. This can be rendered as:
   - A horizontal progress bar at the top of the chat during execution
   - Or inline step cards between messages (like Claude Code's "Finished with N steps" blocks)
   - Or a collapsible "Pipeline Progress" section

**Test:** Run a multi-step pipeline. See each step's status update in real time. See final cost breakdown.

**Phase 5 Tests (automated, must pass in CI):**
- Replay `multi-step-pipeline.ndjson`. Assert StepProgress component shows correct count, names, and status transitions (pending → running → completed/skipped/failed).
- Assert cost and token accumulation across steps matches the fixture data.
- Assert that the final pipeline_completed summary shows total cost and duration.

---

## 4. Testing Philosophy

**Every behavior described in this plan must have an automated test.** "It works when I try it manually" is not acceptable. The `vscode-ail` extension reached 162 passing tests and still had regressions because the tests weren't exercising real event flows.

### 4.1 Recorded Fixtures, Not Mocks

The core testing strategy is **recorded NDJSON fixtures**: real event sequences captured from actual `ail` runs, saved as `.ndjson` files in `test/fixtures/`. Tests replay these fixtures through the same parsing, state management, and rendering code paths as a live run.

Why fixtures, not mocks:
- Mocks test what you *think* the protocol looks like. Fixtures test what it *actually* looks like.
- When the `ail` event schema changes, you re-record fixtures and immediately see what breaks.
- Fixtures are human-readable — you can inspect them to understand exactly what sequence of events a test covers.
- An LLM reviewing the test suite can read the fixture files and understand the test coverage.

### 4.2 Test Layers

1. **NDJSON parser unit tests** — given a line of JSON, produce the correct typed event. Given malformed JSON, skip gracefully.
2. **Process manager integration tests** — given a mock subprocess emitting fixture lines, produce the correct sequence of postMessage calls.
3. **React component unit tests** — given typed props, render the correct DOM structure. Use React Testing Library.
4. **Webview state management tests** — given a sequence of postMessage events, produce the correct component state (message list, active HITL gates, step progress).
5. **Stdin write tests** — given a user action (approve HITL, deny permission), write the correct NDJSON line.
6. **Session persistence tests** — serialize/deserialize round-trip through VS Code's globalState mock.

### 4.3 CI Gate

The test suite runs on every commit. The extension does not ship if tests fail. Target: 90%+ line coverage on the extension host code, 80%+ on the webview components.

### 4.4 Test Framework

- Extension host tests: Vitest (or Jest) with VS Code extension test utilities
- React component tests: Vitest + React Testing Library + jsdom
- Fixture replay: custom test helper that reads `.ndjson` files and emits lines with configurable timing

---

## 5. Technical Constraints & Decisions

### 4.1 Webview Technology

Use **React** (with TypeScript) for the webview. This matches the industry standard — Claude Code, Cursor, and GitHub Next's official VS Code webview template all use React. Use **esbuild** as the bundler for the webview (fast, minimal config). No Create React App — just a straightforward esbuild setup that compiles `.tsx` files into a single bundled JS file for the webview.

For markdown rendering, use `marked` or `react-markdown`. Code blocks should use VS Code's syntax highlighting colors via CSS variables. Do not attempt to run a full syntax highlighter — just style `<pre><code>` blocks with monospace font and the appropriate background color.

The extension host side (process management, session persistence) remains plain TypeScript — React is only for the webview UI.

### 4.3 Theme Integration

The webview MUST respect VS Code's active color theme — light, dark, and high contrast. VS Code injects CSS variables into every webview automatically. Never hardcode colors. Use CSS variables exclusively:
```css
body {
  background-color: var(--vscode-editor-background);
  color: var(--vscode-editor-foreground);
  font-family: var(--vscode-font-family);
  font-size: var(--vscode-font-size);
}
```

Test against at least: the default Dark+ theme, the default Light+ theme, and one high contrast theme. If a color looks wrong in light mode, you've hardcoded something.

Key variables to use:
- `--vscode-editor-background` / `--vscode-editor-foreground`
- `--vscode-input-background` / `--vscode-input-foreground` / `--vscode-input-border`
- `--vscode-button-background` / `--vscode-button-foreground`
- `--vscode-badge-background` / `--vscode-badge-foreground`
- `--vscode-textLink-foreground`
- `--vscode-descriptionForeground` (for dimmed text)

### 4.4 Binary Discovery

The extension needs to find the `ail` binary. Resolution order (first match wins):
1. `ail.binaryPath` setting (explicit override — for contributors and custom builds)
2. `ail` on the system PATH (user's own installation takes precedence)
3. Bundled binary shipped with the extension (zero-config default for new users)

The extension ships pre-compiled `ail` binaries for Intel Mac, Apple Silicon, Linux (musl for static linking), and Windows. These are included in the extension package under a `bin/` directory, selected by platform/arch at activation time. The CI release pipeline uses `cross` for cross-compilation.

At activation, if using the bundled binary, the extension should verify it's executable (chmod +x on Unix) and log which binary was resolved to the Output channel. If a version mismatch is detected between the bundled binary and a user's PATH binary, log a warning but prefer the user's PATH binary (they likely have a newer build).

### 4.5 Pipeline Discovery

When spawning `ail`, the extension needs to know which pipeline to use:
1. If the workspace has a `.ail.yaml`, use it implicitly (ail discovers it automatically)
2. If the workspace has `.ail/default.yaml`, same
3. The user can specify via a setting `ail.pipelinePath`
4. If no pipeline exists, `ail` runs in passthrough mode — this is fine, the extension doesn't need to handle it specially

### 4.6 Error Handling

- If `ail` is not found: show an error notification with install instructions
- If `ail` exits with non-zero: show the stderr output in the chat as an error message
- If NDJSON parsing fails on a line: log it to the Output channel, skip the line, continue
- If the process crashes mid-stream: show "Pipeline interrupted" in the chat, re-enable input

### 4.7 Extension Activation

The extension activates on:
- The `ail-chat.open` command
- When a `.ail.yaml` file is present in the workspace (lazy — don't activate until the user opens the panel)

---

## 6. File Structure (Final)

```
vscode-ail-chat/
  package.json
  tsconfig.json
  esbuild.js                  # builds extension + webview bundle
  vitest.config.ts             # test configuration
  src/
    extension.ts              # activate(), command registration, webview provider
    ail-process-manager.ts    # spawns ail, reads NDJSON, writes stdin
    session-manager.ts        # persists sessions to globalState
    types.ts                  # shared types for ail events
    ndjson-parser.ts          # line-by-line NDJSON parser with error recovery
    webview/
      index.tsx               # React entry point
      App.tsx                 # root component, message state, postMessage handler
      components/
        ChatMessage.tsx       # single message renderer (user/assistant/system)
        ChatInput.tsx         # text input with submit button
        ToolCallCard.tsx      # collapsible tool call display
        HitlCard.tsx          # interactive HITL gate card with approve/reject/modify
        ThinkingBlock.tsx     # collapsible thinking/reasoning display
        StepProgress.tsx      # pipeline step progress indicator
        SessionList.tsx       # sidebar/inline session browser
      styles.css              # VS Code theme-aware styles (CSS variables only)
  test/
    fixtures/
      simple-prompt.ndjson          # single step, text response only
      multi-step-pipeline.ndjson    # 3 steps, streaming, costs, tokens
      tool-calls.ndjson             # Read, Edit, Bash tool calls
      thinking-blocks.ndjson        # thinking events
      hitl-gate.ndjson              # pause_for_human gate
      permission-request.ndjson     # permission_requested event
      step-failure.ndjson           # mid-pipeline step failure
      pipeline-error.ndjson         # pipeline abort
      malformed-lines.ndjson        # valid events mixed with garbage
    ndjson-parser.test.ts           # parser unit tests against all fixtures
    ail-process-manager.test.ts     # process manager with mock subprocess
    session-manager.test.ts         # session persistence round-trip
    webview/
      ChatMessage.test.tsx          # component render tests
      ToolCallCard.test.tsx
      HitlCard.test.tsx
      ThinkingBlock.test.tsx
      StepProgress.test.tsx
      SessionList.test.tsx
      App.test.tsx                  # full message flow integration test
  .vscodeignore
  README.md
```

---

## 7. What Success Looks Like

After all phases are complete:

1. A developer opens the `ail-chat` panel in VS Code.
2. They type "refactor the auth module for DRY compliance" and press Enter.
3. They see their message appear in the chat.
4. A "Pipeline started (3 steps)" indicator appears.
5. Step 1 starts. They see "Step: dry_refactor (1/3)" and then streaming text from the model as it works, with tool calls appearing as collapsible cards showing file reads and edits.
6. Step 1 completes. Cost and token count appear: "$0.003 · 1,847 tokens · 4.2s"
7. Step 2 starts. If it has a `pause_for_human` gate, an interactive card appears asking for approval. The developer clicks "Approve". The pipeline continues.
8. Step 3 completes. A final summary shows total cost across all steps.
9. The developer's input is re-enabled. They can type another prompt.
10. The session appears in the sessions list. If they close and reopen VS Code, the session history is still there.

---

## 8. Reference: Existing Codebase Context

### The ail binary
- Located wherever the user has built it (typically `target/release/ail`)
- CLI: `ail --once "<prompt>" --output-format json [--pipeline <path>]`
- Outputs NDJSON to stdout, diagnostics to stderr
- Stdin accepts NDJSON control messages when `--output-format json` is active
- Must have `CLAUDECODE` env var removed from child process to avoid nested session guard

### The vscode-ail extension (separate, leave it alone)
- Lives in `vscode-ail/` in the ail repo
- Has its own webview, YAML editor integration, pipeline browser
- We are NOT modifying or extending it
- We ARE learning from its patterns (especially the process spawning and NDJSON parsing)

### Key spec references
- `spec/core/s23-structured-output.md` — full NDJSON event schema and stdin protocol
- `spec/core/s13-hitl-gates.md` — HITL gate types and responses
- `spec/runner/r02-claude-cli.md` — Claude CLI runner details (session_id, --resume, env vars)
- `ARCHITECTURE.md §2.4` — separation of UI and core
- `ARCHITECTURE.md §6` — control plane / agent boundary