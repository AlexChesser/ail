import { describe, it, expect } from 'vitest';
import * as vscode from 'vscode';

describe('vscode stub', () => {
  it('provides window.showInformationMessage', () => {
    expect(typeof vscode.window.showInformationMessage).toBe('function');
  });

  it('provides workspace.getConfiguration', () => {
    expect(typeof vscode.workspace.getConfiguration).toBe('function');
  });

  it('provides TreeItem constructor', () => {
    const item = new vscode.TreeItem('test label');
    expect(item.label).toBe('test label');
  });

  it('provides Uri.file', () => {
    const uri = vscode.Uri.file('/some/path');
    expect(uri.fsPath).toBe('/some/path');
  });

  it('provides TreeItemCollapsibleState enum', () => {
    expect(vscode.TreeItemCollapsibleState.None).toBe(0);
    expect(vscode.TreeItemCollapsibleState.Collapsed).toBe(1);
    expect(vscode.TreeItemCollapsibleState.Expanded).toBe(2);
  });
});
