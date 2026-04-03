# VS Code Extension: Issues #4 + #6 Merged Implementation Plan

## Context

The `vscode-ail` extension is a working proof of concept with solid DI architecture, NDJSON event streaming, and basic UI panels. Issues #4 and #6 describe the path from PoC to daily driver. #4 focuses on infrastructure (HITL gates, interrupt/resume, bidirectional communication). #6 focuses on UX (turn-based history, step decomposition, developer utility features). This plan merges both, triaging what's done, what's blocked, and what to build.

## Issue #4 Triage

| Item | Status | Notes |
|---|---|---|
| Headless execution | DONE | `AilProcess.invoke()` passes `--headless` |
| Execution Monitor | DONE | `UnifiedPanel` webview (replaced ExecutionPanel) with steps, streaming, cost |
| HITL gate approval/rejection | DONE | Full bidirectional chain; approve/reject UI in UnifiedPanel webview |
| Path A interrupt (resume) | DONE | stdin `pause`/`resume` messages wired to `ExecutionControl` atomics |
| Path B guidance injection | DONE | `hitl_response` carries optional `text` field to executor |
| Path C hard kill | DONE | `AilProcess.cancel()` sends SIGTERM/SIGKILL |
| YAML highlighting | DONE | `ail-pipeline` language contribution |
| Pipeline Explorer | DONE | `PipelineTreeProvider` |
| Command palette | DONE | 13 commands |
| Pipeline History | DONE | `HistoryService` + `HistoryTreeProvider` |
| Multi-pipeline concurrency | DONE | `_activeRuns: Map<string, RunContext>` in `RunnerService` |

## Issue #6 Feature Triage

| Feature | Verdict | Phase |
|---|---|---|
| History Rail + Stage layout | DONE | 2 |
| Chat clears on send | DONE | 0 |
| Step decomposition (collapsible Thinking/Response) | DONE | - |
| Auto-collapse finished steps | DONE | 2 (auto-selects next step) |
| Prompt rehydration ("The Fork") | DONE | 3 |
| Inspected Payload toggle | DONE | 2 |
| Resource telemetry chips | DONE | 2 |
| Active Editor Bridge (`{{ selection }}`) | DONE | 2 |
| Shadow Diff | DEFER | - |
| Halt-and-Hedge | DEFER | - |
| One-Click Apply to File | DEFER (runtime already applies via tools) | - |
| Pipeline Health Mini-map | DEFER | - |

---

## Phase 0: Hygiene ✓ DONE

**Goal:** Clean codebase, faster builds, contributor onboarding.

### 0.1 Delete dead code ✓ DONE
### 0.2 Add esbuild bundler ✓ DONE (package.json `"main"` fixed to `./dist/extension.js` 2026-04-03)
### 0.3 Create `vscode-ail/CLAUDE.md` ✓ DONE
### 0.4 Clear textarea on send ✓ DONE
### 0.5 Extract `resolvePipelinePath` utility ✓ DONE

---

## Phase 1: Daily Driver Foundation ✓ DONE

**Goal:** Extension works reliably for the common case. Each run produces useful, persistent output.

### 1.1 Proper YAML parsing ✓ DONE (`parseYaml.ts` uses `yaml` npm package)
### 1.2 Enrich NDJSON event stream ✓ DONE (`resolved_prompt` + `response` on both Rust and TS sides)
### 1.3 Permission request display ✓ DONE (fully interactive Allow/Deny in UnifiedPanel)
### 1.4 Latency tracking ✓ DONE (`_stepStartTimes` map + rendered chips)
### 1.5 Test infrastructure ✓ DONE (13 test files + `@vscode/test-electron` runner)

---

## Phase 2: History and Stage ✓ DONE

**Goal:** Persistent run history, the #6 "History Rail + Stage" UX.

### 2.1 History persistence ✓ DONE (`HistoryService` with content-hash caching, all 6 fields)
### 2.2 History Rail ✓ DONE (`HistoryTreeProvider` with outcome glyph, cost, prompt preview)
### 2.3 Stage panel ✓ DONE (`UnifiedPanel` — live + review modes, telemetry chips)
### 2.4 Inspected Payload toggle ✓ DONE (collapsible "Inspected Payload" block per step)
### 2.5 Active Editor Bridge ✓ DONE (`AIL_SELECTION` env var from `activeTextEditor.selection`)

---

## Phase 3: Bidirectional Communication ✓ DONE

**Goal:** HITL gates work, pause/resume works. Completes #4 Phase 1.

### 3.1 Rust-side stdin protocol ✓ DONE (stdin reader in `main.rs`: hitl_response, permission_response, pause, resume, kill)
### 3.2 Extension stdin writer ✓ DONE (`writeStdin()` on `IAilClient` + `AilProcess`, `_writeStdinMap` per-run routing)
### 3.3 HITL gate UI ✓ DONE (approve/reject buttons + guidance textarea in UnifiedPanel webview)
### 3.4 Permission request UI ✓ DONE (Allow/Deny buttons in Thinking block, forwarded via `permission_response`)
### 3.5 Prompt rehydration ✓ DONE (`ail.forkHistoryRun` command wired to history tree context menu)

---

## Phase 4: Polish and Concurrency ✓ DONE

### 4.1 Multi-pipeline concurrent execution ✓ DONE (`_activeRuns: Map<string, RunContext>` in RunnerService)
### 4.2 Advanced diagnostics ✓ DONE (`ValidationError` with line/column, mapped to VS Code Diagnostic ranges)
### 4.3 Evaluate deferred features — Shadow Diff, Pipeline Health Mini-map still deferred

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
