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

const { mockExistsSync, mockStatSync, mockReaddirSync, mockMkdirSync, mockCopyFileSync } = vi.hoisted(() => ({
  mockExistsSync: vi.fn(),
  mockStatSync: vi.fn(),
  mockReaddirSync: vi.fn(),
  mockMkdirSync: vi.fn(),
  mockCopyFileSync: vi.fn(),
}));

// ── fs mock ──────────────────────────────────────────────────────────────────

vi.mock('fs', () => ({
  existsSync: mockExistsSync,
  statSync: mockStatSync,
  readdirSync: mockReaddirSync,
  mkdirSync: mockMkdirSync,
  copyFileSync: mockCopyFileSync,
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

import { checkAndOfferInstall } from '../src/install-wizard';

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
  mockShowQuickPick.mockImplementation((items: Array<{ label: string; dir: string }>) =>
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

  describe('template installation', () => {
    beforeEach(() => {
      mockExistsSync.mockImplementation((p: unknown) =>
        typeof p === 'string' && p.includes('dist/templates')
      );
      mockReaddirSync.mockReturnValue([]);
    });

    it('copies starter template to .ail/ directory', async () => {
      pickTemplate('Starter');
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockMkdirSync).toHaveBeenCalledWith('/workspace/.ail', expect.objectContaining({ recursive: true }));
    });

    it('calls reloadPipeline after installing a template', async () => {
      pickTemplate('Starter');
      const provider = makeChatProvider();
      await checkAndOfferInstall(makeContext(), provider);
      expect(provider.reloadPipeline).toHaveBeenCalledOnce();
    });

    it('opens README in markdown preview after install when README exists', async () => {
      pickTemplate('Starter');
      mockExistsSync.mockImplementation((p: unknown) =>
        typeof p === 'string' && (p.includes('dist/templates') || p.includes('README.md'))
      );
      await checkAndOfferInstall(makeContext(), makeChatProvider());
      expect(mockExecuteCommand).toHaveBeenCalledWith('markdown.showPreview', expect.anything());
    });

    it('does not call reloadPipeline when template source is missing', async () => {
      pickTemplate('Starter');
      mockExistsSync.mockReturnValue(false);
      const provider = makeChatProvider();
      await checkAndOfferInstall(makeContext(), provider);
      expect(provider.reloadPipeline).not.toHaveBeenCalled();
    });
  });
});
