/**
 * ServiceContext — DI container for application services.
 *
 * Created once in activate() and passed to services and commands.
 * The outputChannel is owned here and registered on context.subscriptions.
 */

import * as vscode from 'vscode';
import { IAilClient } from './IAilClient';

export interface ServiceContext {
  readonly extensionContext: vscode.ExtensionContext;
  readonly outputChannel: vscode.OutputChannel;
  readonly client: IAilClient;
}

export function createServiceContext(
  extensionContext: vscode.ExtensionContext,
  client: IAilClient,
): ServiceContext {
  const outputChannel = vscode.window.createOutputChannel('ail');
  extensionContext.subscriptions.push(outputChannel);
  return { extensionContext, outputChannel, client };
}
