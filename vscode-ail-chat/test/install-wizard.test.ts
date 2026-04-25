import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Hoisted mocks (available before vi.mock factories run) ────────────────────

const { mockShowQuickPick, mockExecuteCommand, mockShowErrorMessage, workspaceStore } = vi.hoisted(() => {
  const workspaceStore = new Map<string, unknown>();
  return {
    workspaceStore,
    mockShowQuickPick: vi.fn(),
    mockExecuteCommand: vi.fn(() => Promise.resolve(undefined)),
    mockShowErrorMessage: vi.fn(() => Promise.resolve(undefined)),
  };
});

const { mockExistsSync, mockStatSync, mockReaddirSync } = vi.hoisted(() => ({
  mockExistsSync: vi.fn(),
  mockStatSync: vi.fn(),
  mockReaddirSync: vi.fn(),
}));

const { mockExecFile } = vi.hoisted(() => ({
  mockExecFile: vi.fn((_cmd: string, _args: string[], _opts: unknown, cb: (err: null, result: { stdout: string; stderr: string }) => void) =>
    cb(null, { stdout: '', stderr: '' })
  ),
}));

const { mockResolveBinary } = vi.hoisted(() => ({
  mockResolveBinary: vi.fn(() => Promise.resolve({ path: '/usr/local/bin/ail' })),
}));

// ── fs mock ──────────────────────────────────────────────────────────────────

vi.mock('fs', () => ({
  existsSync: mockExistsSync,
  statSync: mockStatSync,
  readdirSync: mockReaddirSync,
}));

// ── child_process mock ────────────────────────────────────────────────────────

vi.mock('child_process', () => ({
  execFile: mockExecFile,
}));

// ── binary mock ───────────────────────────────────────────────────────────────

vi.mock('../src/binary', () => ({
  resolveBinary: mockResolveBinary,
}));

// ── vscode mock ───────────────────────────────────────────────────────────────

vi.mock('vscode', () => ({
  window: {
    showQuickPick: mockShowQuickPick,
    showErrorMessage: mockShowErrorMessage,
  },
  workspace: {
    workspaceFolders: [{ uri: { fsPath: '/workspace' } }],
  },
  commands: {
    executeCommand: mockExecuteCommand,
  },
  Uri: {
    file: (p: string) => ({ fsPath: p }),
  },
}));

// ── Import under test ─────────────────────────────────────────────────────────

import { checkAndOfferInstall, runInstallWizard } from '../src/install-wizard';

// ── Helpers ───────────────────────────────────────────────────────────────────

const mockWorkspaceState = {
  get: (key: string) => workspaceStore.get(key),
  update: async (key: string, value: unknown) => { workspaceStore.set(key, value); },
  keys: () => [] as readonly string[],
  setKeysForSync: () => {},
};

function makeContext() {
  return {
    workspaceState: mockWorkspaceState,
    extensionPath: '/ext',
  } as unknown as import('vscode').ExtensionContext;
}

function makeChatProvider() {
  return { reloadPipeline: vi.fn() } as unknown as import('../src/chat-view-provider').ChatViewProvider;
}

function pickTemplate(label: string) {
  mockShowQuickPick.mockImplementation((items: Array<{ label: string; templateName: string }>) =>
    Promise.resolve(items.find((i) => i.label.includes(label)))
  );
}

beforeEach(() => {
  workspaceStore.clear();
  vi.clearAllMocks();
  mockExistsSync.mockReturnValue(false);
  mockStatSync.mockReturnValue({ isDirectory: () => false });
  mockReaddirSync.mockReturnValue([]);
  mockShowQuickPick.mockResolvedValue(undefined);
  mockExecFile.mockImplementation((_cmd: string, _args: string[], _opts: unknown, cb: (err: null, result: { stdout: string; stderr: string }) => void) =>
    cb(null, { stdout: '', stderr: '' })
  );
  mockResolveBinary.mockResolvedValue({ path: '/usr/local/bin/ail' });
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('checkAndOfferInstall', () => {
  describe('early-exit conditions', () => {
    it('does not show QuickPick when dismiss flag is set', async () => {
      workspaceStore.set('ail-chat.installPromptDismissed', true);
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockShowQuickPick).not.toHaveBeenCalled();
    });

    it('does not show QuickPick when .ail.yaml exists', async () => {
      mockExistsSync.mockImplementation((p: unknown) => p === '/workspace/.ail.yaml');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockShowQuickPick).not.toHaveBeenCalled();
    });

    it('does not show QuickPick when .ail.yml exists', async () => {
      mockExistsSync.mockImplementation((p: unknown) => p === '/workspace/.ail.yml');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockShowQuickPick).not.toHaveBeenCalled();
    });

    it('does not show QuickPick when .ail/ dir contains a yaml file', async () => {
      mockExistsSync.mockImplementation((p: unknown) => p === '/workspace/.ail');
      mockStatSync.mockReturnValue({ isDirectory: () => true });
      mockReaddirSync.mockReturnValue(['default.yaml']);
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockShowQuickPick).not.toHaveBeenCalled();
    });

    it('shows QuickPick when no pipeline files exist', async () => {
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockShowQuickPick).toHaveBeenCalledOnce();
    });
  });

  describe('dismiss semantics', () => {
    it('sets dismiss flag when Dismiss item is picked', async () => {
      pickTemplate('Dismiss');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(workspaceStore.get('ail-chat.installPromptDismissed')).toBe(true);
    });

    it('does NOT set dismiss flag when user escapes (undefined result)', async () => {
      mockShowQuickPick.mockResolvedValue(undefined);
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(workspaceStore.has('ail-chat.installPromptDismissed')).toBe(false);
    });

    it('does not call reloadPipeline when Dismiss is picked', async () => {
      pickTemplate('Dismiss');
      const provider = makeChatProvider();
      await checkAndOfferInstall(makeContext(), provider);
      expect(provider.reloadPipeline).not.toHaveBeenCalled();
    });
  });

  describe('template installation via ail init', () => {
    it('spawns ail init with the correct template name for Starter', async () => {
      pickTemplate('Starter');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockExecFile).toHaveBeenCalledWith(
        '/usr/local/bin/ail',
        ['init', 'starter'],
        expect.objectContaining({ cwd: '/workspace' }),
        expect.any(Function)
      );
    });

    it('spawns ail init with oh-my-ail for Oh My AIL', async () => {
      pickTemplate('Oh My AIL');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockExecFile).toHaveBeenCalledWith(
        '/usr/local/bin/ail',
        ['init', 'oh-my-ail'],
        expect.any(Object),
        expect.any(Function)
      );
    });

    it('spawns ail init with superpowers for Superpowers', async () => {
      pickTemplate('Superpowers');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockExecFile).toHaveBeenCalledWith(
        '/usr/local/bin/ail',
        ['init', 'superpowers'],
        expect.any(Object),
        expect.any(Function)
      );
    });

    it('calls reloadPipeline after successful ail init', async () => {
      pickTemplate('Starter');
      const provider = makeChatProvider();
      await checkAndOfferInstall(makeContext(), provider);
      expect(provider.reloadPipeline).toHaveBeenCalledOnce();
    });

    it('opens README in markdown preview when README exists after install', async () => {
      pickTemplate('Starter');
      mockExistsSync.mockImplementation((p: unknown) =>
        typeof p === 'string' && p === '/workspace/.ail/README.md'
      );
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockExecuteCommand).toHaveBeenCalledWith('markdown.showPreview', expect.anything());
    });

    it('shows error message and does not reload when ail init fails', async () => {
      pickTemplate('Starter');
      const err = Object.assign(new Error('conflict'), { stderr: 'already exist' });
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      mockExecFile.mockImplementation((...args: any[]) => args[args.length - 1](err));
      const provider = makeChatProvider();
      await checkAndOfferInstall(makeContext(), provider);
      expect(mockShowErrorMessage).toHaveBeenCalledWith(expect.stringContaining('already exist'));
      expect(provider.reloadPipeline).not.toHaveBeenCalled();
    });

    it('does not call ail init when resolveBinary throws', async () => {
      pickTemplate('Starter');
      mockResolveBinary.mockRejectedValue(new Error('binary not found'));
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockExecFile).not.toHaveBeenCalled();
    });
  });
});

describe('runInstallWizard', () => {
  it('bypasses dismiss flag when bypassDismiss is true', async () => {
    workspaceStore.set('ail-chat.installPromptDismissed', true);
    await runInstallWizard(makeContext(), makeChatProvider(), { bypassDismiss: true });
    expect(mockShowQuickPick).toHaveBeenCalledOnce();
  });

  it('respects dismiss flag when bypassDismiss is not set', async () => {
    workspaceStore.set('ail-chat.installPromptDismissed', true);
    await runInstallWizard(makeContext(), makeChatProvider());
    expect(mockShowQuickPick).not.toHaveBeenCalled();
  });
});
