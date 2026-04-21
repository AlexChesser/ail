/**
 * HistoryService — indexes run logs via `ail logs --format json`.
 *
 * Replaces direct disk reads of ~/.ail/projects/<sha1>/runs/*.jsonl with a
 * subprocess call to `ail logs --format json [--query <text>] [--limit <n>]`.
 * The binary emits one JSON object per session (not raw NDJSON turn entries),
 * so the shape parsed here matches the `ail logs` output schema.
 *
 * Public types (TurnEntry, RunRecord, RunOutcome) and the legacy
 * parseRunFileContent() helper are defined in parseRunFile.ts and re-exported
 * from here for backward compatibility with other modules.
 */

import * as vscode from 'vscode';
import { execFile } from 'child_process';
import { promisify } from 'util';

// Re-export types and pure helper for backward compatibility.
export { TurnEntry, RunRecord, RunOutcome, parseRunFileContent } from './parseRunFile';
import type { TurnEntry, RunRecord, RunOutcome } from './parseRunFile';

const execFileAsync = promisify(execFile);

// ── Wire format from `ail logs --format json` ─────────────────────────────────

interface LogsSessionStep {
  step_id?: string;
  event_type?: string;
  response?: string;
  prompt?: string;
  cost_usd?: number;
  input_tokens?: number;
  output_tokens?: number;
  latency_ms?: number;
  runner_session_id?: string;
  stdout?: string;
  stderr?: string;
  exit_code?: string;
  thinking?: string;
}

interface LogsSession {
  run_id?: string;
  pipeline_source?: string;
  started_at?: number;
  completed_at?: number;
  total_cost_usd?: number;
  status?: string;
  steps?: LogsSessionStep[];
}

// ── ail logs wire → RunRecord ─────────────────────────────────────────────────

function sessionToRunRecord(session: LogsSession): RunRecord | null {
  const runId = session.run_id ?? '';
  if (!runId) {
    return null;
  }

  const timestamp = typeof session.started_at === 'number'
    ? session.started_at
    : 0;

  const pipelineSource = session.pipeline_source || 'unknown';

  let outcome: RunOutcome = 'unknown';
  if (session.status === 'completed') {
    outcome = 'completed';
  } else if (session.status === 'failed') {
    outcome = 'failed';
  }

  const totalCostUsd = typeof session.total_cost_usd === 'number'
    ? session.total_cost_usd
    : 0;

  const rawSteps = Array.isArray(session.steps) ? session.steps : [];
  const steps: TurnEntry[] = rawSteps.map((s) => ({
    step_id: typeof s.step_id === 'string' ? s.step_id : '',
    event_type: typeof s.event_type === 'string' ? s.event_type : null,
    prompt: typeof s.prompt === 'string' ? s.prompt : null,
    response: typeof s.response === 'string' ? s.response : null,
    cost_usd: typeof s.cost_usd === 'number' ? s.cost_usd : null,
    input_tokens: typeof s.input_tokens === 'number' ? s.input_tokens : null,
    output_tokens: typeof s.output_tokens === 'number' ? s.output_tokens : null,
    latency_ms: typeof s.latency_ms === 'number' ? s.latency_ms : null,
    runner_session_id: typeof s.runner_session_id === 'string' ? s.runner_session_id : null,
    stdout: typeof s.stdout === 'string' ? s.stdout : null,
    stderr: typeof s.stderr === 'string' ? s.stderr : null,
    exit_code: typeof s.exit_code === 'string' ? s.exit_code : null,
    thinking: typeof s.thinking === 'string' ? s.thinking : null,
  })).filter((e) => e.step_id.length > 0);

  if (outcome === 'unknown' && steps.length > 0) {
    outcome = 'completed';
  }

  if (steps.length === 0) {
    return null;
  }

  const invocationEntry = steps.find((s) => s.step_id === 'invocation');
  const invocationPrompt = invocationEntry?.prompt ?? '';

  return { runId, timestamp, pipelineSource, outcome, totalCostUsd, invocationPrompt, steps };
}

// ── HistoryService ────────────────────────────────────────────────────────────

export class HistoryService {
  private readonly _binaryPath: string;
  private readonly _cwd: string | undefined;

  constructor(_context: vscode.ExtensionContext, cwd: string | undefined, binaryPath?: string) {
    this._cwd = cwd || undefined;
    if (binaryPath !== undefined) {
      this._binaryPath = binaryPath;
    } else {
      // No binary path provided — read from workspace config or fall back to 'ail'.
      const configPath = vscode.workspace
        .getConfiguration('ail')
        .get<string>('binaryPath', '');
      this._binaryPath = configPath || 'ail';
    }
  }

  /** Return all run records sorted newest-first. */
  async getHistory(limit = 50): Promise<RunRecord[]> {
    return this._fetchLogs({ limit });
  }

  /**
   * Return the rolling average cost (in USD) of the N most recent completed
   * runs that have a positive cost. Returns 0 if there are no such runs.
   */
  async getRecentAverageCost(n: number): Promise<number> {
    const records = await this._fetchLogs({ limit: n });
    const costs = records
      .filter((r) => r.outcome === 'completed' && r.totalCostUsd > 0)
      .map((r) => r.totalCostUsd);
    if (costs.length === 0) {
      return 0;
    }
    return costs.reduce((sum, c) => sum + c, 0) / costs.length;
  }

  /** Return a single run record by runId. */
  async getRunDetail(runId: string): Promise<RunRecord | undefined> {
    // Search by session prefix — ail logs --session <prefix>
    const records = await this._fetchLogs({ session: runId, limit: 1 });
    return records.find((r) => r.runId === runId) ?? records[0];
  }

  /**
   * Search run history by free-text query.
   * Returns up to `limit` records sorted newest-first.
   */
  async searchLogs(query: string, limit = 20): Promise<RunRecord[]> {
    return this._fetchLogs({ query, limit });
  }

  // ── Private ─────────────────────────────────────────────────────────────────

  protected async _fetchLogs(opts: {
    limit?: number;
    session?: string;
    query?: string;
  }): Promise<RunRecord[]> {
    const args: string[] = ['logs', '--format', 'json'];
    if (opts.limit !== undefined) {
      args.push('--limit', String(opts.limit));
    }
    if (opts.session) {
      args.push('--session', opts.session);
    }
    if (opts.query) {
      args.push('--query', opts.query);
    }

    let stdout: string;
    try {
      const result = await execFileAsync(this._binaryPath, args, {
        timeout: 15000,
        cwd: this._cwd,
      });
      stdout = result.stdout;
    } catch {
      // Binary not available or no logs yet — return empty list.
      return [];
    }

    const records: RunRecord[] = [];
    for (const line of stdout.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed) {
        continue;
      }
      let session: LogsSession;
      try {
        session = JSON.parse(trimmed) as LogsSession;
      } catch {
        continue;
      }
      const record = sessionToRunRecord(session);
      if (record) {
        records.push(record);
      }
    }

    // Ensure newest-first ordering (the binary may already guarantee this,
    // but we sort defensively).
    records.sort((a, b) => b.timestamp - a.timestamp);
    return records;
  }
}
