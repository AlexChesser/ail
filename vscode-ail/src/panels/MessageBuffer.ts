/**
 * MessageBuffer — queues messages until the webview signals it is ready.
 *
 * The VS Code webview API delivers postMessage calls to the webview script,
 * but only after the script has executed and registered its message listener.
 * Setting `.html` is asynchronous from the script's perspective, so messages
 * posted immediately after panel creation are silently dropped.
 *
 * Usage:
 *   const buf = new MessageBuffer((msg) => panel.webview.postMessage(msg));
 *   // From host: call buf.post() instead of postMessage() directly.
 *   // From webview script: call vscode.postMessage({ type: 'ready' }) at end of <script>.
 *   // In onDidReceiveMessage: if msg.type === 'ready', call buf.markReady().
 */

export class MessageBuffer {
  private _ready = false;
  private _queue: object[] = [];
  private readonly _send: (msg: object) => void;

  constructor(send: (msg: object) => void) {
    this._send = send;
  }

  /** Post a message. Queued until markReady() is called; sent immediately after. */
  post(msg: object): void {
    if (this._ready) {
      this._send(msg);
    } else {
      this._queue.push(msg);
    }
  }

  /** Signal that the webview is ready. Flushes all queued messages in order. */
  markReady(): void {
    if (this._ready) return;
    this._ready = true;
    for (const msg of this._queue) {
      this._send(msg);
    }
    this._queue = [];
  }

  get ready(): boolean {
    return this._ready;
  }
}
