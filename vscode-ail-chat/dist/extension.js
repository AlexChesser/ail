"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/extension.ts
var extension_exports = {};
__export(extension_exports, {
  activate: () => activate,
  deactivate: () => deactivate
});
module.exports = __toCommonJS(extension_exports);
var vscode2 = __toESM(require("vscode"));
var path2 = __toESM(require("path"));
var fs2 = __toESM(require("fs"));

// src/binary.ts
var fs = __toESM(require("fs"));
var path = __toESM(require("path"));
var import_child_process = require("child_process");
var vscode = __toESM(require("vscode"));
var cached;
function platformTriple() {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  const platformMap = {
    darwin: "apple-darwin",
    linux: "unknown-linux-musl",
    win32: "pc-windows-msvc"
  };
  const plat = platformMap[process.platform] ?? "unknown-linux-musl";
  return `${arch}-${plat}`;
}
function bundledBinaryName() {
  const triple = platformTriple();
  return process.platform === "win32" ? `ail-${triple}.exe` : `ail-${triple}`;
}
async function getVersion(binaryPath) {
  return new Promise((resolve, reject) => {
    (0, import_child_process.execFile)(binaryPath, ["--version"], { timeout: 5e3 }, (err, stdout) => {
      if (err) {
        reject(err);
      } else {
        const version = stdout.trim().split(/\s+/).pop() ?? "unknown";
        resolve(version);
      }
    });
  });
}
function meetsMinVersion(actual, minimum) {
  const parse = (v) => v.split(".").map((n) => parseInt(n, 10) || 0);
  const [maj1, min1, pat1] = parse(actual);
  const [maj2, min2, pat2] = parse(minimum);
  if (maj1 !== maj2) return maj1 > maj2;
  if (min1 !== min2) return min1 > min2;
  return pat1 >= pat2;
}
async function resolveBinary(context) {
  var _a;
  if (cached) {
    return cached;
  }
  const config = vscode.workspace.getConfiguration("ail-chat");
  const configuredPath = config.get("binaryPath", "");
  let binaryPath;
  if (configuredPath && fs.existsSync(configuredPath)) {
    binaryPath = configuredPath;
  }
  if (!binaryPath) {
    const bundled = path.join(
      context.extensionPath,
      "dist",
      bundledBinaryName()
    );
    if (fs.existsSync(bundled)) {
      binaryPath = bundled;
      try {
        fs.chmodSync(bundled, 493);
      } catch {
      }
    }
  }
  if (!binaryPath) {
    binaryPath = process.platform === "win32" ? "ail.exe" : "ail";
  }
  let version;
  try {
    version = await getVersion(binaryPath);
  } catch (err) {
    const msg = `ail binary not found or not executable at '${binaryPath}'. Set ail-chat.binaryPath to override.`;
    void vscode.window.showErrorMessage(msg);
    throw new Error(msg);
  }
  const pkgJson = JSON.parse(
    fs.readFileSync(path.join(context.extensionPath, "package.json"), "utf-8")
  );
  const minVersion = ((_a = pkgJson.config) == null ? void 0 : _a.ailMinVersion) ?? "0.0.0";
  if (!meetsMinVersion(version, minVersion)) {
    void vscode.window.showWarningMessage(
      `ail ${version} is below the minimum required version ${minVersion}. Some features may not work correctly. Update ail or set ail.binaryPath.`
    );
  }
  cached = { path: binaryPath, version };
  return cached;
}
function clearBinaryCache() {
  cached = void 0;
}

// src/ail-process-manager.ts
var import_child_process2 = require("child_process");

// src/ndjson.ts
function parseNdjsonStream(stream, onEvent, onError) {
  let buffer = "";
  stream.setEncoding("utf-8");
  stream.on("data", (chunk) => {
    buffer += chunk;
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        const event = JSON.parse(trimmed);
        onEvent(event);
      } catch {
        console.warn(`[ail] Skipping malformed NDJSON line: ${trimmed}`);
      }
    }
  });
  stream.on("end", () => {
    const trimmed = buffer.trim();
    if (trimmed) {
      try {
        const event = JSON.parse(trimmed);
        onEvent(event);
      } catch {
        console.warn(`[ail] Skipping malformed NDJSON at EOF: ${trimmed}`);
      }
    }
    buffer = "";
  });
  stream.on("error", (err) => {
    console.error(`[ail] Stream error: ${err.message}`);
    onError == null ? void 0 : onError(err);
  });
}

// src/ail-process-manager.ts
var AilProcessManager = class {
  constructor(binaryPath, cwd) {
    this._handlers = /* @__PURE__ */ new Set();
    this._binaryPath = binaryPath;
    this._cwd = cwd;
  }
  /** Register a handler that receives HostToWebviewMessages as they arrive. */
  onMessage(handler) {
    this._handlers.add(handler);
  }
  _emit(msg) {
    for (const h of this._handlers) {
      h(msg);
    }
  }
  /** Write a JSON message to the active process stdin (for HITL responses, etc.). */
  writeStdin(message) {
    var _a;
    if (!((_a = this._activeProcess) == null ? void 0 : _a.stdin)) {
      return;
    }
    this._activeProcess.stdin.write(JSON.stringify(message) + "\n");
  }
  /** Returns true when a process is currently running. */
  get isRunning() {
    return this._activeProcess !== void 0;
  }
  /**
   * Spawn `ail --once <prompt> --pipeline <pipeline> --output-format json`.
   * Rejects immediately if a process is already running.
   */
  start(prompt, pipeline, options = {}) {
    if (this._activeProcess) {
      return Promise.reject(new Error("A pipeline is already running"));
    }
    const args = [
      "--once",
      prompt,
      "--pipeline",
      pipeline,
      "--output-format",
      "json"
    ];
    if (options.headless) {
      args.push("--headless");
    }
    return new Promise((resolve, reject) => {
      var _a;
      const env = { ...process.env };
      delete env["CLAUDECODE"];
      const proc = (0, import_child_process2.spawn)(this._binaryPath, args, { cwd: this._cwd, env });
      this._activeProcess = proc;
      parseNdjsonStream(
        proc.stdout,
        (event) => {
          const msgs = mapAilEventToMessages(event);
          for (const msg of msgs) {
            this._emit(msg);
          }
        },
        (err) => {
          console.error(`[ail-chat] NDJSON stream error: ${err.message}`);
        }
      );
      (_a = proc.stderr) == null ? void 0 : _a.resume();
      proc.on("close", (code) => {
        this._activeProcess = void 0;
        if (code !== 0 && code !== null) {
          this._emit({ type: "processError", message: `ail exited with code ${code}` });
        }
        resolve();
      });
      proc.on("error", (err) => {
        this._activeProcess = void 0;
        this._emit({ type: "processError", message: `Failed to spawn ail: ${err.message}` });
        reject(err);
      });
    });
  }
  /** Send SIGTERM, escalate to SIGKILL after 5 seconds. */
  cancel() {
    if (!this._activeProcess) {
      return;
    }
    const proc = this._activeProcess;
    proc.kill("SIGTERM");
    const timeout = setTimeout(() => {
      if (this._activeProcess === proc) {
        proc.kill("SIGKILL");
      }
    }, 5e3);
    proc.once("close", () => clearTimeout(timeout));
  }
};
function mapAilEventToMessages(event) {
  switch (event.type) {
    case "run_started":
      return [{ type: "runStarted", runId: event.run_id, totalSteps: event.total_steps }];
    case "step_started":
      return [{
        type: "stepStarted",
        stepId: event.step_id,
        stepIndex: event.step_index,
        totalSteps: event.total_steps
      }];
    case "step_completed":
      return [{
        type: "stepCompleted",
        stepId: event.step_id,
        costUsd: event.cost_usd,
        inputTokens: event.input_tokens,
        outputTokens: event.output_tokens
      }];
    case "step_skipped":
      return [{ type: "stepSkipped", stepId: event.step_id }];
    case "step_failed":
      return [{ type: "stepFailed", stepId: event.step_id, error: event.error }];
    case "hitl_gate_reached":
      return [{ type: "hitlGate", stepId: event.step_id, message: event.message }];
    case "pipeline_completed":
      return [{ type: "pipelineCompleted" }];
    case "pipeline_error":
      return [{ type: "pipelineError", error: event.error }];
    case "runner_event": {
      const inner = event.event;
      switch (inner.type) {
        case "stream_delta":
          return [{ type: "streamDelta", text: inner.text }];
        case "thinking":
          return [{ type: "thinking", text: inner.text }];
        case "tool_use":
          return [{
            type: "toolUse",
            toolName: inner.tool_name,
            toolUseId: inner.tool_use_id ?? "",
            input: inner.input
          }];
        case "tool_result":
          return [{
            type: "toolResult",
            toolUseId: inner.tool_use_id ?? "",
            content: inner.content ?? "",
            isError: inner.is_error ?? false
          }];
        case "permission_requested":
          return [{
            type: "permissionRequested",
            displayName: inner.display_name,
            displayDetail: inner.display_detail
          }];
        default:
          return [];
      }
    }
    default:
      return [];
  }
}

// src/session-manager.ts
var SESSIONS_KEY = "ail-chat.sessions";
var MAX_SESSIONS = 50;
var SessionManager = class {
  constructor(context) {
    this._currentSessionId = null;
    this._context = context;
  }
  _load() {
    return this._context.globalState.get(SESSIONS_KEY, []);
  }
  async _save(sessions) {
    await this._context.globalState.update(SESSIONS_KEY, sessions);
  }
  /** Return all sessions as summaries (newest first). */
  async getSessions() {
    return this._load().map((s) => ({
      id: s.id,
      title: s.title,
      timestamp: s.timestamp,
      totalCostUsd: s.totalCostUsd
    }));
  }
  /**
   * Record the first prompt of a new session. If this is the first prompt in
   * the current session, creates the session record. Otherwise no-op (session
   * title is already set).
   */
  async recordPrompt(prompt) {
    if (this._currentSessionId !== null) {
      return;
    }
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    this._currentSessionId = id;
    const title = prompt.length > 50 ? prompt.slice(0, 50) + "\u2026" : prompt;
    const session = {
      id,
      title,
      timestamp: Date.now(),
      totalCostUsd: 0,
      firstPrompt: prompt
    };
    const sessions = this._load();
    sessions.unshift(session);
    if (sessions.length > MAX_SESSIONS) {
      sessions.splice(MAX_SESSIONS);
    }
    await this._save(sessions);
  }
  /** Update the cumulative cost for the current session. */
  async updateCost(costDelta) {
    if (this._currentSessionId === null) return;
    const sessions = this._load();
    const idx = sessions.findIndex((s) => s.id === this._currentSessionId);
    if (idx >= 0) {
      sessions[idx].totalCostUsd += costDelta;
      await this._save(sessions);
    }
  }
  /**
   * Switch to a historical session. Returns the session if found.
   * Resets the current session ID so the next prompt starts a new session.
   */
  async switchSession(sessionId) {
    this._currentSessionId = null;
    const sessions = this._load();
    return sessions.find((s) => s.id === sessionId) ?? null;
  }
  /** Start a fresh session (called when user clicks "New Session"). */
  newSession() {
    this._currentSessionId = null;
  }
};

// src/extension.ts
var panel;
var processManager;
async function activate(context) {
  context.subscriptions.push(
    vscode2.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("ail-chat.binaryPath")) {
        clearBinaryCache();
      }
    })
  );
  const sessionManager = new SessionManager(context);
  context.subscriptions.push(
    vscode2.commands.registerCommand("ail-chat.open", async () => {
      var _a, _b;
      if (panel) {
        panel.reveal();
        return;
      }
      let binary;
      try {
        binary = await resolveBinary(context);
      } catch {
        return;
      }
      const cwd = (_b = (_a = vscode2.workspace.workspaceFolders) == null ? void 0 : _a[0]) == null ? void 0 : _b.uri.fsPath;
      processManager = new AilProcessManager(binary.path, cwd);
      panel = vscode2.window.createWebviewPanel(
        "ailChat",
        "ail Chat",
        vscode2.ViewColumn.Beside,
        {
          enableScripts: true,
          retainContextWhenHidden: true,
          localResourceRoots: [vscode2.Uri.file(path2.join(context.extensionPath, "dist"))]
        }
      );
      panel.webview.html = getWebviewHtml(panel.webview, context);
      processManager.onMessage((msg) => {
        void (panel == null ? void 0 : panel.webview.postMessage(msg));
      });
      panel.webview.onDidReceiveMessage((raw) => {
        handleWebviewMessage(raw, processManager, sessionManager, panel, cwd);
      });
      panel.onDidDispose(() => {
        processManager == null ? void 0 : processManager.cancel();
        panel = void 0;
        processManager = void 0;
      });
      context.subscriptions.push(panel);
    }),
    vscode2.commands.registerCommand("ail-chat.newSession", () => {
      void vscode2.commands.executeCommand("ail-chat.open");
    })
  );
}
function handleWebviewMessage(msg, mgr, sessions, wvPanel, cwd) {
  switch (msg.type) {
    case "ready": {
      void sessions.getSessions().then((list) => {
        void wvPanel.webview.postMessage({ type: "sessionsUpdated", sessions: list });
      });
      break;
    }
    case "submitPrompt": {
      const config = vscode2.workspace.getConfiguration("ail-chat");
      const defaultPipeline = config.get("defaultPipeline", "");
      const headless = config.get("headless", false);
      let pipeline = defaultPipeline;
      if (!pipeline && cwd) {
        const candidate = path2.join(cwd, ".ail.yaml");
        if (fs2.existsSync(candidate)) {
          pipeline = candidate;
        }
      }
      if (!pipeline) {
        void wvPanel.webview.postMessage({
          type: "processError",
          message: "No pipeline file found. Set ail-chat.defaultPipeline or add .ail.yaml to your workspace."
        });
        return;
      }
      void mgr.start(msg.text, pipeline, { headless }).catch((err) => {
        void wvPanel.webview.postMessage({ type: "processError", message: err.message });
      });
      void sessions.recordPrompt(msg.text).then(
        () => sessions.getSessions().then((list) => {
          void wvPanel.webview.postMessage({ type: "sessionsUpdated", sessions: list });
        })
      );
      break;
    }
    case "hitlResponse":
      mgr.writeStdin({ type: "hitl_response", step_id: msg.stepId, text: msg.text });
      break;
    case "permissionResponse":
      mgr.writeStdin({ type: "permission_response", allowed: msg.allowed, reason: msg.reason });
      break;
    case "killProcess":
      mgr.cancel();
      break;
    case "switchSession":
      void sessions.switchSession(msg.sessionId).then((sessionData) => {
        if (sessionData) {
          void wvPanel.webview.postMessage({ type: "sessionsUpdated", sessions: [] });
        }
      });
      break;
    case "newSession":
      break;
  }
}
function deactivate() {
  processManager == null ? void 0 : processManager.cancel();
  panel == null ? void 0 : panel.dispose();
}
function getWebviewHtml(webview, context) {
  const scriptUri = webview.asWebviewUri(
    vscode2.Uri.file(path2.join(context.extensionPath, "dist", "webview.js"))
  );
  const nonce = generateNonce();
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="
    default-src 'none';
    script-src 'nonce-${nonce}';
    style-src 'unsafe-inline';
    font-src ${webview.cspSource};
  ">
  <title>ail Chat</title>
  <style>
    body, html { margin: 0; padding: 0; height: 100vh; overflow: hidden; }
    #root { display: flex; height: 100%; }
  </style>
</head>
<body>
  <div id="root"></div>
  <script nonce="${nonce}" src="${scriptUri.toString()}"></script>
</body>
</html>`;
}
function generateNonce() {
  let text = "";
  const possible = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  activate,
  deactivate
});
//# sourceMappingURL=extension.js.map
