# VS Code Extension: Issues #4 + #6 Merged Implementation Plan

## Context

The `vscode-ail` extension is a working proof of concept with solid DI architecture, NDJSON event streaming, and basic UI panels. Issues #4 and #6 describe the path from PoC to daily driver. #4 focuses on infrastructure (HITL gates, interrupt/resume, bidirectional communication). #6 focuses on UX (turn-based history, step decomposition, developer utility features). This plan merges both, triaging what's done, what's blocked, and what to build.

## Issue #4 Triage

| Item | Status | Notes |
|---|---|---|
| Headless execution | DONE | `AilProcess.invoke()` passes `--headless` |
| Execution Monitor | DONE | `ExecutionPanel` webview with steps, streaming, cost |
| HITL gate approval/rejection | DONE | Full bidirectional chain implemented; `on_result: pause_for_human` also unblocks in controlled executor |
| Path A interrupt (resume) | DONE | stdin `pause`/`resume` messages wired to `ExecutionControl` atomics |
| Path B guidance injection | DONE | `hitl_response` carries optional `text` field to executor |
| Path C hard kill | DONE | `AilProcess.cancel()` sends SIGTERM/SIGKILL |
| YAML highlighting | DONE | `ail-pipeline` language contribution |
| Pipeline Explorer | DONE | `PipelineTreeProvider` |
| Command palette | DONE | 11 commands |
| Pipeline History | NOT STARTED | |
| Multi-pipeline concurrency | NOT STARTED | Single-process constraint |

## Issue #6 Feature Triage

| Feature | Verdict | Phase |
|---|---|---|
| History Rail + Stage layout | BUILD | 2 |
| Chat clears on send | BUILD | 0 |
| Step decomposition (collapsible Thinking/Response) | DONE | - |
| Auto-collapse finished steps | BUILD | 2 |
| Prompt rehydration ("The Fork") | BUILD | 3 |
| Inspected Payload toggle | BUILD (needs Rust event enrichment) | 2 |
| Resource telemetry chips | BUILD | 2 |
| Active Editor Bridge (`{{ selection }}`) | BUILD | 2 |
| Shadow Diff | DEFER | - |
| Halt-and-Hedge | DEFER (needs pause protocol) | - |
| One-Click Apply to File | DEFER (runtime already applies via tools) | - |
| Pipeline Health Mini-map | DEFER | - |

---

## Phase 0: Hygiene

**Goal:** Clean codebase, faster builds, contributor onboarding.

### 0.1 Delete dead code
- Remove `src/commands/run.ts` and `src/commands/validate.ts` (legacy pre-DI implementations)
- Confirmed: `extension.ts` imports only from `commands/RunCommand` and `commands/ValidateCommand`

### 0.2 Add esbuild bundler
- Add `esbuild.mjs` build script producing single `dist/extension.js`
- Update `package.json`: `"main": "./dist/extension.js"`, `"vscode:prepublish"` script
- Required for marketplace publishing and reduces load time

### 0.3 Create `vscode-ail/CLAUDE.md`
- Document: DI wiring, event flow (AilEvent vs RunnerEvent), build/test commands, NDJSON contract

### 0.4 Clear textarea on send
- In `ChatViewProvider._html()`, add `textarea.value = ''` after `vscode.postMessage({ type: 'send', prompt })` in the `send()` function
- File: `vscode-ail/src/views/ChatViewProvider.ts:118`

### 0.5 Extract `resolvePipelinePath` utility
- Pipeline resolution logic is duplicated in `RunCommand.ts`, `ValidateCommand.ts`, and the inline `ail.materializePipeline` handler in `extension.ts` (lines 102-129)
- Extract to `src/utils/pipelinePath.ts`

---

## Phase 1: Daily Driver Foundation

**Goal:** Extension works reliably for the common case. Each run produces useful, persistent output.

### 1.1 Proper YAML parsing
- Replace regex step parser in `pipeline.ts` with the `yaml` npm package
- Current parser fails on multi-line prompts, quoted step IDs, comments-in-values
- File: `vscode-ail/src/pipeline.ts`

### 1.2 Enrich NDJSON event stream (Rust-side)
- Add `resolved_prompt: String` to `ExecutorEvent::StepStarted`
- Add `response: Option<String>` to `ExecutorEvent::StepCompleted`
- Files: `ail-core/src/executor.rs` (enum + emission sites), `ail/src/main.rs` (if envelope changes needed)
- Update spec: `spec/core/s23-structured-output.md`

### 1.3 Permission request display
- `ExecutionPanel` currently ignores `permission_requested` events
- Show inline banner with tool name + detail (display-only, no response mechanism yet)
- File: `vscode-ail/src/panels/ExecutionPanel.ts`

### 1.4 Latency tracking
- Capture `Date.now()` at `stepStarted`, compute delta at `stepCompleted`
- Display latency alongside tokens/cost in ExecutionPanel step headers
- File: `vscode-ail/src/panels/ExecutionPanel.ts`

### 1.5 Test infrastructure
- Add `@vscode/test-electron` with a runner in `src/test/suite/`
- Port existing 3 test files (`binary.test.ts`, `ndjson.test.ts`, `pipeline-tree.test.ts`)
- Add at least one integration test that activates the extension

---

## Phase 2: History and Stage

**Goal:** Persistent run history, the #6 "History Rail + Stage" UX.

### 2.1 History persistence via run log indexing
- The Rust runtime already writes NDJSON run logs to `~/.ail/projects/<sha>/runs/<run_id>.jsonl`
- Build `HistoryService` that indexes: run_id, timestamp, pipeline_source, outcome, total_cost, invocation_prompt
- Cache index in `workspaceState` with file-hash for incremental updates

### 2.2 History Rail (TreeView)
- New `HistoryTreeProvider` in the sidebar, below Steps view
- Each item: timestamp, pipeline name, outcome glyph, cost
- Clicking opens Stage panel populated from the run log

### 2.3 Stage panel (replaces ExecutionPanel)
- Two modes: **live** (during a run, same streaming behavior) and **review** (from historical run log)
- Step headers with telemetry chips: tokens in/out, cost, latency
- Auto-collapse finished steps during active run; expand final step's response on completion
- File: new `src/panels/StagePanel.ts` replacing `src/panels/ExecutionPanel.ts`

### 2.4 Inspected Payload toggle
- Per-step "View resolved prompt" toggle in the Stage panel
- Depends on Phase 1.2 (Rust-side `resolved_prompt` in `StepStarted`)

### 2.5 Active Editor Bridge
- Capture `vscode.window.activeTextEditor.selection` text before starting a run
- Pass as `AIL_SELECTION` env var to the child process
- Available in pipelines as `{{ env.AIL_SELECTION }}`
- File: `vscode-ail/src/commands/RunCommand.ts`, `vscode-ail/src/infrastructure/AilProcess.ts`

---

## Phase 3: Bidirectional Communication

**Goal:** HITL gates work, pause/resume works. Completes #4 Phase 1.

### 3.1 Rust-side stdin protocol
- In `run_once_json` (`ail/src/main.rs`), spawn a thread reading NDJSON from stdin
- Message types:
  ```json
  {"type": "hitl_response", "step_id": "...", "action": "approve|reject", "text": "..."}
  {"type": "permission_response", "request_id": "...", "allowed": true}
  {"type": "pause"}
  {"type": "resume"}
  ```
- Dispatch to existing `hitl_tx` channel and `ExecutionControl` atomics
- Update spec: `spec/core/s23-structured-output.md` (add stdin protocol section)

### 3.2 Extension stdin writer
- Add `writeStdin(message: object): void` to `IAilClient` interface
- `AilProcess` implements by writing NDJSON to `_activeProcess.stdin`
- File: `vscode-ail/src/application/IAilClient.ts`, `vscode-ail/src/infrastructure/AilProcess.ts`

### 3.3 HITL gate UI
- When `hitl_gate_reached` fires, show inline panel in Stage with:
  - Approve / Reject buttons
  - Text input for guidance (Path B injection)
- On submit: `client.writeStdin({ type: "hitl_response", ... })`
- File: `vscode-ail/src/panels/StagePanel.ts`

### 3.4 Permission request UI
- Similar to HITL: show tool name + detail, Approve/Deny buttons
- On submit: `client.writeStdin({ type: "permission_response", ... })`

### 3.5 Prompt rehydration ("The Fork")
- Clicking a historical run in the History Rail populates the chat textarea with the original prompt
- Requires: invocation prompt stored in history index (it is -- from run log's invocation TurnEntry)

---

## Phase 4: Polish and Concurrency

### 4.1 Multi-pipeline concurrent execution
- Refactor `RunnerService`: `_isRunning: boolean` -> `_activeRuns: Map<string, RunContext>`
- Each run gets its own `AilProcess` instance
- Stage panel shows selected run; History Rail shows all active runs

### 4.2 Advanced diagnostics
- Structured validation errors with line/column (Rust-side change to `ail validate`)
- Map to proper VS Code `Diagnostic` ranges instead of pinning to line 0

### 4.3 Evaluate deferred features
- Shadow Diff, Pipeline Health Mini-map -- evaluate based on actual usage patterns

---

## Key Files

| File | Role |
|---|---|
| `vscode-ail/src/extension.ts` | Entry point, DI wiring |
| `vscode-ail/src/application/RunnerService.ts` | Run orchestration, drives all UI updates |
| `vscode-ail/src/application/IAilClient.ts` | Interface for runtime interaction |
| `vscode-ail/src/infrastructure/AilProcess.ts` | Concrete client (child_process.spawn) |
| `vscode-ail/src/panels/ExecutionPanel.ts` | Current execution webview (replaced in Phase 2) |
| `vscode-ail/src/views/ChatViewProvider.ts` | Sidebar chat input |
| `vscode-ail/src/views/StepsTreeProvider.ts` | Step status tree |
| `vscode-ail/src/pipeline.ts` | YAML step parsing (needs proper parser) |
| `vscode-ail/src/types.ts` | NDJSON event type definitions |
| `ail-core/src/executor.rs` | ExecutorEvent enum, execute_with_control() |
| `ail/src/main.rs` | run_once_json() -- stdin reader needed here |
| `spec/core/s23-structured-output.md` | NDJSON event spec |

## Verification

- **Phase 0:** `npm run compile` succeeds, `npm run build` (esbuild) produces `dist/extension.js`, extension activates in dev host
- **Phase 1:** Run a pipeline in the extension, verify latency and permission events appear; `npm test` passes with new test infrastructure
- **Phase 2:** Run a pipeline, close and reopen VS Code, verify run appears in History Rail with correct data; click to review
- **Phase 3:** Create a pipeline with `pause_for_human` step, run in extension, verify HITL gate UI appears and response unblocks the pipeline
- **Phase 4:** Start two pipelines simultaneously, verify both run and display independently
