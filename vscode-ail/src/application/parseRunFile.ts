/**
 * parseRunFileContent — pure function for parsing raw NDJSON turn log lines.
 *
 * Extracted from HistoryService so it can be imported in unit tests without
 * requiring a VS Code runtime (HistoryService.ts imports 'vscode').
 *
 * All public types used by both HistoryService and the tests are defined here.
 */

// ── Public types ──────────────────────────────────────────────────────────────

export interface TurnEntry {
  step_id: string;
  prompt: string | null;
  response: string | null;
  cost_usd: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  runner_session_id: string | null;
  stdout: string | null;
  stderr: string | null;
  exit_code: string | null;
  thinking: string | null;
}

export type RunOutcome = 'completed' | 'failed' | 'unknown';

export interface RunRecord {
  runId: string;
  /** Unix epoch milliseconds. */
  timestamp: number;
  /** From pipeline_source field, or 'unknown'. */
  pipelineSource: string;
  outcome: RunOutcome;
  totalCostUsd: number;
  /** The original user prompt from the invocation step. */
  invocationPrompt: string;
  steps: TurnEntry[];
}

// ── Pure parsing function ─────────────────────────────────────────────────────

/**
 * Parse trimmed non-empty lines from a single .jsonl run file into a RunRecord.
 * Exported as a standalone function so it can be unit-tested without
 * constructing a full HistoryService (which requires a vscode.ExtensionContext).
 *
 * @returns A RunRecord, or null if no valid TurnEntries were found.
 */
export function parseRunFileContent(
  lines: string[],
  runId: string,
  timestamp: number,
): RunRecord | null {
  const steps: TurnEntry[] = [];
  let pipelineSource = 'unknown';
  let outcome: RunOutcome = 'unknown';
  let totalCostUsd = 0;

  for (const line of lines) {
    let obj: Record<string, unknown>;
    try {
      obj = JSON.parse(line) as Record<string, unknown>;
    } catch {
      continue;
    }

    if (typeof obj['type'] === 'string') {
      const evType = obj['type'] as string;
      if (evType === 'step_started') {
        const src = obj['pipeline_source'];
        if (typeof src === 'string' && src) {
          pipelineSource = src;
        }
      } else if (evType === 'pipeline_completed') {
        outcome = 'completed';
      } else if (evType === 'pipeline_error') {
        outcome = 'failed';
      }
    } else {
      const entry: TurnEntry = {
        step_id: typeof obj['step_id'] === 'string' ? obj['step_id'] : '',
        prompt: typeof obj['prompt'] === 'string' ? obj['prompt'] : null,
        response: typeof obj['response'] === 'string' ? obj['response'] : null,
        cost_usd: typeof obj['cost_usd'] === 'number' ? obj['cost_usd'] : null,
        input_tokens: typeof obj['input_tokens'] === 'number' ? obj['input_tokens'] : null,
        output_tokens: typeof obj['output_tokens'] === 'number' ? obj['output_tokens'] : null,
        runner_session_id: typeof obj['runner_session_id'] === 'string' ? obj['runner_session_id'] : null,
        stdout: typeof obj['stdout'] === 'string' ? obj['stdout'] : null,
        stderr: typeof obj['stderr'] === 'string' ? obj['stderr'] : null,
        exit_code: typeof obj['exit_code'] === 'string' ? obj['exit_code'] : null,
        thinking: typeof obj['thinking'] === 'string' ? obj['thinking'] : null,
      };

      if (entry.step_id) {
        steps.push(entry);
        if (entry.cost_usd != null) {
          totalCostUsd += entry.cost_usd;
        }
      }
    }
  }

  const invocationEntry = steps.find((s) => s.step_id === 'invocation');
  const invocationPrompt = invocationEntry?.prompt ?? '';

  if (outcome === 'unknown' && steps.length > 0) {
    outcome = 'completed';
  }

  if (steps.length === 0) {
    return null;
  }

  return { runId, timestamp, pipelineSource, outcome, totalCostUsd, invocationPrompt, steps };
}
