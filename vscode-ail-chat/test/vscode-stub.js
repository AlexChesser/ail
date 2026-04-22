/**
 * vscode-stub — minimal CommonJS stub for the `vscode` module.
 *
 * Used by vitest via a module alias in vitest.config.ts so that
 * extension-host TypeScript that imports 'vscode' can be exercised
 * in unit tests without a running VS Code extension host.
 *
 * Only the surface area exercised by tests (and expected by Units 1–4)
 * is implemented. Return sensible defaults so tests that don't care
 * about a specific API still pass without errors.
 */

'use strict';

// ── EventEmitter ──────────────────────────────────────────────────────────────

class EventEmitter {
  constructor() {
    this._listeners = new Set();
    this._disposed = false;
    // `event` is the function callers register handlers with
    this.event = (listener) => {
      if (!this._disposed) {
        this._listeners.add(listener);
      }
      return { dispose: () => this._listeners.delete(listener) };
    };
  }

  fire(value) {
    if (this._disposed) return;
    for (const l of this._listeners) {
      l(value);
    }
  }

  dispose() {
    this._disposed = true;
    this._listeners.clear();
  }
}

// ── Uri ───────────────────────────────────────────────────────────────────────

class Uri {
  constructor({ scheme, authority, path, query, fragment }) {
    this.scheme    = scheme    ?? '';
    this.authority = authority ?? '';
    this.path      = path      ?? '';
    this.query     = query     ?? '';
    this.fragment  = fragment  ?? '';
    // fsPath is the decoded, OS-native file path
    this.fsPath = this.path;
  }

  toString() {
    let s = '';
    if (this.scheme)    s += this.scheme + ':';
    if (this.authority !== undefined) s += '//' + this.authority;
    if (this.path)      s += this.path;
    if (this.query)     s += '?' + this.query;
    if (this.fragment)  s += '#' + this.fragment;
    return s;
  }

  static parse(value) {
    const m = value.match(/^([a-z][a-z0-9+\-.]*):\/\/([^/?#]*)([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/i);
    if (m) {
      return new Uri({
        scheme:    m[1] ?? '',
        authority: m[2] ?? '',
        path:      m[3] ?? '',
        query:     m[4] ?? '',
        fragment:  m[5] ?? '',
      });
    }
    const m2 = value.match(/^([a-z][a-z0-9+\-.]*):([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/i);
    if (m2) {
      return new Uri({
        scheme:    m2[1] ?? '',
        authority: '',
        path:      m2[2] ?? '',
        query:     m2[3] ?? '',
        fragment:  m2[4] ?? '',
      });
    }
    return new Uri({ scheme: '', authority: '', path: value });
  }

  static file(path) {
    const uri = new Uri({ scheme: 'file', authority: '', path });
    uri.fsPath = path;
    return uri;
  }
}

// ── OutputChannel ─────────────────────────────────────────────────────────────

function makeOutputChannel(name) {
  return {
    name,
    appendLine: () => {},
    append: () => {},
    clear: () => {},
    show: () => {},
    hide: () => {},
    dispose: () => {},
  };
}

// ── TreeItem ──────────────────────────────────────────────────────────────────

class TreeItem {
  constructor(label, collapsibleState) {
    this.label = label;
    this.collapsibleState = collapsibleState ?? 0; // None
    this.description = undefined;
    this.tooltip = undefined;
    this.iconPath = undefined;
    this.contextValue = undefined;
    this.command = undefined;
  }
}

// ── ThemeIcon / ThemeColor ────────────────────────────────────────────────────

class ThemeIcon {
  constructor(id, color) {
    this.id = id;
    this.color = color;
  }
}

class ThemeColor {
  constructor(id) {
    this.id = id;
  }
}

// ── Range / Position ──────────────────────────────────────────────────────────

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}

class Range {
  constructor(startLineOrPosition, startCharacterOrEnd, endLine, endCharacter) {
    if (startLineOrPosition instanceof Position) {
      this.start = startLineOrPosition;
      this.end = startCharacterOrEnd;
    } else {
      this.start = new Position(startLineOrPosition, startCharacterOrEnd);
      this.end = new Position(endLine, endCharacter);
    }
  }
}

// ── RelativePattern ───────────────────────────────────────────────────────────

class RelativePattern {
  constructor(base, pattern) {
    this.base = base;
    this.pattern = pattern;
  }
}

// ── Disposable ────────────────────────────────────────────────────────────────

class Disposable {
  constructor(fn) {
    this.dispose = fn ?? (() => {});
  }
}

// ── FoldingRange ──────────────────────────────────────────────────────────────

class FoldingRange {
  constructor(start, end, kind) {
    this.start = start;
    this.end = end;
    this.kind = kind;
  }
}

// ── vscode module object ──────────────────────────────────────────────────────

const vscode = {
  EventEmitter,
  Uri,
  TreeItem,
  ThemeIcon,
  ThemeColor,
  Position,
  Range,
  RelativePattern,
  Disposable,
  FoldingRange,

  TreeItemCollapsibleState: { None: 0, Collapsed: 1, Expanded: 2 },
  StatusBarAlignment:       { Left: 1, Right: 2 },
  ViewColumn:               { One: 1, Two: 2, Three: 3, Beside: -2, Active: -1 },
  FoldingRangeKind:         { Comment: 1, Imports: 2, Region: 3 },

  window: {
    showErrorMessage:       () => Promise.resolve(undefined),
    showWarningMessage:     () => Promise.resolve(undefined),
    showInformationMessage: () => Promise.resolve(undefined),
    showQuickPick:          () => Promise.resolve(undefined),
    showOpenDialog:         () => Promise.resolve(undefined),
    showTextDocument:       () => Promise.resolve(undefined),
    activeTextEditor:       undefined,
    createOutputChannel:    (name) => makeOutputChannel(name),
    createStatusBarItem:    () => ({ text: '', show: () => {}, hide: () => {}, dispose: () => {} }),
    registerWebviewViewProvider: () => ({ dispose: () => {} }),
    createTreeView:         () => ({ dispose: () => {}, onDidChangeSelection: () => ({ dispose: () => {} }) }),
  },

  workspace: {
    getConfiguration:           () => ({ get: (_key, defaultValue) => defaultValue }),
    onDidChangeConfiguration:   () => ({ dispose: () => {} }),
    onDidSaveTextDocument:      () => ({ dispose: () => {} }),
    onDidChangeWorkspaceFolders: () => ({ dispose: () => {} }),
    openTextDocument:           () => Promise.resolve({ getText: () => '' }),
    workspaceFolders:           undefined,
    registerTextDocumentContentProvider: () => ({ dispose: () => {} }),
    createFileSystemWatcher:    () => ({
      onDidChange: () => ({ dispose: () => {} }),
      onDidCreate: () => ({ dispose: () => {} }),
      onDidDelete: () => ({ dispose: () => {} }),
      dispose: () => {},
    }),
  },

  commands: {
    registerCommand:   () => ({ dispose: () => {} }),
    executeCommand:    () => Promise.resolve(undefined),
  },

  languages: {
    registerFoldingRangeProvider:    () => ({ dispose: () => {} }),
    registerCompletionItemProvider:  () => ({ dispose: () => {} }),
    createDiagnosticCollection:      () => ({ clear: () => {}, dispose: () => {}, set: () => {} }),
  },
};

module.exports = vscode;
