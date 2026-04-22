# Design: Install Wizard (#166) + Toggle Side Panel (#167)

**Date:** 2026-04-21
**Issues:** [#166](https://github.com/AlexChesser/ail/issues/166), [#167](https://github.com/AlexChesser/ail/issues/167)
**Approach:** Sequential branches, one PR per issue. 166 first; 167 branches from main after 166 merges.

---

## Issue 166 — Install Wizard for Empty Workspaces

### Architecture

`install-wizard.ts` exports a single async function `checkAndOfferInstall(context, chatProvider)`. Called from two sites in `extension.ts`:

1. End of `activate()`
2. Inside `onDidChangeWorkspaceFolders`

Early-exit conditions (checked in order):
1. `workspaceState.get('ail-chat.installPromptDismissed') === true` → return
2. Any pipeline file found in workspace root (`.ail.yaml`, `.ail.yml`, `.ail/*.yaml`, `.ail/*.yml`) → return

`ChatViewProvider` gains one new public method: `reloadPipeline()`. Re-runs the existing `_resolvedPipeline()` logic, persists the result to `workspaceState`, and calls `_sendPipelineChanged()`. The wizard calls this after copying a template so the chat view picks up the new pipeline immediately.

### Templates & Build Integration

All three template sets live under `demo/` as first-class demo directories:

- `demo/starter/` — new, checked into repo. Contains `default.yaml` (single `invocation:` step, heavily commented) and `README.md`. Uses `{{ step.invocation.prompt }}` (canonical form, not the deprecated alias).
- `demo/oh-my-ail/` — existing
- `demo/superpowers/` — existing

`scripts/sync-templates.js` (new Node script, no external deps) copies all three from `demo/` into `vscode-ail-chat/templates/` at build time. `vscode-ail-chat/templates/` is entirely `.gitignore`'d.

`package.json` script changes:
```json
"build": "node scripts/sync-templates.js && node esbuild.js",
"vscode:prepublish": "node scripts/sync-templates.js && node esbuild.js --production"
```

`esbuild.js` gets a `copyTemplates()` step copying `templates/**` into `dist/templates/`. The wizard reads from `context.extensionPath + '/dist/templates/<name>/'` at runtime and copies the chosen tree into `<workspaceRoot>/.ail/`.

### QuickPick UX & Dismiss Semantics

Four QuickPick items (three templates + dismiss):
```
① Starter — Invocation-only pipeline (recommended)
② Oh My AIL — Intent-routed multi-agent orchestration
③ Superpowers — Curated high-leverage workflows
④ Dismiss
```

After a template is picked:
1. Copy chosen template tree from `dist/templates/<name>/` into `<workspaceRoot>/.ail/`
2. Call `chatProvider.reloadPipeline()`
3. Open template `README.md` in markdown preview

Dismiss semantics:
- "Dismiss" item selected → `workspaceState.update('ail-chat.installPromptDismissed', true)` → no future prompts for this workspace
- Escape/cancel (QuickPick returns `undefined`) → flag **not** set → re-prompts on next activation

### Files

**New:**
- `demo/starter/default.yaml`
- `demo/starter/README.md`
- `vscode-ail-chat/src/install-wizard.ts`
- `vscode-ail-chat/scripts/sync-templates.js`
- `vscode-ail-chat/test/install-wizard.test.ts`

**Modified:**
- `vscode-ail-chat/src/extension.ts` — call wizard from `activate()` + `onDidChangeWorkspaceFolders`
- `vscode-ail-chat/src/chat-view-provider.ts` — add public `reloadPipeline()`
- `vscode-ail-chat/package.json` — script hooks for sync-templates
- `vscode-ail-chat/esbuild.js` — template copy step
- `vscode-ail-chat/.gitignore` — ignore `templates/`

### Test Coverage

- No-pipeline detection across all four path patterns — wizard fires when none exist
- Wizard is a no-op when any pipeline already exists
- Dismiss flag set only on "Dismiss" item, not on Escape/cancel
- Template files copied to correct `.ail/` directory
- `reloadPipeline()` called after successful install
- Re-prompt suppressed after dismiss flag is set

All tests use the existing `vscode-stub.js` mock.

---

## Issue 167 — Toggle Side Panel: Run History + Pipeline Steps

*Branches from main after #166 merges.*

### Architecture

Two new tree data providers registered in `extension.ts`:

**`RunHistoryProvider`** (`src/history-tree-provider.ts`):
- Calls `ail logs --format json --limit 100` via `resolveBinary` to populate tree
- No `workspaceState` persistence — CLI owns the storage
- `refresh()` re-runs the CLI query; called by `ChatViewProvider` on run completion
- Tree items: first 60 chars of prompt + relative timestamp
- `ail-chat.openRunLog` command: calls `ail log <runId>`, opens result as VS Code text document (`language: 'markdown'`)
- Binary call is injected as a testable function for unit tests

**`PipelineStepsProvider`** (`src/steps-tree-provider.ts`):
- Parses active pipeline YAML via the `yaml` package (already a dep)
- Top-level steps only; sub-pipeline references render as single node with filename label
- Error node (no expand) when pipeline missing, YAML malformed, or passthrough mode
- Each item: step `id` + type icon (prompt/context/skill/sub-pipeline)
- `ail-chat.openStep` command: opens YAML file, reveals `id:` line in viewport
- `refresh(pipelinePath)` called by `ChatViewProvider` on pipeline change

**Toggle mechanism:**
```typescript
let panelVisible = false;
commands.registerCommand('ail-chat.toggleInfoPanel', () => {
  panelVisible = !panelVisible;
  void commands.executeCommand('setContext', 'ail-chat.panelVisible', panelVisible);
});
```
Views use `"when": "ail-chat.panelVisible"` in `package.json`. Toggle button in chat view title bar via `menus["view/title"]` with `$(layout-sidebar-right)` icon.

**`ChatViewProvider` changes:**
- Calls `historyProvider.refresh()` on run completion (replaces the `addRun()` pattern)
- Calls `stepsProvider.refresh(pipelinePath)` on pipeline change

### Files

**New:**
- `vscode-ail-chat/src/history-tree-provider.ts`
- `vscode-ail-chat/src/steps-tree-provider.ts`
- `vscode-ail-chat/test/history-tree-provider.test.ts`
- `vscode-ail-chat/test/steps-tree-provider.test.ts`

**Modified:**
- `vscode-ail-chat/src/extension.ts` — register both providers + toggle command
- `vscode-ail-chat/src/chat-view-provider.ts` — call `historyProvider.refresh()` on run completion; `stepsProvider.refresh()` on pipeline change
- `vscode-ail-chat/package.json` — new views (`when: ail-chat.panelVisible`), toggle command + icon, `openRunLog` and `openStep` commands

### Test Coverage

`history-tree-provider.test.ts`:
- `refresh()` calls `ail logs --format json` and populates tree items
- Results capped at 100 via `--limit 100` passed to the CLI
- `getChildren()` returns entries reverse-chronological
- `openRunLog` calls `ail log <runId>` and opens result as markdown document

`steps-tree-provider.test.ts`:
- Valid multi-step pipeline → one `StepItem` per top-level step
- Sub-pipeline reference → single node with filename label
- Malformed YAML → error node, no throw
- Null/passthrough pipeline → empty tree

All tests use the existing `vscode-stub.js` mock.

### Related

- Log search/fuzzy matching exploration tracked in [#171](https://github.com/AlexChesser/ail/issues/171)
