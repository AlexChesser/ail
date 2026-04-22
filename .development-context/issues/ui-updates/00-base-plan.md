# Plan: VSCode Extension UX Improvements



## Context



The `vscode-ail-chat` extension (at `vscode-ail-chat/`) is the active UI for AIL. A Windows user ran a passthrough pipeline and encountered multiple problems: no guidance when no pipeline exists, no diagnostic output, a hang that caused "A pipeline is already running" on the next prompt, and no way to inspect history or steps. This plan addresses all 8 reported items.



---



## Scope: `vscode-ail-chat/` only



The legacy extension (`vscode-ail-legacy/`) has many of these features (output channel, history tree, steps tree, log viewer) but is not the active extension. We are not merging the two — we are bringing the chat extension up to parity on the critical items.



---



## Critical Files



**Modified:**

| File | Role |

|------|------|

| `vscode-ail-chat/src/extension.ts` | Entry point — activation, wizard trigger, output channel + tree-view registration |

| `vscode-ail-chat/src/chat-view-provider.ts` | Adds `reloadPipeline()`, exposes `currentPipeline`, notifies tree providers on events |

| `vscode-ail-chat/src/ail-process-manager.ts` | Accepts `ProcessKiller` and `AilOutputChannel`; logs spawn/events/stderr/exit |

| `vscode-ail-chat/package.json` | New views, commands, `menus.view/title` toggle button, `templates/` in files |

| `vscode-ail-chat/esbuild.js` | Copies `templates/**` into `dist/templates/` |



**New:**

| File | Role |

|------|------|

| `vscode-ail-chat/src/install-wizard.ts` | No-pipeline detection + QuickPick + template copy |

| `vscode-ail-chat/src/output-channel.ts` | `AilOutputChannel` wrapper for diagnostics |

| `vscode-ail-chat/src/process/process-killer.ts` | `ProcessKiller` interface |

| `vscode-ail-chat/src/process/posix-process-killer.ts` | SIGTERM → SIGKILL implementation |

| `vscode-ail-chat/src/process/windows-process-killer.ts` | `taskkill /F /T /PID` implementation |

| `vscode-ail-chat/src/process/process-killer-factory.ts` | Platform selector |

| `vscode-ail-chat/src/history-tree-provider.ts` | `RunHistoryProvider` TreeDataProvider |

| `vscode-ail-chat/src/steps-tree-provider.ts` | `PipelineStepsProvider` TreeDataProvider |

| `vscode-ail-chat/templates/starter/default.yaml` | First-time-user invocation-only pipeline |

| `vscode-ail-chat/templates/starter/README.md` | Onboarding README opened on install |

| `vscode-ail-chat/templates/oh-my-ail/**` | Verbatim copy of `demo/oh-my-ail/` (synced at build) |

| `vscode-ail-chat/templates/superpowers/**` | Verbatim copy of `demo/superpowers/` (synced at build) |

| `vscode-ail-chat/scripts/sync-templates.js` | Copies demo trees into `templates/` before build |



---



## Item 7 (Windows hang) — Root Cause



`AilProcessManager.cancel()` calls `proc.kill('SIGTERM')`. On Windows, `SIGTERM` is not a real signal — Node.js translates it to a `TerminateProcess()` call on the immediate process, but if `ail` spawns child processes (Claude CLI), those children stay alive and hold the stdio pipes open. The parent `ail-chat` process manager never sees the `close` event, so `_activeProcess` is never cleared, and the next prompt hits the "A pipeline is already running" guard.



---



## Implementation Plan



### 1. Install Wizard — Items 1–5



**New file:** `vscode-ail-chat/src/install-wizard.ts`



Export `checkAndOfferInstall(context: ExtensionContext, chatProvider: ChatViewProvider): Promise<void>`



Logic:

1. Scan workspace root for `.ail.yaml`, `.ail.yml`, `.ail/*.yaml`, `.ail/*.yml`

2. If any found → return immediately (no-op)

3. Show `vscode.window.showInformationMessage('No AIL pipeline found in this workspace.', 'Configure AIL', 'Dismiss')`

4. On "Configure AIL": show `vscode.window.showQuickPick` with 3 items:



```

① Starter — Invocation-only pipeline (recommended for first-time users)

  A single explicit invocation step. Heavily commented YAML so you can see exactly

  what AIL is doing on every prompt. Defaults to the Claude runner.



② Oh My AIL — Intent-routed multi-agent orchestration

  The full Sisyphus classifier (TRIVIAL/EXPLICIT/EXPLORATORY/AMBIGUOUS) plus all

  agent and workflow files copied from the AIL demos.



③ Superpowers — Curated high-leverage workflows

  The full superpowers collection copied from the AIL demos: TDD, code-review,

  planning, brainstorming, parallel debug, plan execution, and more.

```



5. Create `.ail/` directory at workspace root (if it doesn't exist)

6. Copy the chosen template into `.ail/`:

   - **Starter**: copy `templates/starter/default.yaml` (heavily-commented invocation-only pipeline) and `templates/starter/README.md` into `.ail/`

   - **Oh My AIL**: recursively copy `templates/oh-my-ail/` into `.ail/oh-my-ail/`. This is the **full demo tree** (`.ohmy.ail.yaml`, `agents/*.ail.yaml`, `workflows/*.ail.yaml`, `prompts/*.md`, `README.md`) sourced verbatim from `demo/oh-my-ail/`. The default pipeline becomes `.ail/oh-my-ail/.ohmy.ail.yaml`.

   - **Superpowers**: recursively copy `templates/superpowers/` into `.ail/superpowers/`. This is the **full demo tree** (all `*.ail.yaml` files plus `prompts/`, `README.md`) sourced verbatim from `demo/superpowers/`. Since superpowers has no single entry point, the wizard offers a follow-up QuickPick listing the available `*.ail.yaml` files (`tdd-enriched`, `code-review`, `brainstorming`, etc.) and sets the chosen file as the default. The README explains how to switch between them later.

7. Set the chat extension's `defaultPipeline` setting (workspace scope) to the entry-point YAML so the chat picks it up immediately

8. Open the bundled README in markdown preview:

   ```typescript

   await vscode.commands.executeCommand(

     'markdown.showPreview',

     vscode.Uri.file(path.join(workspaceRoot, '.ail', '<chosen>', 'README.md'))

   );

   ```

9. Notify `ChatViewProvider` to reload pipeline (call a new `reloadPipeline()` method)



**Trigger in `extension.ts`:**

- Call `checkAndOfferInstall()` at end of `activate()` if a workspace folder exists

- Also on `vscode.workspace.onDidChangeWorkspaceFolders`

- Suppress repeat notifications via a `workspaceState` flag (`ail-chat.installPromptDismissed`) when the user picks "Dismiss". **Do not set this flag** if the user opens the wizard (picks "Configure AIL") but then cancels out of the QuickPick with Escape — that's not a dismissal, just an interruption; show the wizard again on the next activation.



**Pipeline templates** (new `vscode-ail-chat/templates/` directory):



`templates/starter/default.yaml` — explicit invocation step (matches the demo passthrough pattern in `demo/.ail.invocation.yaml`):

```yaml

# Welcome to AIL!

# This file is your pipeline. AIL runs these steps for every prompt you send.

# Edit freely — your changes take effect on the next prompt.

version: "0.0.1"



# 'defaults' applies to every step unless overridden.

defaults:

  # Runner choices:

  #   claude — uses the Claude Code CLI (must be installed: `npm i -g @anthropic-ai/claude-code`)

  #   codex  — uses the OpenAI Codex CLI plugin runner (if installed)

  #   http   — direct OpenAI-compatible HTTP API (e.g. Ollama). NOTE: cannot execute

  #            tool calls yet — text generation only.

  runner: claude



  # Default provider — points at a local Ollama server.

  # base_url and auth_token are exported as ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN

  # in the runner subprocess, so this works with both the 'claude' runner and the

  # 'http' runner. Edit 'model' to your model of choice, or swap base_url/auth_token

  # for another OpenAI-compatible endpoint (or remove the provider block entirely to

  # use the Claude CLI's default auth).

  provider:

    base_url: http://localhost:11434 # change this to your provider URL

    model: gemma4:e4b # change this to your model of choice

    auth_token: ollama



pipeline:

  # The 'invocation' step is the entry point. Its prompt is what your text becomes.

  - id: invocation

    prompt: "{{ step.invocation.prompt }}"

    tools:

      allow:

        - Read

        - Write

        - Edit

        - Bash

```



`templates/starter/README.md` — the in-IDE README that pops on install:

- Welcome blurb (what AIL is, what just got created)

- "Try it now": open the AIL Chat sidebar and type a prompt

- Where to find your pipeline (`.ail/default.yaml`) and how to add steps

- Runner choices explained (claude/codex/http) with install pointers

- Template variable cheatsheet

- Where to go next: link to `spec/README.md` and the demo workflows



`templates/oh-my-ail/` — copy of `demo/oh-my-ail/` (full tree, kept in sync via build step that runs before VSIX packaging — script: `node scripts/sync-templates.js`)



`templates/superpowers/` — copy of `demo/superpowers/` (full tree, same sync mechanism)



**`scripts/sync-templates.js` (new):** Copies `demo/oh-my-ail/` → `vscode-ail-chat/templates/oh-my-ail/` and `demo/superpowers/` → `vscode-ail-chat/templates/superpowers/`. Run it in **both** `npm run build` and as a `prepackage` step in `vscode:prepublish` — packaging bypasses `npm run build`, so without both hooks, stale templates can ship in a VSIX. Add `templates/oh-my-ail/` and `templates/superpowers/` to `.gitignore` so they are always regenerated rather than tracked; this also ensures CI fails fast if the sync step is skipped. CI must run `node scripts/sync-templates.js` before `vsce package` and verify the output matches `demo/`.



**esbuild config:** Update `esbuild.js` so that during build, `templates/**` is copied to `dist/templates/` (preserving directory structure). The wizard reads from `path.join(context.extensionPath, 'dist', 'templates', ...)`.



---



### 2. Dedicated Output Channel — Item 6



**New file:** `vscode-ail-chat/src/output-channel.ts`



Thin wrapper:

```typescript

export class AilOutputChannel {

  constructor(private readonly _channel: vscode.OutputChannel) {}

  spawn(binary: string, args: string[]): void { ... }

  event(e: AilEvent): void { ... }  // JSON.stringify the event, prefix [event]

  stderr(line: string): void { ... }

  exit(code: number | null): void { ... }

  error(msg: string): void { ... }

}

```



**`extension.ts`:** Create the channel and pass to `ChatViewProvider`:

```typescript

const outputChannel = vscode.window.createOutputChannel('AIL');

context.subscriptions.push(outputChannel);

```



**`AilProcessManager`:** Accept optional `AilOutputChannel` in constructor. Wire into:

- `start()`: log `[spawn] ail ${args.join(' ')}`

- `parseNdjsonStream` callback: log every `AilEvent`

- `proc.stderr`: pipe to `outputChannel.stderr()` instead of silently discarding (the current `proc.stderr?.resume()` drops all stderr — this is why diagnostics are invisible)

- `proc.on('close')`: log `[exit] code=${code}`

- `proc.on('error')`: log `[error] ${err.message}`



The channel does NOT auto-reveal. The user opens it via `View → Output → AIL` when they need diagnostics.



---



### 3. Windows Process Termination Fix — Item 7



The root cause is two bugs combined: (a) Node's `proc.kill('SIGTERM')` on Windows only terminates the immediate process, leaving `ail`'s child Claude CLI process holding the stdio pipes open, so `close` never fires; (b) `_activeProcess` is only cleared inside the `close` handler, so the stale pointer triggers the "A pipeline is already running" guard.



Per the project's DI principles, platform-specific logic goes behind an interface — the same way `Runner` abstracts CLI vs HTTP backends in `ail-core`.



**New file:** `vscode-ail-chat/src/process/process-killer.ts`



```typescript

export interface ProcessKiller {

  /** Terminate the process and its descendants. Returns when termination is requested. */

  kill(proc: ChildProcess): Promise<void>;

}

```



**New file:** `vscode-ail-chat/src/process/posix-process-killer.ts` — `SIGTERM` then `SIGKILL` after 5s (current behavior).



**New file:** `vscode-ail-chat/src/process/windows-process-killer.ts` — `taskkill /F /T /PID <pid>` via `child_process.spawn` (async, no shell injection risk since pid is a number).



**New file:** `vscode-ail-chat/src/process/process-killer-factory.ts`:

```typescript

export function createProcessKiller(): ProcessKiller {

  return process.platform === 'win32'

    ? new WindowsProcessKiller()

    : new PosixProcessKiller();

}

```



**`ail-process-manager.ts` changes:**

- Constructor accepts an injected `ProcessKiller` (default in factory): `new AilProcessManager(binary, cwd, outputChannel, killer)`

- `cancel()` clears `_activeProcess` **immediately** before delegating to the killer — do not wait for the `close` event to clear it. The `close` handler becomes a no-op fallback that does nothing if the pointer is already null. This is the primary fix: the `close` event never fires on Windows while orphaned children hold the pipes open, so clearing inside `close` is too late.

  ```typescript
  cancel(): void {
    if (!this._activeProcess) return;
    const proc = this._activeProcess;
    this._activeProcess = undefined;   // clear NOW, before async kill
    void this._killer.kill(proc).catch((err) => {
      this._outputChannel?.error(`kill failed: ${err.message}`);
    });
  }
  ```

- `cancel()` is fire-and-forget from the caller's perspective (returns `void`, errors are logged to the output channel). Do not make it `Promise<void>` — callers in `chat-view-provider.ts` and `webviewView.onDidDispose` don't await, and there's nothing actionable to propagate.

- Tests inject a `StubProcessKiller` to verify cancel behavior without touching real processes.



**Stale-pointer check in `start()` is no longer needed** once `_activeProcess` is cleared eagerly in `cancel()`. Remove the belt-and-suspenders guard from `start()` — it papers over a race that the eager clear eliminates.



---



### 4. Toggle Side Panel — Item 8



#### 4a. Package manifest additions (`package.json`)



Add two new tree views to `ail-chat-sidebar`:

```json

"views": {

  "ail-chat-sidebar": [

    { "id": "ail-chat.chatView", "name": "Chat", "type": "webview" },

    { "id": "ail-chat.historyView", "name": "Run History", "when": "ail-chat.panelVisible" },

    { "id": "ail-chat.stepsView",   "name": "Pipeline Steps", "when": "ail-chat.panelVisible" }

  ]

}

```



Add toggle command with layout icon:

```json

{

  "command": "ail-chat.toggleInfoPanel",

  "title": "Toggle History & Steps",

  "icon": "$(layout-sidebar-right)"

}

```



Wire icon button to the chat webview's title bar:

```json

"menus": {

  "view/title": [

    {

      "command": "ail-chat.toggleInfoPanel",

      "when": "view == ail-chat.chatView",

      "group": "navigation"

    }

  ]

}

```



#### 4b. New file: `vscode-ail-chat/src/history-tree-provider.ts`



`RunHistoryProvider implements vscode.TreeDataProvider<RunItem>`:

- Items stored in `workspaceState` as `{ runId: string, prompt: string, timestamp: number, logPath: string }[]` — **compute and store the full log path at recording time** (`~/.ail/projects/<sha1(cwd)>/runs/<runId>.jsonl` where `cwd` is the workspace root at that moment). Hashing the current cwd at click-time would lose history when the user switches workspaces.

- History is **capped at 100 entries** (evict oldest on overflow). workspaceState has no automatic compaction; an unbounded list degrades tree render performance.

- `addRun(runId, prompt, cwd)` called by `ChatViewProvider` on each `run_started` event

- Tree item: label = first 60 chars of prompt, description = relative timestamp

- `contextValue = 'runItem'` → enables `ail-chat.openRunLog` command



`ail-chat.openRunLog` command:

1. Read the pre-computed `logPath` from the `RunItem` (stored at record time — no cwd hashing here)

2. Read and parse NDJSON entries

3. Format as readable text (step IDs, responses, costs, errors)

4. Open as a VS Code document: `vscode.workspace.openTextDocument({ content, language: 'log' })` then `vscode.window.showTextDocument(...)`. **Do not post a webview message** — the native document gives the user find, copy, and folding for free and keeps the React app unchanged.



SHA1 of cwd: use Node's `crypto.createHash('sha1').update(cwd).digest('hex')`



#### 4c. New file: `vscode-ail-chat/src/steps-tree-provider.ts`



`PipelineStepsProvider implements vscode.TreeDataProvider<StepItem>`:

- Reads the active pipeline YAML (from `ChatViewProvider.currentPipeline`)

- Parses steps using the `yaml` package (already a dependency)

- **Top-level steps only** — do not recurse into sub-pipeline files (`pipeline: ./workflows/foo.ail.yaml`). Oh-my-ail has 4 workflows × N agents each; recursive expansion would produce an illegible tree and requires loading arbitrary external files. Show sub-pipeline steps as a single node labelled with the referenced filename.

- Each tree item: step id + type icon (prompt/context/skill/sub-pipeline)

- Show an error node (no expand) when the pipeline path is missing, YAML is malformed, or the provider is in passthrough mode.

- On item click → `ail-chat.openStep` command



`ail-chat.openStep` command:

1. Open YAML file: `vscode.window.showTextDocument(vscode.Uri.file(pipelinePath))`

2. Search for the step ID in the document text to find the line number

3. Reveal and select: `editor.revealRange(range, TextEditorRevealType.InCenter)`



`ChatViewProvider` needs a new `currentPipeline` getter (already exists as `_resolvedPipeline()` — expose it publicly) and a notify method so the steps provider refreshes when pipeline changes.



#### 4d. Toggle command in `extension.ts`



```typescript

let panelVisible = false;

vscode.commands.registerCommand('ail-chat.toggleInfoPanel', () => {

  panelVisible = !panelVisible;

  void vscode.commands.executeCommand(

    'setContext', 'ail-chat.panelVisible', panelVisible

  );

});

```



---



## Wiring in `chat-view-provider.ts`



- Accept `RunHistoryProvider` and `PipelineStepsProvider` via constructor or setter

- On `run_started` event from process manager → call `historyProvider.addRun(runId, prompt)`

- On `pipelineChanged` → call `stepsProvider.refresh(newPipeline)`

- Open run log as a native VS Code document (`openTextDocument` / `showTextDocument`) — no webview message required



---



## Webview React additions

None required. Run log display uses `vscode.workspace.openTextDocument` (native VS Code document, no React changes). Toggle panel state is handled entirely via VS Code tree view context keys.



---



## Test Coverage Requirements

Each new file needs a corresponding test file under `vscode-ail-chat/test/`. The existing test suite (vitest, node environment) avoids importing `vscode` directly — new tests for tree providers and the wizard must do the same via a **vscode stub**. Port `vscode-ail/src/test/vscode-stub.js` into `vscode-ail-chat/test/vscode-stub.js` and mock the module in vitest (`vi.mock('vscode', () => require('./vscode-stub'))`).

| Test file | What to cover |
|-----------|---------------|
| `test/install-wizard.test.ts` | No-pipeline detection across all four path patterns; wizard no-ops when pipeline exists; dismiss flag set only on "Dismiss" button (not on QuickPick Escape); template copy to correct directory; `defaultPipeline` setting updated; repeated activation suppressed after dismiss |
| `test/process-killer-factory.test.ts` | Returns `WindowsProcessKiller` when `process.platform === 'win32'`, `PosixProcessKiller` otherwise |
| `test/process/windows-process-killer.test.ts` | Constructs `taskkill` args as `['/F', '/T', '/PID', String(pid)]`; pid is numeric so no injection possible — assert that directly, no real spawn needed |
| `test/process/posix-process-killer.test.ts` | SIGTERM then SIGKILL after 5s if process still alive — requires spawning a real sleep process; guard with `it.skipIf(process.platform === 'win32')` |
| `test/ail-process-manager.test.ts` (extend) | `cancel()` clears `_activeProcess` immediately (before `close` fires); inject `StubProcessKiller`, call `cancel()`, assert `isRunning === false` before the kill resolves; then call `start()` and assert it does not reject |
| `test/output-channel.test.ts` | Each method (`spawn`, `event`, `stderr`, `exit`, `error`) appends the correct prefix to a mock `OutputChannel.appendLine` |
| `test/history-tree-provider.test.ts` | `addRun` stores entries with `logPath` computed from cwd; capped at 100 (101st evicts oldest); round-trips through workspaceState mock; `getChildren` returns items in reverse-chronological order |
| `test/steps-tree-provider.test.ts` | Parses a valid pipeline and returns one `StepItem` per top-level step; sub-pipeline reference shows single node with filename label; malformed YAML returns error node without throwing; passthrough mode (null pipeline) returns empty tree |

## Spec Impact

**No `spec/` updates required.** This plan modifies only the VS Code extension (`vscode-ail-chat/`). It does not change runner behavior, pipeline semantics, turn log format, template variable resolution, or any other behavior governed by `spec/core/` or `spec/runner/`.

---

## Verification



1. **Wizard appears:** Open an empty workspace → "No AIL pipeline found" notification → click "Configure AIL" → QuickPick shows 3 options

2. **Starter install:** Pick "Starter" → `.ail/default.yaml` + `.ail/README.md` exist → README opens in markdown preview → typing a prompt in chat works without further setup

3. **Oh My AIL install:** Pick "Oh My AIL" → `.ail/oh-my-ail/` contains the full demo tree (`.ohmy.ail.yaml`, `agents/`, `workflows/`, `prompts/`) → `defaultPipeline` setting points to `.ail/oh-my-ail/.ohmy.ail.yaml`

4. **Superpowers install:** Pick "Superpowers" → second QuickPick lists all `*.ail.yaml` files → choose one → `.ail/superpowers/` populated → `defaultPipeline` points to the chosen file

5. **Templates valid:** Run `ail validate --pipeline .ail/default.yaml` (and the other entry points) — all pass

6. **Templates stay in sync:** Run `npm run build` in `vscode-ail-chat/` → `templates/oh-my-ail/` and `templates/superpowers/` byte-match `demo/oh-my-ail/` and `demo/superpowers/`

7. **Output channel:** Run a prompt → open `View → Output → AIL` → see spawn command, every NDJSON event, any stderr, exit code

8. **Windows process fix:** On Windows (or via `WindowsProcessKiller` unit test), start a prompt → cancel it → immediately submit another prompt → no "A pipeline is already running" error

9. **Stale pointer guard:** Force a `_activeProcess` to remain set with `exitCode = 0` (test harness) → `start()` clears it and proceeds

10. **Toggle panel:** Click the side-panel icon in the chat view title → "Run History" and "Pipeline Steps" tree views appear; click again → they disappear

11. **History click:** Run two prompts → click an item in "Run History" → a VS Code text document opens with the formatted run log (language: `log`); find/copy/fold work; webview is untouched

12. **Step click:** Open a multi-step pipeline → click a step in "Pipeline Steps" → YAML opens at that step's `id:` line, line revealed in viewport

13. **All unit tests pass:** `npm test` in `vscode-ail-chat/` is green with the new test files included

14. **Sync freshness check:** Run `npm run build` in a clean state, then confirm `templates/oh-my-ail/` and `templates/superpowers/` byte-match `demo/oh-my-ail/` and `demo/superpowers/`; also run `vsce package --no-yarn` and confirm the VSIX contains `dist/templates/`



---



## Build impact

- `esbuild.js` must copy `templates/**` into `dist/templates/` so templates are bundled in the VSIX

- `package.json` scripts: `sync-templates` step runs in `npm run build` **and** in `vscode:prepublish` (before `node esbuild.js --production`). Example:

  ```json
  "vscode:prepublish": "node scripts/sync-templates.js && node esbuild.js --production"
  ```

- `vscode-ail-chat/.gitignore`: add `templates/oh-my-ail/` and `templates/superpowers/` — these are generated artifacts, not source

- `package.json` localResourceRoots unchanged (templates are read from extensionPath at runtime, not via webview URIs)

- New source files are TypeScript only (no new React components needed for tree views)

- `tsconfig.json` should already pick up new files under `src/`

