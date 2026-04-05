/**
 * Tests for SessionManager.
 *
 * SessionManager depends on vscode.ExtensionContext.globalState.
 * We stub it with a plain Map so no VS Code host is needed.
 */

import { describe, it, expect, beforeEach } from 'vitest';

// ── Minimal vscode stub ──────────────────────────────────────────────────────

const store = new Map<string, unknown>();

const globalState = {
  get<T>(key: string, defaultValue: T): T {
    return (store.has(key) ? store.get(key) : defaultValue) as T;
  },
  async update(key: string, value: unknown): Promise<void> {
    store.set(key, value);
  },
  keys: (): readonly string[] => [],
  setKeysForSync: (_keys: readonly string[]): void => { /* no-op */ },
};

const fakeContext = { globalState } as unknown as import('vscode').ExtensionContext;

// ── Import under test ────────────────────────────────────────────────────────

// We need to stub the 'vscode' module before importing session-manager.
// Vitest supports vi.mock at the module level, but since session-manager only
// uses vscode.ExtensionContext as a passed-in value (not a top-level import
// that runs code), we can mock the module lazily.
import { vi } from 'vitest';

vi.mock('vscode', () => ({
  // session-manager only uses context.globalState — nothing else needed
}));

import { SessionManager } from '../src/session-manager';

// ── Tests ────────────────────────────────────────────────────────────────────

beforeEach(() => {
  store.clear();
});

describe('SessionManager', () => {
  it('getSessions returns empty array when no sessions stored', async () => {
    const mgr = new SessionManager(fakeContext);
    expect(await mgr.getSessions()).toEqual([]);
  });

  it('recordPrompt creates a session with truncated title (≤50 chars)', async () => {
    const mgr = new SessionManager(fakeContext);
    const prompt = 'Refactor the authentication module for DRY compliance';
    await mgr.recordPrompt(prompt);
    const sessions = await mgr.getSessions();
    expect(sessions).toHaveLength(1);
    expect(sessions[0].title.length).toBeLessThanOrEqual(51); // 50 + ellipsis char
    expect(sessions[0].title.endsWith('…')).toBe(true);
  });

  it('recordPrompt keeps short title unchanged', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('Fix login bug');
    const sessions = await mgr.getSessions();
    expect(sessions[0].title).toBe('Fix login bug');
  });

  it('recordPrompt only sets title from the first prompt', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('First prompt');
    await mgr.recordPrompt('Second prompt');
    const sessions = await mgr.getSessions();
    expect(sessions).toHaveLength(1);
    expect(sessions[0].title).toBe('First prompt');
  });

  it('stores sessions newest-first', async () => {
    const mgr1 = new SessionManager(fakeContext);
    await mgr1.recordPrompt('Session A');

    const mgr2 = new SessionManager(fakeContext);
    await mgr2.recordPrompt('Session B');

    const sessions = await mgr2.getSessions();
    expect(sessions[0].title).toBe('Session B');
    expect(sessions[1].title).toBe('Session A');
  });

  it('round-trips through globalState correctly', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('Round-trip test');

    // A second manager instance reading the same store should see the session.
    const mgr2 = new SessionManager(fakeContext);
    const sessions = await mgr2.getSessions();
    expect(sessions).toHaveLength(1);
    expect(sessions[0].title).toBe('Round-trip test');
    expect(sessions[0].totalCostUsd).toBe(0);
  });

  it('updateCost accumulates cost on the current session', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('Cost test');
    await mgr.updateCost(0.005);
    await mgr.updateCost(0.003);
    const sessions = await mgr.getSessions();
    expect(sessions[0].totalCostUsd).toBeCloseTo(0.008);
  });

  it('updateCost is a no-op when no session is active', async () => {
    const mgr = new SessionManager(fakeContext);
    await expect(mgr.updateCost(0.1)).resolves.toBeUndefined();
    expect(await mgr.getSessions()).toHaveLength(0);
  });

  it('newSession resets current session so next prompt creates a new one', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('First');
    mgr.newSession();
    await mgr.recordPrompt('Second');
    const sessions = await mgr.getSessions();
    expect(sessions).toHaveLength(2);
  });

  it('switchSession returns null for unknown id', async () => {
    const mgr = new SessionManager(fakeContext);
    const result = await mgr.switchSession('non-existent');
    expect(result).toBeNull();
  });

  it('switchSession returns the matching session', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('My session');
    const sessions = await mgr.getSessions();
    const id = sessions[0].id;

    const result = await mgr.switchSession(id);
    expect(result?.title).toBe('My session');
  });

  it('switchSession resets current session so next prompt is a new session', async () => {
    const mgr = new SessionManager(fakeContext);
    await mgr.recordPrompt('Original');
    const sessions = await mgr.getSessions();
    await mgr.switchSession(sessions[0].id);
    await mgr.recordPrompt('After switch');
    const updated = await mgr.getSessions();
    expect(updated).toHaveLength(2);
  });

  it('caps stored sessions at 50', async () => {
    const mgr = new SessionManager(fakeContext);
    for (let i = 0; i < 55; i++) {
      const m = new SessionManager(fakeContext);
      await m.recordPrompt(`Session ${i}`);
    }
    const sessions = await new SessionManager(fakeContext).getSessions();
    expect(sessions.length).toBeLessThanOrEqual(50);
  });
});
