import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as os from 'os';
import * as fs from 'fs';
import * as path from 'path';

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
    showTextDocument: vi.fn(() => Promise.resolve({
      revealRange: vi.fn(),
    })),
  },
  Uri: { file: (p: string) => ({ fsPath: p }) },
  Position: class Position { constructor(public line: number, public character: number) {} },
  Range: class Range { constructor(public start: unknown, public end: unknown) {} },
  TextEditorRevealType: { InCenter: 2 },
}));

// ── Import under test ─────────────────────────────────────────────────────────

import { PipelineStepsProvider, StepItem, parseStepsFromYaml } from '../src/steps-tree-provider';

// ── Helpers ───────────────────────────────────────────────────────────────────

function writeTempYaml(content: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ail-steps-test-'));
  const filePath = path.join(dir, 'pipeline.yaml');
  fs.writeFileSync(filePath, content);
  return filePath;
}

beforeEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('parseStepsFromYaml', () => {
  it('returns one StepItem per top-level step', () => {
    const p = writeTempYaml(`
version: "0.0.1"
pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"
  - id: review
    prompt: review it
`);
    const steps = parseStepsFromYaml(p);
    expect(steps).toHaveLength(2);
    expect(steps[0].id).toBe('invocation');
    expect(steps[1].id).toBe('review');
  });

  it('detects step types correctly', () => {
    const p = writeTempYaml(`
pipeline:
  - id: s1
    prompt: hi
  - id: s2
    context:
      shell: echo hi
  - id: s3
    skill: brainstorm
`);
    const steps = parseStepsFromYaml(p);
    expect(steps[0].type).toBe('prompt');
    expect(steps[1].type).toBe('context');
    expect(steps[2].type).toBe('skill');
  });

  it('renders sub-pipeline reference as single node with filename label', () => {
    const p = writeTempYaml(`
pipeline:
  - id: sub
    pipeline: ./workflows/foo.ail.yaml
`);
    const steps = parseStepsFromYaml(p);
    expect(steps).toHaveLength(1);
    expect(steps[0].id).toContain('foo.ail.yaml');
    expect(steps[0].type).toBe('sub-pipeline');
  });

  it('returns empty array for malformed YAML without throwing', () => {
    const p = writeTempYaml('this: is: not: valid: yaml: {{{{');
    expect(() => parseStepsFromYaml(p)).not.toThrow();
    const steps = parseStepsFromYaml(p);
    expect(steps).toHaveLength(0);
  });

  it('returns empty array when pipeline key is missing', () => {
    const p = writeTempYaml('version: "0.0.1"\nmeta:\n  name: test');
    expect(parseStepsFromYaml(p)).toHaveLength(0);
  });
});

describe('PipelineStepsProvider', () => {
  it('returns error item when no pipeline is loaded', () => {
    const provider = new PipelineStepsProvider();
    const items = provider.getChildren();
    expect(items).toHaveLength(1);
    expect((items[0] as StepItem).label).toContain('No pipeline');
  });

  it('returns StepItems for a valid pipeline', () => {
    const p = writeTempYaml(`
pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"
  - id: check
    context:
      shell: echo ok
`);
    const provider = new PipelineStepsProvider();
    provider.refresh(p);
    const items = provider.getChildren();
    expect(items).toHaveLength(2);
    expect(items[0]).toBeInstanceOf(StepItem);
  });

  it('returns error item for malformed YAML', () => {
    const p = writeTempYaml('not: valid: yaml: {{{');
    const provider = new PipelineStepsProvider();
    provider.refresh(p);
    const items = provider.getChildren();
    expect(items).toHaveLength(1);
  });

  it('returns error item for null/passthrough pipeline', () => {
    const provider = new PipelineStepsProvider();
    provider.refresh(null);
    const items = provider.getChildren();
    expect((items[0] as StepItem).label).toContain('No pipeline');
  });
});
