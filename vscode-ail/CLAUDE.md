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
- **Shared type changes require updating all consumers:** `TurnEntry` and `RunRecord` (in `parseRunFile.ts`) are imported by both production code and test helpers. Adding or removing fields must be reflected in every inline object literal that constructs those types — check `src/test/suite/*.test.ts` in addition to production callers. Run `npm run compile` to catch mismatches immediately.

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

---

# Log Display Architecture (Issue #34)

The extension displays run logs via a single, subprocess-based interface: `ail log` binary. All database access is tunneled through that binary — there is no direct SQLite access from TypeScript, no `better-sqlite3`, no embedded database client.

## Design Principle: Binary as Single Data Interface

The SQLite schema is an internal implementation detail of the `ail` binary. The extension is a thin display consumer that spawns `ail log [run_id]` subprocesses and renders their formatted stdout. This isolation ensures:

1. **Schema evolution is decoupled**: database changes don't break the extension.
2. **Consistent semantics**: all log consumers (CLI, extension, future TUIs) use the same binary interface.
3. **Resource efficiency**: no duplicate database connections; the Rust process handles pooling and locking.
4. **Observability**: the binary logs its own polling behavior; the extension stays simple.

## Data Flow

```
User clicks "Open Log" or views history
         ↓
AilLogProvider.provideTextDocumentContent(uri)
         ↓
AilProcess.log(runId?) [spawns `ail log [run_id] --format markdown`]
         ↓
Binary reads SQLite, emits ail-log/1 format to stdout
         ↓
Extension captures stdout → Virtual document
         ↓
TextMate grammar (ail-log.tmLanguage.json) adds syntax highlighting
         ↓
Markdown preview renders collapsible directives (:::thinking, :::tool-call, etc.)
         ↓
Auto-folding (FoldingProvider) collapses blocks per ail.autoFoldThinking config
```

## Two Rendering Modes

### Raw Mode (Primary)

- **Input:** ail-log/1 format stdout from `ail log --format markdown`
- **Processing:** URI scheme `ail-log` provides virtual document; TextMate grammar highlights blocks
- **Interaction:** Click to fold/expand `:::` directives
- **Spec:** `spec/runner/r04-ail-log-format.md` (format contract)

### Preview Mode (Markdown)

- **Input:** Same ail-log/1 content
- **Processing:** VS Code's built-in Markdown preview renderer transforms the `:::directive` syntax into `<details><summary>` HTML
- **Interaction:** Click to fold/expand; standard Markdown preview features
- **Future:** custom CSS for ail-specific styling

## Commands

Register these in `package.json` → `contributes.commands`:

- **`ail.openLog`** — Opens the log viewer for a given run
  - **Arguments:** `run_id?: string` (optional; if omitted, resolves to latest run via `ail log` with no run_id)
  - **Behavior:** Spawns `ail log [run_id] --format markdown`, opens virtual document with URI scheme `ail-log://{run_id}`, shows Markdown preview
  - **Keybinding:** (none by default; callable from command palette or other commands)

- **`ail.followTail`** — Toggle live-tail mode for an in-progress run
  - **Arguments:** `run_id: string`
  - **Behavior:** If run is in-progress, spawns `ail log --follow <run_id>` subprocess; streams appended lines to virtual document as they arrive. Auto-scrolls to bottom if enabled (default: on for in-progress runs, off for completed runs).
  - **Exit:** Binary exits code 0 when run completes; extension detects process exit and stops streaming.

- **`ail.toggleView`** — Switch between raw and preview modes
  - **Behavior:** Opens the same virtual document in different editors (raw editor + preview panel)
  - **State:** Persisted in workspaceState

## Configuration

Register these in `package.json` → `contributes.configuration`:

- **`ail.autoFoldThinking`** — Boolean, default `true`
  - **Behavior:** When true, FoldingProvider auto-collapses all `:::thinking` blocks on document open
  - **Scope:** User/Workspace
  - **Example:**
    ```json
    {
      "ail.autoFoldThinking": true
    }
    ```

## Live Tail via `ail log --follow`

The binary handles all polling — the extension is a passive consumer:

**Rust side (binary responsibility):**
- `ail log --follow <run_id>` reads the run's status from SQLite
- Emits full run (ail-log/1 format) on first line
- Polls `SELECT * FROM steps WHERE run_id = ? AND recorded_at > ? ORDER BY recorded_at` every 500ms for new steps
- Appends formatted new lines to stdout as they arrive
- Exits code 0 when `step_completed` is encountered on the final step
- Exits code 1 on database error or invalid run_id
- Retries on SQLITE_BUSY: up to 3 retries with 50ms backoff before skipping the tick

**TypeScript side (extension responsibility):**
- `AilLogStream.ts` spawns the subprocess
- Attaches to `proc.stdout` and fires `_onDidChange` emitter on each line
- Debounces to max 1 emission per 300ms
- `AilLogProvider` re-reads the virtual document content on each change
- Calls `editor.revealRange()` to auto-scroll (respects `ail.followTail` setting)
- Cleans up subprocess on error or when process exits

## Non-Goals for v1

These will be implemented in v2 or later:

- **CodeLens:** no run-re-run or edit-and-rerun buttons on log lines
- **Per-tool-call blocks:** `:::tool-call` and `:::tool-result` directives require a future `run_events` schema extension; v1 emits thinking + response only
- **HITL (Human-in-the-Loop):** no approve/reject buttons for permission gates
- **Log search:** search happens via `ail logs --query` (different command); not integrated into the log viewer yet
- **Diff mode:** no side-by-side comparison of logs from consecutive runs

## Relationship to Existing Extension Architecture

The log display system is **orthogonal** to the pipeline execution system:

- **Pipeline execution** (`AilProcess.invoke()`, `RunnerService`, `UnifiedPanel`): Uses `--output-format json` with NDJSON streaming. Real-time event handling for UI updates during a run. Schema: `spec/core/s23-structured-output.md`.

- **Log display** (`AilProcess.log()`, `AilLogProvider`, `AilLogStream`): Uses `--format markdown` or `--format json` (static format or streaming). Displays completed or in-progress run history. Schema: `spec/runner/r04-ail-log-format.md`.

These are separate concerns with separate data flows:
- Execution flow: `invoke()` → binary → NDJSON events → RunnerService → UI updates
- Log flow: `log()` → binary → ail-log/1 stdout → virtual document → preview

`HistoryService.getRunDetail()` currently uses `ail logs` (plural, table format). The `log` (singular) subcommand is intended as a future replacement for detailed run inspection; both can coexist during the transition.

## File Organization (Implementation Plan)

**Phase 1 (C1–C3):** Infrastructure + rendering

- `src/infrastructure/AilProcess.ts` — add `log(runId?: string): Promise<string>` method
- `src/infrastructure/AilLogProvider.ts` — new `TextDocumentContentProvider` for `ail-log` URI scheme
- `src/infrastructure/AilLogStream.ts` — new subprocess wrapper for `--follow` mode
- `src/infrastructure/FoldingProvider.ts` — auto-folding controller respecting `ail.autoFoldThinking`
- `src/commands/OpenLogCommand.ts` — command handler for `ail.openLog`
- `src/types.ts` — add TypeScript types for ail-log directive taxonomy (if needed for grammar or formatter)
- `syntaxes/ail-log.tmLanguage.json` — TextMate grammar for syntax highlighting and folding
- `test-fixtures/sample.ail-log` — manual verification fixture

**Phase 2 (D1–D2):** Consistency + live tail

- Cross-check formatter output against golden fixture
- Wire `AilLogStream` into `AilLogProvider` for live runs
- Auto-scroll and debouncing

## Testing

- `src/test/AilProcess.test.ts` — mock subprocess; assert `log()` invocation and argument passing
- `src/test/AilLogProvider.test.ts` — mock `AilProcess.log()`, assert URI parsing and virtual document content
- Manual smoke test: open a historical run and verify collapsible blocks render correctly
- Grammar test: validate `ail-log.tmLanguage.json` against `test-fixtures/sample.ail-log`
