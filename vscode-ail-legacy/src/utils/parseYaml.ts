/**
 * Pure YAML step parsing — no VS Code API dependency.
 * Extracted so tests can import this without a VS Code runtime.
 */

import * as fs from 'fs';
import { parse } from 'yaml';

export interface ParsedStep {
  id: string;
  type: string;
  /** Zero-based line number of the step's `id:` key in the source file. */
  line: number;
}

// Step type keys that identify a step's kind.
const STEP_TYPE_KEYS = ['prompt', 'context', 'action', 'pipeline'] as const;

/**
 * Extract pipeline steps from a .ail.yaml file.
 * Returns [] for missing files, unreadable files, or pipelines with no steps.
 */
export function parseStepsFromYaml(filePath: string): ParsedStep[] {
  let content: string;
  try {
    content = fs.readFileSync(filePath, 'utf-8');
  } catch {
    return [];
  }

  let doc: unknown;
  try {
    doc = parse(content);
  } catch {
    return [];
  }

  if (
    !doc ||
    typeof doc !== 'object' ||
    !Array.isArray((doc as Record<string, unknown>)['pipeline'])
  ) {
    return [];
  }

  const rawSteps = (doc as Record<string, unknown>)['pipeline'] as unknown[];

  // Build a line-number index: for each step id, find the line of `id: <value>`.
  // We do this with a simple scan of the raw text so we don't need the YAML
  // parser's CST (which would add complexity).
  const lines = content.split('\n');
  const idLineIndex = buildIdLineIndex(lines);

  const steps: ParsedStep[] = [];
  for (const raw of rawSteps) {
    if (!raw || typeof raw !== 'object') continue;
    const step = raw as Record<string, unknown>;
    const id = typeof step['id'] === 'string' ? step['id'] : undefined;
    if (!id) continue;

    const type = STEP_TYPE_KEYS.find((k) => k in step);
    if (!type) continue;

    steps.push({ id, type, line: idLineIndex.get(id) ?? 0 });
  }

  return steps;
}

/**
 * Scan source lines for `id: <value>` entries and return a map from id → line number.
 * Only looks for bare `id:` keys (indented or not) to match step declarations.
 */
function buildIdLineIndex(lines: string[]): Map<string, number> {
  const index = new Map<string, number>();
  const idPattern = /^\s*-?\s*id:\s*(['"]?)(.+?)\1\s*$/;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(idPattern);
    if (m) {
      index.set(m[2], i);
    }
  }
  return index;
}
