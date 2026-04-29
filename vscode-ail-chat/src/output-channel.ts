import * as vscode from 'vscode';
import { AilEvent } from './types';

/** ISO-8601 with millisecond precision in local time, e.g. "2026-04-29 14:32:08.421". */
function timestamp(): string {
  const d = new Date();
  const pad = (n: number, w = 2): string => String(n).padStart(w, '0');
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`
  );
}

export class AilOutputChannel {
  constructor(private readonly _channel: vscode.OutputChannel) {}

  private _log(tag: string, body: string): void {
    this._channel.appendLine(`[${timestamp()}] [${tag}] ${body}`);
  }

  spawn(binary: string, args: string[]): void {
    this._log('spawn', `${binary} ${args.join(' ')}`);
  }

  event(e: AilEvent): void {
    this._log('event', JSON.stringify(e));
  }

  stderr(line: string): void {
    this._log('stderr', line);
  }

  exit(code: number | null): void {
    this._log('exit', `code=${code}`);
  }

  error(msg: string): void {
    this._log('error', msg);
  }
}
