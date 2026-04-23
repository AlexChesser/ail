import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── vscode mock ───────────────────────────────────────────────────────────────

vi.mock('vscode', () => ({
  TreeItem: class TreeItem {
    constructor(public label: string, public collapsibleState: number) {}
  },
  TreeItemCollapsibleState: { None: 0, Collapsed: 1, Expanded: 2 },
  ThemeIcon: class ThemeIcon { constructor(public id: string) {} },
  EventEmitter: class EventEmitter {
    event = vi.fn();
    fire = vi.fn();
  },
  window: {
    showErrorMessage: vi.fn(() => Promise.resolve(undefined)),
    showTextDocument: vi.fn(() => Promise.resolve(undefined)),
  },
  workspace: {
    openTextDocument: vi.fn(() => Promise.resolve({ getText: () => '' })),
  },
  Uri: { file: (p: string) => ({ fsPath: p }) },
  Position: class Position { constructor(public line: number, public character: number) {} },
  Range: class Range { constructor(public start: unknown, public end: unknown) {} },
  TextEditorRevealType: { InCenter: 2 },
  commands: {
    registerCommand: vi.fn(() => ({ dispose: vi.fn() })),
  },
}));

// ── child_process mock ────────────────────────────────────────────────────────

const { mockSpawn } = vi.hoisted(() => ({ mockSpawn: vi.fn() }));
vi.mock('child_process', () => ({ spawn: mockSpawn }));

// ── Import under test ─────────────────────────────────────────────────────────

import { RunHistoryProvider, RunItem } from '../src/history-tree-provider';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeRunLogFn(output: string, fail = false) {
  return vi.fn(() => fail ? Promise.reject(new Error('failed')) : Promise.resolve(output));
}

function makeSession(overrides?: Partial<{
  run_id: string;
  started_at: number;
  steps: Array<{ step_id: string; prompt?: string }>;
}>) {
  return JSON.stringify({
    run_id: 'abc-123',
    started_at: Math.floor(Date.now() / 1000) - 60,
    steps: [{ step_id: 'invocation', prompt: 'hello world' }],
    ...overrides,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('RunHistoryProvider', () => {
  describe('refresh', () => {
    it('does not call runLogFn when binaryPath is empty', async () => {
      const fn = makeRunLogFn('');
      const provider = new RunHistoryProvider('', undefined, fn);
      provider.refresh();
      await Promise.resolve(); // flush microtasks
      expect(fn).not.toHaveBeenCalled();
    });

    it('calls ail logs with correct args after setBinaryPath', async () => {
      const fn = makeRunLogFn(makeSession());
      const provider = new RunHistoryProvider('', undefined, fn);
      provider.setBinaryPath('/usr/local/bin/ail');
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      expect(fn).toHaveBeenCalledWith('/usr/local/bin/ail', ['logs', '--format', 'json', '--limit', '100'], undefined);
    });

    it('populates tree items from NDJSON output', async () => {
      const output = [makeSession({ run_id: 'r1' }), makeSession({ run_id: 'r2' })].join('\n');
      const fn = makeRunLogFn(output);
      const provider = new RunHistoryProvider('/bin/ail', undefined, fn);
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      const items = provider.getChildren();
      expect(items).toHaveLength(2);
      expect(items[0]).toBeInstanceOf(RunItem);
    });

    it('uses invocation step prompt as label', async () => {
      const fn = makeRunLogFn(makeSession({ steps: [{ step_id: 'invocation', prompt: 'fix the bug' }] }));
      const provider = new RunHistoryProvider('/bin/ail', undefined, fn);
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      const items = provider.getChildren();
      expect((items[0] as RunItem).label).toContain('fix the bug');
    });

    it('truncates long prompts to 60 chars', async () => {
      const longPrompt = 'a'.repeat(80);
      const fn = makeRunLogFn(makeSession({ steps: [{ step_id: 'invocation', prompt: longPrompt }] }));
      const provider = new RunHistoryProvider('/bin/ail', undefined, fn);
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      const label = (provider.getChildren()[0] as RunItem).label as string;
      expect(label.length).toBeLessThanOrEqual(61); // 60 + ellipsis
    });

    it('shows empty state when no sessions returned', async () => {
      const fn = makeRunLogFn('');
      const provider = new RunHistoryProvider('/bin/ail', undefined, fn);
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      const items = provider.getChildren();
      expect(items).toHaveLength(1);
      expect((items[0] as RunItem).label).toContain('No runs');
    });

    it('shows error state when CLI call fails', async () => {
      const fn = makeRunLogFn('', true);
      const provider = new RunHistoryProvider('/bin/ail', undefined, fn);
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      const items = provider.getChildren();
      expect((items[0] as RunItem).label).toContain('Failed');
    });

    it('skips malformed NDJSON lines without throwing', async () => {
      const output = ['not-json', makeSession({ run_id: 'good' })].join('\n');
      const fn = makeRunLogFn(output);
      const provider = new RunHistoryProvider('/bin/ail', undefined, fn);
      provider.refresh();
      await new Promise((r) => setTimeout(r, 0));
      expect(provider.getChildren()).toHaveLength(1);
    });
  });

  describe('openRunLog', () => {
    it('calls ail log <runId> and opens a markdown document', async () => {
      const logContent = '# Run abc-123\nsome content';
      const fn = makeRunLogFn(logContent);
      const provider = new RunHistoryProvider('/bin/ail', '/workspace', fn);
      const item = new RunItem('abc-123', 'test prompt', Date.now() / 1000, '/bin/ail', '/workspace', fn);

      await provider.openRunLog(item);

      expect(fn).toHaveBeenCalledWith('/bin/ail', ['log', 'abc-123'], '/workspace');
    });
  });
});
