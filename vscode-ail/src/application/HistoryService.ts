/**
 * HistoryService — indexes run logs from ~/.ail/projects/<sha1_of_cwd>/runs/*.jsonl
 *
 * Each .jsonl file is an NDJSON stream of ail events. Lines with a `type` field
 * are `step_started` / other executor events (crash evidence). Lines without a
 * `type` field are completed `TurnEntry` records.
 *
 * The service builds a `RunRecord` index, caches it in workspaceState keyed by
 * file content hash, and only re-parses changed files.
 */

import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as crypto from 'crypto';

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
}

export type RunOutcome = 'completed' | 'failed' | 'unknown';

export interface RunRecord {
  runId: string;
  /** File modification time (ms since epoch). */
  timestamp: number;
  /** From step_started records' pipeline_source field, or 'unknown'. */
  pipelineSource: string;
  outcome: RunOutcome;
  totalCostUsd: number;
  /** The original user prompt from the invocation step. */
  invocationPrompt: string;
  steps: TurnEntry[];
}

// ── Cache shape stored in workspaceState ──────────────────────────────────────

interface CacheEntry {
  /** sha1 of the file content at read time. */
  contentHash: string;
  record: RunRecord;
}

interface HistoryCache {
  /** runId → CacheEntry */
  entries: Record<string, CacheEntry>;
}

const CACHE_KEY = 'ail.historyCache';

// ── HistoryService ────────────────────────────────────────────────────────────

export class HistoryService {
  private readonly _context: vscode.ExtensionContext;
  private readonly _runsDir: string;

  constructor(context: vscode.ExtensionContext, cwd: string) {
    this._context = context;
    const cwdHash = crypto.createHash('sha1').update(cwd).digest('hex');
    this._runsDir = path.join(os.homedir(), '.ail', 'projects', cwdHash, 'runs');
  }

  /** Return all run records sorted newest-first. */
  async getHistory(): Promise<RunRecord[]> {
    return this._loadAll();
  }

  /** Return a single run record by runId. */
  async getRunDetail(runId: string): Promise<RunRecord | undefined> {
    const all = await this._loadAll();
    return all.find((r) => r.runId === runId);
  }

  // ── Private ─────────────────────────────────────────────────────────────────

  private async _loadAll(): Promise<RunRecord[]> {
    let files: string[];
    try {
      files = fs.readdirSync(this._runsDir)
        .filter((f) => f.endsWith('.jsonl'))
        .map((f) => path.join(this._runsDir, f));
    } catch {
      // Runs directory doesn't exist yet — no history.
      return [];
    }

    const cache = this._readCache();
    const updated: Record<string, CacheEntry> = {};
    const records: RunRecord[] = [];

    for (const filePath of files) {
      const runId = path.basename(filePath, '.jsonl');
      let stat: fs.Stats;
      try {
        stat = fs.statSync(filePath);
      } catch {
        continue;
      }

      // Compute content hash for cache invalidation
      let contentHash: string;
      try {
        const content = fs.readFileSync(filePath);
        contentHash = crypto.createHash('sha1').update(content).digest('hex');
      } catch {
        continue;
      }

      // Check cache
      const cached = cache.entries[runId];
      if (cached && cached.contentHash === contentHash) {
        updated[runId] = cached;
        records.push(cached.record);
        continue;
      }

      // Parse the file
      const record = this._parseRunFile(filePath, runId, stat.mtimeMs);
      if (record) {
        updated[runId] = { contentHash, record };
        records.push(record);
      }
    }

    // Persist updated cache (drop stale entries)
    this._writeCache({ entries: updated });

    // Sort newest-first
    records.sort((a, b) => b.timestamp - a.timestamp);
    return records;
  }

  private _parseRunFile(filePath: string, runId: string, timestamp: number): RunRecord | null {
    let rawContent: string;
    try {
      rawContent = fs.readFileSync(filePath, 'utf8');
    } catch {
      return null;
    }

    const lines = rawContent.split('\n').filter((l) => l.trim().length > 0);

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
        // Executor event line (has a `type` field)
        const evType = obj['type'] as string;

        if (evType === 'step_started') {
          // Extract pipeline_source if available
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
        // TurnEntry (no `type` field)
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
        };

        if (entry.step_id) {
          steps.push(entry);
          if (entry.cost_usd != null) {
            totalCostUsd += entry.cost_usd;
          }
        }
      }
    }

    // Extract invocation prompt from the first invocation TurnEntry
    const invocationEntry = steps.find((s) => s.step_id === 'invocation');
    const invocationPrompt = invocationEntry?.prompt ?? '';

    // If outcome is still unknown but we have steps, treat it as completed
    if (outcome === 'unknown' && steps.length > 0) {
      outcome = 'completed';
    }

    return {
      runId,
      timestamp,
      pipelineSource,
      outcome,
      totalCostUsd,
      invocationPrompt,
      steps,
    };
  }

  private _readCache(): HistoryCache {
    const raw = this._context.workspaceState.get<HistoryCache>(CACHE_KEY);
    return raw ?? { entries: {} };
  }

  private _writeCache(cache: HistoryCache): void {
    void this._context.workspaceState.update(CACHE_KEY, cache);
  }
}
