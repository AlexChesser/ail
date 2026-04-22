import * as vscode from 'vscode';
import { AilEvent } from './types';

export class AilOutputChannel {
  constructor(private readonly _channel: vscode.OutputChannel) {}

  spawn(binary: string, args: string[]): void {
    this._channel.appendLine(`[spawn] ${binary} ${args.join(' ')}`);
  }

  event(e: AilEvent): void {
    this._channel.appendLine(`[event] ${JSON.stringify(e)}`);
  }

  stderr(line: string): void {
    this._channel.appendLine(`[stderr] ${line}`);
  }

  exit(code: number | null): void {
    this._channel.appendLine(`[exit] code=${code}`);
  }

  error(msg: string): void {
    this._channel.appendLine(`[error] ${msg}`);
  }
}
