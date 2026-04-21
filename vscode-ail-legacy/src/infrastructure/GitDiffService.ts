/**
 * GitDiffService — captures git snapshots and computes file diffs between them.
 *
 * Used by UnifiedPanel to detect which files changed during a pipeline step,
 * enabling the per-step diff viewer (issue #23).
 *
 * Uses `execFile` (not `exec`) to avoid shell injection.
 * 5-second timeout on all git subprocesses.
 */

import { execFile } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs';

const execFileAsync = promisify(execFile);
const GIT_TIMEOUT_MS = 5000;

export interface GitSnapshot {
  commitHash: string;
  timestamp: number;
}

export interface FileDiff {
  relativePath: string;
  changeType: 'added' | 'modified' | 'deleted';
  /** The commit hash at the time the snapshot was taken (before the step ran). */
  beforeRef: string;
}

export class GitDiffService {
  /**
   * Returns true if `cwd` is inside a git work tree.
   * Returns false (never throws) if git is not installed or the directory is not a repo.
   */
  async isGitRepo(cwd: string): Promise<boolean> {
    try {
      await execFileAsync('git', ['rev-parse', '--is-inside-work-tree'], {
        cwd,
        timeout: GIT_TIMEOUT_MS,
      });
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Captures the current HEAD commit hash as a snapshot.
   * Throws if git is not available or the directory has no commits.
   */
  async captureSnapshot(cwd: string): Promise<GitSnapshot> {
    const { stdout } = await execFileAsync('git', ['rev-parse', 'HEAD'], {
      cwd,
      timeout: GIT_TIMEOUT_MS,
    });
    return {
      commitHash: stdout.trim(),
      timestamp: Date.now(),
    };
  }

  /**
   * Returns all files that changed between `before.commitHash` and now.
   *
   * Covers two cases:
   *   1. Committed changes: `git diff <before> HEAD --name-status`
   *   2. Uncommitted working-tree changes: `git diff HEAD --name-status`
   *
   * Deduplicates entries — a file that was committed _and_ has working-tree
   * changes is reported once with the highest-priority change type.
   */
  async diffBetween(cwd: string, before: GitSnapshot): Promise<FileDiff[]> {
    const results = new Map<string, FileDiff>();

    // 1. Committed changes since snapshot
    try {
      const { stdout: committedOut } = await execFileAsync(
        'git',
        ['diff', before.commitHash, 'HEAD', '--name-status'],
        { cwd, timeout: GIT_TIMEOUT_MS },
      );
      for (const diff of parseNameStatus(committedOut, before.commitHash)) {
        results.set(diff.relativePath, diff);
      }
    } catch {
      // No commits since snapshot, or HEAD moved — not an error.
    }

    // 2. Uncommitted working-tree changes
    try {
      const { stdout: wtOut } = await execFileAsync(
        'git',
        ['diff', 'HEAD', '--name-status'],
        { cwd, timeout: GIT_TIMEOUT_MS },
      );
      for (const diff of parseNameStatus(wtOut, before.commitHash)) {
        // Overwrite committed entry if the file also has working-tree changes
        results.set(diff.relativePath, diff);
      }
    } catch {
      // Working tree diff failed — not fatal.
    }

    return [...results.values()];
  }

  /**
   * Returns the content of `filePath` at git ref `ref`.
   * Returns an empty string if the file did not exist at that ref (e.g. newly added).
   */
  async getFileAtRef(cwd: string, ref: string, filePath: string): Promise<string> {
    try {
      const { stdout } = await execFileAsync(
        'git',
        ['show', `${ref}:${filePath}`],
        { cwd, timeout: GIT_TIMEOUT_MS, maxBuffer: 10 * 1024 * 1024 },
      );
      return stdout;
    } catch {
      return '';
    }
  }

  /**
   * Returns the on-disk content of `absPath`.
   * Returns an empty string if the file has been deleted.
   */
  async getFileFromDisk(absPath: string): Promise<string> {
    try {
      return await fs.promises.readFile(absPath, 'utf8');
    } catch {
      return '';
    }
  }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Parses `git diff --name-status` output into FileDiff records.
 *
 * Each line is: `<status>\t<path>` where status is M, A, D, or Rnn (rename).
 * Renames are treated as delete + add; only the new path is reported as 'modified'.
 */
function parseNameStatus(output: string, beforeRef: string): FileDiff[] {
  const diffs: FileDiff[] = [];
  for (const line of output.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    const tabIdx = trimmed.indexOf('\t');
    if (tabIdx === -1) continue;

    const statusCode = trimmed.slice(0, tabIdx);
    const rest = trimmed.slice(tabIdx + 1);

    let changeType: FileDiff['changeType'];
    let relativePath: string;

    if (statusCode === 'A') {
      changeType = 'added';
      relativePath = rest;
    } else if (statusCode === 'D') {
      changeType = 'deleted';
      relativePath = rest;
    } else if (statusCode.startsWith('R')) {
      // Renamed: "R100\told/path\tnew/path"
      const parts = rest.split('\t');
      changeType = 'modified';
      relativePath = parts[1] ?? rest;
    } else {
      // M, C, T, U — treat as modified
      changeType = 'modified';
      relativePath = rest;
    }

    if (relativePath) {
      diffs.push({ relativePath, changeType, beforeRef });
    }
  }
  return diffs;
}
