/**
 * graphTransform — converts a pipeline YAML file (and its sub-pipelines)
 * into React Flow nodes and edges for the pipeline graph visualizer.
 *
 * Pure function, no VS Code dependency. Fully unit-testable.
 */

import * as fs from 'fs';
import * as path from 'path';

// ── Public types (shared with webview via postMessage) ──────────────────────

export interface StepNodeData {
  stepId: string;
  type: 'prompt' | 'context' | 'pipeline' | 'action' | 'skill' | 'invocation';
  /** Pipeline meta.name or filename for sub-pipeline group headers. */
  pipelineName?: string;
  /** Source .ail.yaml file path (absolute). */
  sourceFile: string;
  /** 0-based line number of this step in the source file. */
  sourceLine: number;
  /** Raw prompt text (may be long — truncated by the UI). */
  prompt?: string;
  /** System prompt path or inline text. */
  systemPrompt?: string;
  /** append_system_prompt entries count. */
  appendSystemPromptCount?: number;
  /** Tool policy summary. */
  tools?: { allow: string[]; deny: string[] };
  /** Model override, if any. */
  model?: string;
  /** on_result branch count (edges are created separately). */
  onResultCount?: number;
  /** For sub-pipeline steps: the referenced pipeline path (raw, unresolved). */
  subPipelinePath?: string;
  /** Whether this is a sub-pipeline group wrapper node. */
  isSubPipelineGroup?: boolean;
  /** Human-readable label for on_result edge. */
  branchLabel?: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  /** Label for conditional edges (on_result branches). */
  label?: string;
  /** Whether this is a conditional (on_result) edge vs sequential. */
  conditional?: boolean;
}

export interface GraphNode {
  id: string;
  type: 'stepNode' | 'subPipelineGroup';
  position: { x: number; y: number };
  data: StepNodeData;
  /** For group nodes: ID of the parent group. */
  parentId?: string;
}

export interface TransformResult {
  nodes: GraphNode[];
  edges: GraphEdge[];
  errors: string[];
}

// ── YAML parsing (lightweight, no domain validation) ────────────────────────

// The `yaml` package is a dependency — esbuild bundles it into the extension host.
import { parse as yamlParse } from 'yaml';

const MAX_DEPTH = 16;

interface RawStep {
  id?: string;
  prompt?: string;
  skill?: string;
  pipeline?: string;
  action?: string;
  context?: { shell?: string } | string;
  system_prompt?: string;
  append_system_prompt?: unknown[];
  tools?: { allow?: string[]; deny?: string[] };
  model?: string;
  on_result?: RawOnResult[];
  message?: string;
}

interface RawOnResult {
  contains?: string;
  exit_code?: number | string;
  always?: boolean;
  action?: string;
  prompt?: string;
}

interface RawPipelineFile {
  version?: string;
  meta?: { name?: string };
  pipeline?: RawStep[];
}

/**
 * Transform a pipeline YAML file into graph nodes and edges.
 *
 * Recursively expands sub-pipeline references up to MAX_DEPTH.
 */
export function transformPipeline(filePath: string): TransformResult {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const errors: string[] = [];
  const visited = new Set<string>();

  processFile(filePath, nodes, edges, errors, visited, undefined, 0);

  return { nodes, edges, errors };
}

function processFile(
  filePath: string,
  nodes: GraphNode[],
  edges: GraphEdge[],
  errors: string[],
  visited: Set<string>,
  parentGroupId: string | undefined,
  depth: number
): { firstNodeId: string | null; lastNodeId: string | null } {
  const absPath = path.resolve(filePath);

  if (depth > MAX_DEPTH) {
    errors.push(`Max sub-pipeline depth (${MAX_DEPTH}) exceeded at ${absPath}`);
    return { firstNodeId: null, lastNodeId: null };
  }

  if (visited.has(absPath)) {
    // Recursive reference — create a placeholder node, don't expand.
    const placeholderId = `${absPath}::recursive::${depth}`;
    nodes.push({
      id: placeholderId,
      type: 'stepNode',
      position: { x: 0, y: 0 },
      parentId: parentGroupId,
      data: {
        stepId: '(recursive)',
        type: 'pipeline',
        sourceFile: absPath,
        sourceLine: 0,
        subPipelinePath: absPath,
        pipelineName: path.basename(absPath) + ' (recursive)',
      },
    });
    return { firstNodeId: placeholderId, lastNodeId: placeholderId };
  }

  visited.add(absPath);

  let content: string;
  try {
    content = fs.readFileSync(absPath, 'utf-8');
  } catch (err) {
    errors.push(`Cannot read ${absPath}: ${err instanceof Error ? err.message : String(err)}`);
    visited.delete(absPath);
    return { firstNodeId: null, lastNodeId: null };
  }

  let parsed: RawPipelineFile;
  try {
    parsed = yamlParse(content) as RawPipelineFile;
  } catch (err) {
    errors.push(`Cannot parse ${absPath}: ${err instanceof Error ? err.message : String(err)}`);
    visited.delete(absPath);
    return { firstNodeId: null, lastNodeId: null };
  }

  const steps = parsed?.pipeline;
  if (!Array.isArray(steps) || steps.length === 0) {
    errors.push(`No pipeline steps found in ${absPath}`);
    visited.delete(absPath);
    return { firstNodeId: null, lastNodeId: null };
  }

  const baseDir = path.dirname(absPath);
  const lineMap = buildLineMap(content, steps);

  let firstNodeId: string | null = null;
  let prevNodeId: string | null = null;

  for (let i = 0; i < steps.length; i++) {
    const step = steps[i];
    const stepId = step.id ?? `step_${i}`;
    const nodeId = `${absPath}::${stepId}`;
    const stepType = classifyStep(step);
    const line = lineMap.get(stepId) ?? lineMap.get(`step_${i}`) ?? 0;

    const nodeData: StepNodeData = {
      stepId,
      type: stepType,
      sourceFile: absPath,
      sourceLine: line,
      prompt: step.prompt,
      systemPrompt: step.system_prompt,
      appendSystemPromptCount: step.append_system_prompt?.length,
      tools: step.tools ? { allow: step.tools.allow ?? [], deny: step.tools.deny ?? [] } : undefined,
      model: step.model,
      onResultCount: step.on_result?.length,
      pipelineName: parsed.meta?.name,
    };

    // If this step is a sub-pipeline reference (body is `pipeline:`), note the path.
    if (step.pipeline && !step.prompt) {
      nodeData.subPipelinePath = step.pipeline;
    }

    nodes.push({
      id: nodeId,
      type: 'stepNode',
      position: { x: 0, y: 0 },
      parentId: parentGroupId,
      data: nodeData,
    });

    if (!firstNodeId) firstNodeId = nodeId;

    // Sequential edge from previous step.
    if (prevNodeId) {
      edges.push({
        id: `${prevNodeId}->${nodeId}`,
        source: prevNodeId,
        target: nodeId,
      });
    }

    // Handle on_result branches.
    if (step.on_result && step.on_result.length > 0) {
      for (let bi = 0; bi < step.on_result.length; bi++) {
        const branch = step.on_result[bi];
        const branchLabel = describeMatcher(branch);
        const branchAction = branch.action ?? '';

        // Check if this branch calls a sub-pipeline.
        const pipelineMatch = branchAction.match(/^pipeline:\s*(.+)$/);
        if (pipelineMatch) {
          const subPath = resolveRelativePath(pipelineMatch[1].trim(), baseDir);
          const subResult = processFile(subPath, nodes, edges, errors, visited, parentGroupId, depth + 1);
          if (subResult.firstNodeId) {
            edges.push({
              id: `${nodeId}->branch_${bi}_${subResult.firstNodeId}`,
              source: nodeId,
              target: subResult.firstNodeId,
              label: branchLabel,
              conditional: true,
            });
          }
        }
        // Other branch actions (continue, break, abort, pause_for_human)
        // don't create edges to new nodes — they affect control flow but
        // don't add graph structure.
      }
      // on_result branches consumed — don't connect sequentially to next step
      // unless there are non-pipeline branches that continue.
      prevNodeId = null;
    } else if (step.pipeline && !step.prompt) {
      // Inline sub-pipeline step (not via on_result): expand it.
      const subPath = resolveRelativePath(step.pipeline, baseDir);
      const subResult = processFile(subPath, nodes, edges, errors, visited, parentGroupId, depth + 1);
      if (subResult.firstNodeId) {
        // Replace the step node with an edge into the sub-pipeline.
        edges.push({
          id: `${nodeId}->${subResult.firstNodeId}`,
          source: nodeId,
          target: subResult.firstNodeId,
        });
      }
      // The sequential chain continues from the sub-pipeline's last node.
      prevNodeId = subResult.lastNodeId ?? nodeId;
    } else {
      prevNodeId = nodeId;
    }
  }

  visited.delete(absPath);
  return { firstNodeId, lastNodeId: prevNodeId };
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function classifyStep(step: RawStep): StepNodeData['type'] {
  if (step.id === 'invocation') return 'invocation';
  if (step.pipeline && !step.prompt) return 'pipeline';
  if (step.prompt) return 'prompt';
  if (step.context) return 'context';
  if (step.action) return 'action';
  if (step.skill) return 'skill';
  return 'prompt'; // fallback
}

function describeMatcher(branch: RawOnResult): string {
  if (branch.contains) return `contains: "${branch.contains}"`;
  if (branch.exit_code !== undefined) return `exit_code: ${branch.exit_code}`;
  if (branch.always) return 'always';
  return '?';
}

function resolveRelativePath(ref: string, baseDir: string): string {
  if (ref.startsWith('/')) return ref;
  if (ref.startsWith('~/')) return path.join(process.env.HOME ?? '~', ref.slice(2));
  return path.resolve(baseDir, ref);
}

/**
 * Build a map from step id → 0-based line number by scanning for `- id:` patterns.
 * This is a best-effort heuristic (not a full YAML parser with source maps).
 */
function buildLineMap(content: string, _steps: RawStep[]): Map<string, number> {
  const map = new Map<string, number>();
  const lines = content.split('\n');
  const idPattern = /^\s*-\s*id:\s*(.+)$/;

  let stepIdx = 0;
  for (let lineNo = 0; lineNo < lines.length; lineNo++) {
    const m = lines[lineNo].match(idPattern);
    if (m) {
      const id = m[1].trim().replace(/^["']|["']$/g, '');
      map.set(id, lineNo);
      // Also map by index for steps without explicit id matching.
      map.set(`step_${stepIdx}`, lineNo);
      stepIdx++;
    }
  }
  return map;
}
