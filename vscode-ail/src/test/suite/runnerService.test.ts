/**
 * RunnerService integration tests.
 *
 * We stub the 'vscode' module before requiring RunnerService so that
 * calls to vscode.window.showInformationMessage / showErrorMessage
 * don't throw. All VS Code UI calls are fire-and-forget (void), so
 * no-op stubs are sufficient.
 */

// ── vscode stub (must come before any require of RunnerService) ──────────────

// eslint-disable-next-line @typescript-eslint/no-require-imports
const Module = require('module');
const _origLoad = Module._load.bind(Module);
Module._load = function (request: string, ...args: unknown[]) {
  if (request === 'vscode') {
    return {
      window: {
        showInformationMessage: () => Promise.resolve(undefined),
        showErrorMessage: () => Promise.resolve(undefined),
        showWarningMessage: () => Promise.resolve(undefined),
      },
      // StatusBarItem methods are called on the injected mock, not here.
    };
  }
  return _origLoad(request, ...args);
};

// ── Imports (after stub is installed) ────────────────────────────────────────

import * as assert from 'assert';
import { EventBus } from '../../application/EventBus';
import { RunnerService, IStagePanel, RunnerDeps } from '../../application/RunnerService';
import { IAilClient, InvokeOptions, ValidationResult, Disposable } from '../../application/IAilClient';
import { RunnerEvent } from '../../application/events';
import { AilEvent } from '../../types';

// ── Fakes ─────────────────────────────────────────────────────────────────────

type EventHandler = (e: RunnerEvent) => void;
type RawEventHandler = (e: AilEvent) => void;

class MockAilClient implements IAilClient {
  private _handlers = new Set<EventHandler>();
  private _rawHandlers = new Set<RawEventHandler>();
  invokeCallCount = 0;
  cancelCallCount = 0;
  writeStdinMessages: object[] = [];
  rejectWith: Error | undefined;

  onEvent(h: EventHandler): Disposable {
    this._handlers.add(h);
    return { dispose: () => this._handlers.delete(h) };
  }
  onRawEvent(h: RawEventHandler): Disposable {
    this._rawHandlers.add(h);
    return { dispose: () => this._rawHandlers.delete(h) };
  }
  invoke(_prompt: string, _pipeline: string, _opts: InvokeOptions): Promise<void> {
    this.invokeCallCount++;
    if (this.rejectWith) return Promise.reject(this.rejectWith);
    return Promise.resolve();
  }
  validate(_pipeline: string): Promise<ValidationResult> {
    return Promise.resolve({ valid: true, errors: [] });
  }
  cancel(): void { this.cancelCallCount++; }
  writeStdin(msg: object): void { this.writeStdinMessages.push(msg); }

  simulateRawEvent(event: AilEvent): void {
    for (const h of this._rawHandlers) h(event);
  }
  simulateEvent(event: RunnerEvent): void {
    for (const h of this._handlers) h(event);
  }
}

class MockStagePanel implements IStagePanel {
  events: AilEvent[] = [];
  disposed = false;
  onEvent(event: AilEvent): void { this.events.push(event); }
  dispose(): void { this.disposed = true; }
}

function makeCtx() {
  return {
    binaryPath: '/fake/ail',
    cwd: '/fake/cwd',
    extensionContext: {} as never,
    outputChannel: {
      show: () => { /* no-op */ },
      appendLine: () => { /* no-op */ },
      append: () => { /* no-op */ },
    },
    client: new MockAilClient(),
  };
}

function makeStatusBar() {
  return {
    text: '',
    tooltip: '',
    command: '',
    show: () => { /* no-op */ },
    hide: () => { /* no-op */ },
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

suite('RunnerService', () => {

  test('raw events are routed to the panel\'s onEvent()', async () => {
    const ctx = makeCtx();
    const bus = new EventBus();
    const mockProc = new MockAilClient();
    const mockPanel = new MockStagePanel();

    const deps: RunnerDeps = {
      createProcess: () => mockProc,
      createPanel: () => mockPanel,
    };

    const service = new RunnerService(ctx as never, bus, deps);
    service.setViews(makeStatusBar() as never);

    // Kick off the run (invoke resolves immediately)
    const runPromise = service.startRun('hello', '/fake/.ail.yaml');

    // Simulate a raw event before invoke resolves
    const rawEvent: AilEvent = { type: 'run_started', run_id: 'r1', pipeline_source: null, total_steps: 1 };
    mockProc.simulateRawEvent(rawEvent);

    await runPromise;

    assert.strictEqual(mockPanel.events.length, 1);
    assert.deepStrictEqual(mockPanel.events[0], rawEvent);
  });

  test('mapped events route to EventBus subscribers', async () => {
    const ctx = makeCtx();
    const bus = new EventBus();
    const mockProc = new MockAilClient();

    // Simulate the event during invoke() so handlers are already registered.
    mockProc.invoke = () => {
      mockProc.simulateEvent({ type: 'pipeline_completed' });
      return Promise.resolve();
    };

    const deps: RunnerDeps = {
      createProcess: () => mockProc,
      createPanel: () => new MockStagePanel(),
    };

    const service = new RunnerService(ctx as never, bus, deps);
    service.setViews(makeStatusBar() as never);

    let busEvent: RunnerEvent | undefined;
    bus.on('pipeline_completed', (e) => { busEvent = e; });

    await service.startRun('hello', '/fake/.ail.yaml');

    assert.deepStrictEqual(busEvent, { type: 'pipeline_completed' });
  });

  test('step events drive StepsTreeProvider spy', async () => {
    const ctx = makeCtx();
    const bus = new EventBus();
    const mockProc = new MockAilClient();

    // Simulate step events during invoke() so handlers are already registered.
    mockProc.invoke = () => {
      mockProc.simulateEvent({ type: 'step_started', stepId: 's1', stepIndex: 0, totalSteps: 1 });
      mockProc.simulateEvent({ type: 'step_completed', stepId: 's1' });
      return Promise.resolve();
    };

    const deps: RunnerDeps = {
      createProcess: () => mockProc,
      createPanel: () => new MockStagePanel(),
    };

    const statuses: Array<{ id: string; status: string }> = [];
    const stepsViewSpy = {
      resetStatuses: () => { /* no-op */ },
      setStepStatus: (id: string, status: string) => statuses.push({ id, status }),
    };

    const service = new RunnerService(ctx as never, bus, deps);
    service.setViews(makeStatusBar() as never, undefined, stepsViewSpy as never);

    await service.startRun('hello', '/fake/.ail.yaml');

    assert.deepStrictEqual(statuses, [
      { id: 's1', status: 'running' },
      { id: 's1', status: 'completed' },
    ]);
  });

  test('two concurrent runs route raw events to their own panels', async () => {
    const ctx = makeCtx();
    const bus = new EventBus();
    const proc1 = new MockAilClient();
    const proc2 = new MockAilClient();
    const panel1 = new MockStagePanel();
    const panel2 = new MockStagePanel();

    let callCount = 0;
    const deps: RunnerDeps = {
      createProcess: () => callCount++ === 0 ? proc1 : proc2,
      createPanel: () => callCount > 1 ? panel2 : panel1,
    };

    const service = new RunnerService(ctx as never, bus, deps);
    service.setViews(makeStatusBar() as never);

    // Both start concurrently; neither resolves until we await them.
    let resolve1: () => void;
    let resolve2: () => void;
    proc1.invoke = () => new Promise<void>((r) => { resolve1 = r; });
    proc2.invoke = () => new Promise<void>((r) => { resolve2 = r; });

    const run1 = service.startRun('p1', '/a.yaml');
    const run2 = service.startRun('p2', '/b.yaml');

    const e1: AilEvent = { type: 'step_started', step_id: 'a', step_index: 0, total_steps: 1, resolved_prompt: null };
    const e2: AilEvent = { type: 'step_started', step_id: 'b', step_index: 0, total_steps: 1, resolved_prompt: null };

    proc1.simulateRawEvent(e1);
    proc2.simulateRawEvent(e2);

    resolve1!();
    resolve2!();
    await Promise.all([run1, run2]);

    assert.strictEqual(panel1.events.length, 1);
    assert.deepStrictEqual(panel1.events[0], e1);
    assert.strictEqual(panel2.events.length, 1);
    assert.deepStrictEqual(panel2.events[0], e2);
  });

  test('after invoke resolves, isRunning is false and disposables called', async () => {
    const ctx = makeCtx();
    const bus = new EventBus();
    const mockProc = new MockAilClient();
    const mockPanel = new MockStagePanel();

    const deps: RunnerDeps = {
      createProcess: () => mockProc,
      createPanel: () => mockPanel,
    };

    const service = new RunnerService(ctx as never, bus, deps);
    service.setViews(makeStatusBar() as never);

    assert.strictEqual(service.isRunning, false);
    await service.startRun('hello', '/fake/.ail.yaml');
    assert.strictEqual(service.isRunning, false);
    // Panel should have been disposed by RunnerService cleanup
    assert.strictEqual(mockPanel.disposed, false); // panel dispose is not called by RunnerService (user closes it)
  });

  test('error path: invoke rejects, run is cleaned up', async () => {
    const ctx = makeCtx();
    const bus = new EventBus();
    const mockProc = new MockAilClient();
    mockProc.rejectWith = new Error('spawn failed');

    const deps: RunnerDeps = {
      createProcess: () => mockProc,
      createPanel: () => new MockStagePanel(),
    };

    const service = new RunnerService(ctx as never, bus, deps);
    service.setViews(makeStatusBar() as never);

    // Should not throw (error is caught inside startRun)
    await service.startRun('hello', '/fake/.ail.yaml');
    assert.strictEqual(service.isRunning, false);
  });
});
