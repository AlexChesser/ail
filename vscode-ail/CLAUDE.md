# vscode-ail — VS Code Extension

Extension that brings `ail` pipeline orchestration into VS Code: run pipelines from a chat input, monitor live execution, browse pipeline files, and inspect step-level telemetry.

## Workspace Layout

```
vscode-ail/
  esbuild.js              # bundler — produces dist/extension.js
  package.json            # manifest: activation events, commands, views, config
  tsconfig.json           # CommonJS, ES2020, strict

  src/
    extension.ts          # activate() — binary resolution, DI wiring, command registration
    types.ts              # AilEvent union — wire format types matching Rust NDJSON output
    binary.ts             # Binary resolution: config > bundled (dist/) > PATH
    ndjson.ts             # Line-buffered NDJSON stream parser (attaches to Node Readable)
    pipeline.ts           # discoverPipelines() + parseStepsFromYaml()
    state.ts              # Active pipeline path persisted in workspaceState

    application/
      events.ts           # RunnerEvent — simplified application-layer event union
      EventBus.ts         # Typed pub/sub (Map<type, Set<handler>>)
      IAilClient.ts       # Interface: invoke(), validate(), cancel(), onEvent(), onRawEvent()
      RunnerService.ts    # Orchestrates runs; drives UI updates (status bar, views, panel)
      ServiceContext.ts   # DI container: ExtensionContext + OutputChannel + IAilClient

    infrastructure/
      AilProcess.ts       # IAilClient impl: spawn(), parseNdjsonStream, _mapAilEvent()

    commands/
      RunCommand.ts       # Thin: resolvePipelinePath(), prompt user, call RunnerService
      ValidateCommand.ts  # Thin: call IAilClient.validate(), publish DiagnosticCollection

    language/
      completions.ts      # Template variable completions inside {{ }}

    panels/
      UnifiedPanel.ts     # Singleton 3-column WebviewPanel: [Runs | Steps | Detail]
      unifiedPanelHtml.ts # HTML/CSS/JS for the UnifiedPanel webview (extracted for clarity)
      MessageBuffer.ts    # Queue postMessage calls until webview signals ready

    utils/
      pipelinePath.ts     # Shared resolvePipelinePath() — 4-priority resolution

    views/
      ChatViewProvider.ts    # WebviewView (sidebar): textarea + Run/Stop
      PipelineTreeProvider.ts # TreeDataProvider: pipeline file browser
      StepsTreeProvider.ts    # TreeDataProvider: step status glyphs
      HistoryTreeProvider.ts  # TreeDataProvider: past run history (lightweight sidebar)

  test/
    binary.test.ts        # Pure Node: platformTriple(), meetsMinVersion()
    ndjson.test.ts        # NDJSON parser edge cases
    pipeline-tree.test.ts # YAML step parser (writes temp files to disk)
```

## Common Commands

```bash
# Requires Node 24 (use nvm: nvm use 24)

# Build extension bundle (dist/extension.js)
npm run build

# Type-check and compile test files (out/)
npm run compile

# Run all tests
npm test

# Launch Extension Development Host (from .vscode/launch.json or VS Code F5)
```

## Architecture

### DI Graph (wired in extension.ts activate())

```
AilProcess(binaryPath, cwd)   ← concrete IAilClient
  └─ ServiceContext(ctx, outputChannel, client)
       └─ EventBus
            └─ RunnerService(services, bus)
                 ├─ RegisteredViews: statusBarItem, ChatViewProvider, StepsTreeProvider
                 └─ UnifiedPanel (singleton, reused across runs — not per-run)
```

**UnifiedPanel singleton lifecycle:**
- Created on first use (run start or history click); stored in `UnifiedPanel._instance`.
- Reused for all subsequent runs and history reviews — never re-created mid-session.
- `retainContextWhenHidden: true` preserves webview DOM state.
- Disposed only when the user closes the tab; `_instance` is cleared in `onDidDispose`.
- Per-run stdin callbacks stored in `_writeStdinMap: Map<runId, callback>`.
- `onRunComplete(runId)` called in RunnerService `finally` block to release per-run resources.

### Event Two-Tier System

**Tier 1 — `AilEvent` (src/types.ts):** Full-fidelity wire format. Mirrors the Rust NDJSON output exactly. Includes thinking, tool_use, tool_result, cost_update, permission_requested events. Used by `UnifiedPanel` (raw fidelity needed for UI rendering).

**Tier 2 — `RunnerEvent` (src/application/events.ts):** Simplified union of 8 types used by the application layer. `AilProcess._mapAilEvent()` projects Tier 1 → Tier 2. Strips events that don't need application-layer routing.

**Data flow during a run:**
1. `RunnerService.startRun()` registers handlers on `IAilClient`
2. `AilProcess.invoke()` spawns `ail --once --output-format json --headless`
3. `parseNdjsonStream()` parses stdout NDJSON line by line
4. Each `AilEvent` → `_emitRaw()` → raw handlers (UnifiedPanel)
5. Each `AilEvent` → `_mapAilEvent()` → `_emit()` → event handlers (RunnerService)
6. RunnerService drives: EventBus, StepsTreeProvider, OutputChannel

### NDJSON Event Contract (from ail runtime)

Full protocol in `spec/core/s23-structured-output.md`. Key types:

```
run_started → step_started → runner_event* → step_completed → ... → pipeline_completed
```

`runner_event` wraps: `stream_delta`, `thinking`, `tool_use`, `tool_result`, `cost_update`, `permission_requested`, `completed`, `error`

**Gaps (data in TurnEntry but not yet in events):**
- `step_started` does not include the resolved prompt text (planned: Phase 1)
- `step_completed` does not include the response text (planned: Phase 1)
- `permission_requested` has no response channel (planned: Phase 3)
- `hitl_gate_reached` has no response channel (planned: Phase 3)

### Pipeline Path Resolution Priority

`src/utils/pipelinePath.ts:resolvePipelinePath()`:
1. Active pipeline set via sidebar selector (`state.ts`)
2. Active text editor (if `.ail.yaml`/`.ail.yml`)
3. `ail.defaultPipeline` workspace setting
4. `.ail.yaml` at workspace root

## Code Conventions

- No direct `spawn()` calls outside `AilProcess.ts`
- No VS Code API calls outside `src/` (tests are pure Node)
- `AilEvent` types live in `types.ts` — never define wire types elsewhere
- Use `RunnerEvent` in application layer; only `UnifiedPanel` needs `AilEvent`
- `resolvePipelinePath()` is the single source of truth for pipeline discovery
- `UnifiedPanel._createWebviewPanel` is the injectable factory for VS Code panel creation — override in tests instead of mocking the entire vscode module

## Known Issues / Planned Work

Current known gaps:
- **`parseStepsFromYaml()`** in `pipeline.ts` uses regex — should use `yaml` npm package
- Column widths in UnifiedPanel are fixed; drag-to-resize handles are not yet implemented

## Building the VSIX

```bash
nvm use 24
npm install -g @vscode/vsce
npm run build   # produce dist/extension.js
vsce package    # produce vscode-ail-X.Y.Z.vsix
```

The `vscode:prepublish` script runs `node esbuild.js --production` (minified, no sourcemaps).
