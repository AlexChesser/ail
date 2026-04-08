/**
 * graphTransform — integration tests against real demo pipeline files.
 *
 * Each demo pipeline (oh-my-ail + superpowers) must produce a non-empty graph
 * with no fatal errors. These tests exist to catch the class of crash that
 * "SubPipelineGroupNode is not a function" represented: a valid YAML file that
 * produces graph data triggering a runtime failure in the visualizer.
 */

import { describe, it, expect } from 'vitest';
import * as path from 'path';
import { transformPipeline } from '../src/pipeline-graph/graphTransform';

// Repo root relative to this test file (test/ → vscode-ail-chat/ → ail/).
const REPO_ROOT = path.resolve(__dirname, '..', '..');

function fixture(rel: string): string {
  return path.join(REPO_ROOT, rel);
}

// ── oh-my-ail pipelines ─────────────────────────────────────────────────────

describe('graphTransform — oh-my-ail', () => {
  const OMA = 'demo/oh-my-ail';

  it('loads .ohmy.ail.yaml (invocation + on_result branches to sub-pipelines)', () => {
    const result = transformPipeline(fixture(`${OMA}/.ohmy.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
    expect(result.edges.length).toBeGreaterThan(0);
    // Should expand the 4 workflow sub-pipelines referenced via on_result.
    const groupNodes = result.nodes.filter((n) => n.type === 'subPipelineGroup');
    expect(groupNodes.length).toBeGreaterThanOrEqual(4);
  });

  it('loads agents/atlas.ail.yaml (2-step linear prompt pipeline)', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/atlas.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes).toHaveLength(2);
    expect(result.edges).toHaveLength(1);
  });

  it('loads agents/explore.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/explore.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads agents/hephaestus.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/hephaestus.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads agents/librarian.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/librarian.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads agents/metis.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/metis.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads agents/momus.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/momus.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads agents/oracle.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/oracle.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads agents/prometheus.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/agents/prometheus.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads workflows/trivial.ail.yaml (single sub-pipeline step)', () => {
    const result = transformPipeline(fixture(`${OMA}/workflows/trivial.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads workflows/explicit.ail.yaml (explore → atlas)', () => {
    const result = transformPipeline(fixture(`${OMA}/workflows/explicit.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads workflows/exploratory.ail.yaml (4 sequential sub-pipeline steps)', () => {
    const result = transformPipeline(fixture(`${OMA}/workflows/exploratory.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
    // 4 group nodes for explore, prometheus, momus, atlas.
    const groupNodes = result.nodes.filter((n) => n.type === 'subPipelineGroup');
    expect(groupNodes.length).toBe(4);
  });

  it('loads workflows/ambiguous.ail.yaml', () => {
    const result = transformPipeline(fixture(`${OMA}/workflows/ambiguous.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });
});

// ── superpowers pipelines ────────────────────────────────────────────────────

describe('graphTransform — superpowers', () => {
  const SP = 'demo/superpowers';

  it('loads code-review.ail.yaml (context steps + on_result branching + sub-pipeline)', () => {
    const result = transformPipeline(fixture(`${SP}/code-review.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads code-review-sub.ail.yaml (mixed tools.allow + tools.deny)', () => {
    const result = transformPipeline(fixture(`${SP}/code-review-sub.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads brainstorming.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/brainstorming.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads executing-plans.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/executing-plans.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads finishing-branch.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/finishing-branch.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads git-worktree-setup.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/git-worktree-setup.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads parallel-debug.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/parallel-debug.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads subagent-development.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/subagent-development.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads tdd-enriched.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/tdd-enriched.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });

  it('loads writing-plans.ail.yaml', () => {
    const result = transformPipeline(fixture(`${SP}/writing-plans.ail.yaml`));
    expect(result.errors).toHaveLength(0);
    expect(result.nodes.length).toBeGreaterThan(0);
  });
});

// ── graph invariants ─────────────────────────────────────────────────────────

describe('graphTransform — graph invariants', () => {
  it('all edge endpoints reference existing node IDs', () => {
    const pipelines = [
      fixture('demo/oh-my-ail/.ohmy.ail.yaml'),
      fixture('demo/oh-my-ail/workflows/exploratory.ail.yaml'),
      fixture('demo/superpowers/code-review.ail.yaml'),
    ];

    for (const p of pipelines) {
      const result = transformPipeline(p);
      const nodeIds = new Set(result.nodes.map((n) => n.id));
      for (const edge of result.edges) {
        expect(nodeIds.has(edge.source), `${p}: edge source ${edge.source} not in nodes`).toBe(true);
        expect(nodeIds.has(edge.target), `${p}: edge target ${edge.target} not in nodes`).toBe(true);
      }
    }
  });

  it('all node IDs are unique within a pipeline', () => {
    const pipelines = [
      fixture('demo/oh-my-ail/.ohmy.ail.yaml'),
      fixture('demo/oh-my-ail/workflows/exploratory.ail.yaml'),
      fixture('demo/superpowers/code-review.ail.yaml'),
    ];

    for (const p of pipelines) {
      const result = transformPipeline(p);
      const ids = result.nodes.map((n) => n.id);
      const unique = new Set(ids);
      expect(unique.size, `${p}: duplicate node IDs found`).toBe(ids.length);
    }
  });
});
