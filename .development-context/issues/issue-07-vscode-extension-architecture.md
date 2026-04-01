# Issue #7: VSCode Extension Clean Architecture Refactor

## Context

The current `vscode-ail` extension works but has structural problems that will make adding features (issue #6) painful:

- **No process abstraction:** `src/commands/run.ts` directly calls `spawn()` — untestable, no seam for mocking
- **Global module state:** `src/commands/run.ts` holds running process state at module scope
- **Scattered event handling:** NDJSON parsing and state transitions mixed into command handlers
- **No DI:** `vscode.OutputChannel` and `vscode.ExtensionContext` passed ad-hoc, not injected

The goal is to establish clean layers before implementing the broader feature set in issue #6.

---

## Current Architecture

```
extension.ts          → activates, registers commands, creates tree providers
commands/run.ts       → spawn() + inline NDJSON parsing + module-level state
commands/validate.ts  → spawn() for validate subcommand
views/
  PipelineTreeProvider.ts   → tree data provider
  StepsTreeProvider.ts      → tree data provider
  ExecutionPanel.ts         → webview panel
  ChatProvider.ts           → chat webview provider
state.ts              → minimal shared state module
```

Tests cover only pure functions: `binary.test.ts`, `ndjson.test.ts`, `pipeline-tree.test.ts`.

---

## Target Architecture

```
infrastructure/
  AilProcess.ts         → implements IAilClient via spawn()
  NdjsonParser.ts       → stream → typed RunnerEvent (already exists, extract)

application/
  IAilClient.ts         → interface: invoke(), validate(), cancel(), on(event)
  EventBus.ts           → typed pub/sub for RunnerEvents → VS Code state
  PipelineService.ts    → query: list pipelines, load YAML, resolve path
  RunnerService.ts      → command: start run, handle events, update state
  StateManager.ts       → single source of truth for extension state

commands/
  RunCommand.ts         → delegates to RunnerService (no spawn here)
  ValidateCommand.ts    → delegates to PipelineService

views/                  → subscribe to EventBus, read StateManager (unchanged externally)

extension.ts            → wires DI, registers commands (thinner)
```

---

## Implementation Steps

### Step 1: Define `IAilClient` interface

**File:** `vscode-ail/src/application/IAilClient.ts` (new)

```typescript
import { RunnerEvent } from './events';

export interface IAilClient {
  invoke(prompt: string, pipeline: string, options: InvokeOptions): Promise<void>;
  validate(pipeline: string): Promise<ValidationResult>;
  cancel(): void;
  onEvent(handler: (event: RunnerEvent) => void): Disposable;
}

export interface InvokeOptions {
  headless?: boolean;
  outputFormat?: 'text' | 'json';
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}
```

---

### Step 2: Define typed events

**File:** `vscode-ail/src/application/events.ts` (new, extracted from existing NDJSON parsing)

Mirror the NDJSON event types already parsed in `ndjson.ts` — do not rewrite parsing logic, just define the TypeScript types:

```typescript
export type RunnerEvent =
  | { type: 'step_started'; stepId: string; stepIndex: number; totalSteps: number }
  | { type: 'stream_delta'; text: string }
  | { type: 'step_completed'; stepId: string }
  | { type: 'pipeline_completed' }
  | { type: 'error'; message: string };
```

---

### Step 3: Create `ServiceContext` (DI container)

**File:** `vscode-ail/src/application/ServiceContext.ts` (new)

```typescript
import * as vscode from 'vscode';
import { IAilClient } from './IAilClient';

export interface ServiceContext {
  readonly extensionContext: vscode.ExtensionContext;
  readonly outputChannel: vscode.OutputChannel;
  readonly client: IAilClient;
}

export function createServiceContext(
  extensionContext: vscode.ExtensionContext,
  client: IAilClient,
): ServiceContext {
  const outputChannel = vscode.window.createOutputChannel('ail');
  extensionContext.subscriptions.push(outputChannel);
  return { extensionContext, outputChannel, client };
}
```

---

### Step 4: Create `AilProcess` infrastructure class

**File:** `vscode-ail/src/infrastructure/AilProcess.ts` (new)

Move spawn logic from `commands/run.ts` here. Implement `IAilClient`:
- `invoke()` → spawns `ail --once ... --output-format json`, pipes NDJSON to `NdjsonParser`, emits typed events
- `validate()` → spawns `ail validate --pipeline ...`, returns result
- `cancel()` → kills child process
- `onEvent()` → register handler, return disposable

Keep all `spawn()` / `child_process` usage contained in this file.

---

### Step 5: Create `EventBus`

**File:** `vscode-ail/src/application/EventBus.ts` (new)

```typescript
export class EventBus {
  private handlers = new Map<string, Set<(e: any) => void>>();

  emit<T extends RunnerEvent>(event: T): void { ... }
  on<K extends RunnerEvent['type']>(type: K, handler: (e: Extract<RunnerEvent, {type: K}>) => void): vscode.Disposable { ... }
}
```

---

### Step 6: Refactor `commands/run.ts` → `RunnerService`

**File:** `vscode-ail/src/application/RunnerService.ts` (new)
**File:** `vscode-ail/src/commands/RunCommand.ts` (replace existing `run.ts`)

`RunnerService` holds run state (active run ID, step counts, etc.) and subscribes to `EventBus` to update `vscode.TreeDataProvider` refresh triggers.

`RunCommand` becomes thin: validate preconditions → `service.startRun(pipeline, prompt)`.

Remove module-level globals from `run.ts`.

---

### Step 7: Wire DI in `extension.ts`

**File:** `vscode-ail/src/extension.ts` (refactor)

```typescript
export async function activate(context: vscode.ExtensionContext) {
  const client = new AilProcess(getBinaryPath(context));
  const services = createServiceContext(context, client);
  const bus = new EventBus();
  const runnerService = new RunnerService(services, bus);

  context.subscriptions.push(
    vscode.commands.registerCommand('ail.run', () => new RunCommand(runnerService).execute()),
    vscode.commands.registerCommand('ail.validate', () => new ValidateCommand(services).execute()),
    // ... existing tree provider registrations unchanged
  );
}
```

---

### Step 8: Add unit tests for new services

**Files:** `vscode-ail/test/RunnerService.test.ts`, `vscode-ail/test/AilProcess.test.ts`

Mock `IAilClient` in tests — the interface is the seam:

```typescript
const mockClient: IAilClient = {
  invoke: jest.fn(),
  validate: jest.fn().mockResolvedValue({ valid: true, errors: [] }),
  cancel: jest.fn(),
  onEvent: jest.fn().mockReturnValue({ dispose: jest.fn() }),
};
```

---

## Files Created / Modified

| File | Action |
|------|--------|
| `src/application/IAilClient.ts` | Create |
| `src/application/events.ts` | Create |
| `src/application/ServiceContext.ts` | Create |
| `src/application/EventBus.ts` | Create |
| `src/application/RunnerService.ts` | Create |
| `src/infrastructure/AilProcess.ts` | Create (extract from run.ts) |
| `src/commands/RunCommand.ts` | Create (replace run.ts logic) |
| `src/commands/ValidateCommand.ts` | Thin wrapper (extract from validate.ts) |
| `src/extension.ts` | Refactor (thinner activation, DI wiring) |
| `src/commands/run.ts` | Delete or reduce to re-export |
| `test/RunnerService.test.ts` | Create |
| `test/AilProcess.test.ts` | Create |

Existing test files and tree provider views are **unchanged** — they remain valid throughout.

---

## Migration Strategy

Do this in phases so the extension stays functional:

1. Define interfaces + types (no behavior change)
2. Create `AilProcess` wrapping existing spawn code (both paths exist temporarily)
3. Wire `ServiceContext` in `extension.ts`
4. Replace `commands/run.ts` globals with `RunnerService`
5. Delete dead code

---

## Verification

```bash
cd vscode-ail
npm run compile        # no TypeScript errors
npm test               # existing + new tests pass
```

Manual: Install extension via `F5` in VS Code, run a pipeline, confirm output still streams to panel, validate command still works.
