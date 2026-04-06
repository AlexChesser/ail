/**
 * graphTransform — converts a pipeline YAML file (and its sub-pipelines)
 * into React Flow nodes and edges for the pipeline graph visualizer.
 *
 * Pure function, no VS Code dependency. Fully unit-testable.
 */

import * as fs from 'fs';
import * as path from 'path';

// ── Public types (shared with webview via postMessage) ──────────────────────
// Canonical definitions live in webview-graph/types.ts (browser side).
// We re-declare them here to avoid cross-bundle imports. Keep in sync.

export interface OnResultBranch {
  matcher: string;
  action: string;
  prompt?: string;
}

export interface AppendSystemPromptEntry {
  type: 'text' | 'file' | 'shell';
  value: string;
}

export interface StepNodeData {
  stepId: string;
  type: 'prompt' | 'context' | 'pipeline' | 'action' | 'skill' | 'invocation';
  pipelineName?: string;
  sourceFile: string;
  sourceLine: number;
  prompt?: string;
  promptIsFile?: boolean;
  systemPrompt?: string;
  systemPromptIsFile?: boolean;
  appendSystemPromptCount?: number;
  appendSystemPromptEntries?: AppendSystemPromptEntry[];
  tools?: { allow: string[]; deny: string[] };
  model?: string;
  onResultCount?: number;
  onResultBranches?: OnResultBranch[];
  subPipelinePath?: string;
  isSubPipelineGroup?: boolean;
  branchLabel?: string;
  childStepCount?: number;
  shellCommand?: string;
  actionKind?: string;
  condition?: string;
  resume?: boolean;
  message?: string;
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

// ── YAML parsing ────────────────────────────────────────────────────────────

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
  append_system_prompt?: RawAppendEntry[];
  tools?: { allow?: string[]; deny?: string[] };
  model?: string;
  on_result?: RawOnResult[];
  message?: string;
  condition?: string;
  resume?: boolean;
}

type RawAppendEntry = string | { text?: string; file?: string; shell?: string };

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

/** Tracks already-expanded sub-pipeline files for deduplication. */
interface SubPipelineRef {
  groupNodeId: string;
  firstNodeId: string;
  lastNodeId: string;
}

/**
 * Transform a pipeline YAML file into graph nodes and edges.
 *
 * Recursively expands sub-pipeline references up to MAX_DEPTH.
 * Deduplicates: if multiple branches reference the same sub-pipeline file,
 * only one group is created and all edges point to it.
 */
export function transformPipeline(filePath: string): TransformResult {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const errors: string[] = [];
  const visited = new Set<string>();
  const subPipelineCache = new Map<string, SubPipelineRef>();

  processFile(filePath, nodes, edges, errors, visited, subPipelineCache, undefined, 0);

  return { nodes, edges, errors };
}

function processFile(
  filePath: string,
  nodes: GraphNode[],
  edges: GraphEdge[],
  errors: string[],
  visited: Set<string>,
  subPipelineCache: Map<string, SubPipelineRef>,
  parentGroupId: string | undefined,
  depth: number
): { firstNodeId: string | null; lastNodeId: string | null; groupNodeId: string | null } {
  const absPath = path.resolve(filePath);

  if (depth > MAX_DEPTH) {
    errors.push(`Max sub-pipeline depth (${MAX_DEPTH}) exceeded at ${absPath}`);
    return { firstNodeId: null, lastNodeId: null, groupNodeId: null };
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
    return { firstNodeId: placeholderId, lastNodeId: placeholderId, groupNodeId: null };
  }

  visited.add(absPath);

  let content: string;
  try {
    content = fs.readFileSync(absPath, 'utf-8');
  } catch (err) {
    errors.push(`Cannot read ${absPath}: ${err instanceof Error ? err.message : String(err)}`);
    visited.delete(absPath);
    return { firstNodeId: null, lastNodeId: null, groupNodeId: null };
  }

  let parsed: RawPipelineFile;
  try {
    parsed = yamlParse(content) as RawPipelineFile;
  } catch (err) {
    errors.push(`Cannot parse ${absPath}: ${err instanceof Error ? err.message : String(err)}`);
    visited.delete(absPath);
    return { firstNodeId: null, lastNodeId: null, groupNodeId: null };
  }

  const steps = parsed?.pipeline;
  if (!Array.isArray(steps) || steps.length === 0) {
    errors.push(`No pipeline steps found in ${absPath}`);
    visited.delete(absPath);
    return { firstNodeId: null, lastNodeId: null, groupNodeId: null };
  }

  const baseDir = path.dirname(absPath);
  const lineMap = buildLineMap(content);
  const pipelineName = parsed.meta?.name ?? path.basename(absPath, path.extname(absPath));

  // Create a group node for sub-pipelines (depth > 0).
  let groupNodeId: string | null = null;
  if (depth > 0) {
    groupNodeId = `${absPath}::group`;
    nodes.push({
      id: groupNodeId,
      type: 'subPipelineGroup',
      position: { x: 0, y: 0 },
      parentId: parentGroupId,
      data: {
        stepId: pipelineName,
        type: 'pipeline',
        sourceFile: absPath,
        sourceLine: 0,
        pipelineName,
        isSubPipelineGroup: true,
        childStepCount: steps.length,
        subPipelinePath: absPath,
      },
    });
  }

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
      promptIsFile: isFilePath(step.prompt),
      systemPrompt: step.system_prompt,
      systemPromptIsFile: isFilePath(step.system_prompt),
      appendSystemPromptCount: step.append_system_prompt?.length,
      appendSystemPromptEntries: parseAppendEntries(step.append_system_prompt),
      tools: step.tools ? { allow: step.tools.allow ?? [], deny: step.tools.deny ?? [] } : undefined,
      model: step.model,
      onResultCount: step.on_result?.length,
      onResultBranches: step.on_result?.map((b) => ({
        matcher: describeMatcher(b),
        action: b.action ?? 'continue',
        prompt: b.prompt,
      })),
      pipelineName,
      shellCommand: extractShellCommand(step.context),
      actionKind: step.action,
      condition: step.condition,
      resume: step.resume,
      message: step.message,
    };

    // If this step is a sub-pipeline reference (body is `pipeline:`), note the path.
    if (step.pipeline && !step.prompt) {
      nodeData.subPipelinePath = step.pipeline;
    }

    nodes.push({
      id: nodeId,
      type: 'stepNode',
      position: { x: 0, y: 0 },
      parentId: groupNodeId ?? parentGroupId,
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
          const subAbsPath = path.resolve(subPath);

          // Deduplication: reuse already-expanded sub-pipeline.
          const cached = subPipelineCache.get(subAbsPath);
          if (cached) {
            edges.push({
              id: `${nodeId}->branch_${bi}_${cached.groupNodeId ?? cached.firstNodeId}`,
              source: nodeId,
              target: cached.groupNodeId ?? cached.firstNodeId,
              label: branchLabel,
              conditional: true,
            });
          } else {
            const subResult = processFile(subPath, nodes, edges, errors, visited, subPipelineCache, parentGroupId, depth + 1);
            if (subResult.firstNodeId) {
              const target = subResult.groupNodeId ?? subResult.firstNodeId;
              edges.push({
                id: `${nodeId}->branch_${bi}_${target}`,
                source: nodeId,
                target,
                label: branchLabel,
                conditional: true,
              });
              subPipelineCache.set(subAbsPath, {
                groupNodeId: subResult.groupNodeId ?? subResult.firstNodeId,
                firstNodeId: subResult.firstNodeId,
                lastNodeId: subResult.lastNodeId ?? subResult.firstNodeId,
              });
            }
          }
        }
        // Other branch actions (continue, break, abort, pause_for_human)
        // don't create edges to new nodes — they affect control flow but
        // don't add graph structure.
      }
      // on_result branches consumed — don't connect sequentially to next step.
      prevNodeId = null;
    } else if (step.pipeline && !step.prompt) {
      // Inline sub-pipeline step (not via on_result): expand it.
      const subPath = resolveRelativePath(step.pipeline, baseDir);
      const subAbsPath = path.resolve(subPath);

      const cached = subPipelineCache.get(subAbsPath);
      if (cached) {
        edges.push({
          id: `${nodeId}->${cached.groupNodeId ?? cached.firstNodeId}`,
          source: nodeId,
          target: cached.groupNodeId ?? cached.firstNodeId,
        });
        prevNodeId = cached.lastNodeId;
      } else {
        const subResult = processFile(subPath, nodes, edges, errors, visited, subPipelineCache, groupNodeId ?? parentGroupId, depth + 1);
        if (subResult.firstNodeId) {
          const target = subResult.groupNodeId ?? subResult.firstNodeId;
          edges.push({
            id: `${nodeId}->${target}`,
            source: nodeId,
            target,
          });
          subPipelineCache.set(subAbsPath, {
            groupNodeId: subResult.groupNodeId ?? subResult.firstNodeId,
            firstNodeId: subResult.firstNodeId,
            lastNodeId: subResult.lastNodeId ?? subResult.firstNodeId,
          });
        }
        prevNodeId = subResult.lastNodeId ?? nodeId;
      }
    } else {
      prevNodeId = nodeId;
    }
  }

  visited.delete(absPath);
  return { firstNodeId, lastNodeId: prevNodeId, groupNodeId };
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
  if (branch.contains) return branch.contains;
  if (branch.exit_code !== undefined) return `exit_code: ${branch.exit_code}`;
  if (branch.always) return 'fallback';
  return '?';
}

function resolveRelativePath(ref: string, baseDir: string): string {
  if (ref.startsWith('/')) return ref;
  if (ref.startsWith('~/')) return path.join(process.env.HOME ?? '~', ref.slice(2));
  return path.resolve(baseDir, ref);
}

function isFilePath(value: string | undefined): boolean {
  if (!value) return false;
  return /^(\.\/|\.\.\/|~\/|\/)/.test(value.trim());
}

function extractShellCommand(context: RawStep['context']): string | undefined {
  if (!context) return undefined;
  if (typeof context === 'string') return context;
  return context.shell;
}

function parseAppendEntries(entries: RawAppendEntry[] | undefined): AppendSystemPromptEntry[] | undefined {
  if (!entries || entries.length === 0) return undefined;
  return entries.map((entry) => {
    if (typeof entry === 'string') {
      return { type: 'text' as const, value: entry };
    }
    if (entry.shell) return { type: 'shell' as const, value: entry.shell };
    if (entry.file) return { type: 'file' as const, value: entry.file };
    if (entry.text) return { type: 'text' as const, value: entry.text };
    return { type: 'text' as const, value: JSON.stringify(entry) };
  });
}

/**
 * Build a map from step id → 0-based line number by scanning for `- id:` patterns.
 */
function buildLineMap(content: string): Map<string, number> {
  const map = new Map<string, number>();
  const lines = content.split('\n');
  const idPattern = /^\s*-\s*id:\s*(.+)$/;

  let stepIdx = 0;
  for (let lineNo = 0; lineNo < lines.length; lineNo++) {
    const m = lines[lineNo].match(idPattern);
    if (m) {
      const id = m[1].trim().replace(/^["']|["']$/g, '');
      map.set(id, lineNo);
      map.set(`step_${stepIdx}`, lineNo);
      stepIdx++;
    }
  }
  return map;
}
