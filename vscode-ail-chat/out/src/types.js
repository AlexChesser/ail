"use strict";
/**
 * Shared types for vscode-ail-chat.
 *
 * Two sections:
 *   1. AilEvent — the NDJSON wire contract emitted by `ail --output-format json`.
 *      Copied from vscode-ail/src/types.ts; both must track the same protocol.
 *      Source of truth: spec/core/s23-structured-output.md
 *
 *   2. WebviewMessage — the postMessage protocol between the extension host
 *      and the React webview panel. Typed as a discriminated union on `type`.
 */
Object.defineProperty(exports, "__esModule", { value: true });
//# sourceMappingURL=types.js.map