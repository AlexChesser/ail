/**
 * vscode-stub — minimal stub for the `vscode` module.
 *
 * Intercepts Module._resolveFilename so that `require('vscode')` is
 * redirected to this file before Node attempts a disk lookup. Must be
 * loaded via `--require` (mocharc) before any test or module under test
 * that imports `vscode`.
 *
 * Only the surface area actually exercised by unit tests is implemented.
 */

'use strict';

const Module = require('module');

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

  /**
   * Parse a URI string into a Uri object, mimicking vscode.Uri.parse().
   * Uses regex decomposition to work with any URI scheme.
   *
   * RFC 3986 grammar:
   *   URI = scheme ":" hier-part [ "?" query ] [ "#" fragment ]
   *   hier-part = "//" authority path-abempty / ...
   */
  static parse(value) {
    // scheme://authority/path?query#fragment
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
    // Fallback: no authority component
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
    return new Uri({ scheme: 'file', authority: '', path });
  }
}

// ── vscode module object ──────────────────────────────────────────────────────

const vscode = {
  EventEmitter,
  Uri,
  window: {
    showErrorMessage:       () => Promise.resolve(undefined),
    showWarningMessage:     () => Promise.resolve(undefined),
    showInformationMessage: () => Promise.resolve(undefined),
    createStatusBarItem:    () => ({ text: '', show: () => {}, hide: () => {}, dispose: () => {} }),
  },
  workspace: {
    getConfiguration:           () => ({ get: () => undefined }),
    onDidChangeConfiguration:   () => ({ dispose: () => {} }),
    onDidSaveTextDocument:      () => ({ dispose: () => {} }),
    workspaceFolders:           undefined,
    registerTextDocumentContentProvider: () => ({ dispose: () => {} }),
  },
  commands: {
    registerCommand:   () => ({ dispose: () => {} }),
    executeCommand:    () => Promise.resolve(undefined),
  },
  languages: {
    registerFoldingRangeProvider: () => ({ dispose: () => {} }),
  },
  StatusBarAlignment:            { Left: 1, Right: 2 },
  TreeItemCollapsibleState:      { None: 0, Collapsed: 1, Expanded: 2 },
  ThemeIcon:   class ThemeIcon   { constructor(id)  { this.id   = id;  } },
  Disposable:  class Disposable  { constructor(fn)  { this.dispose = fn ?? (() => {}); } },
  FoldingRange: class FoldingRange { constructor(s, e, k) { this.start = s; this.end = e; this.kind = k; } },
  FoldingRangeKind: { Comment: 1, Imports: 2, Region: 3 },
  TreeItem:    class TreeItem    { constructor(label) { this.label = label; } },
  ViewColumn:  { One: 1, Two: 2, Three: 3, Beside: -2, Active: -1 },
};

// ── Module resolution intercept ───────────────────────────────────────────────

const _originalResolve = Module._resolveFilename.bind(Module);

Module._resolveFilename = function(request, parent, isMain, options) {
  if (request === 'vscode') {
    return __filename; // resolve to THIS file
  }
  return _originalResolve(request, parent, isMain, options);
};

// Register this file in the cache under the key `vscode` will resolve to.
// (Module._resolveFilename now returns __filename for 'vscode')
require.cache[__filename] = {
  id:       __filename,
  filename: __filename,
  loaded:   true,
  exports:  vscode,
  children: [],
  paths:    [],
};

module.exports = vscode;
