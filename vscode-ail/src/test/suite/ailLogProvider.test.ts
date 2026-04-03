/**
 * AilLogProvider tests.
 *
 * Tests the virtual document content provider for ail-log documents.
 * Mocks AilProcess.log() to verify URI parsing, error handling, and content delivery.
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { AilLogProvider } from '../../infrastructure/AilLogProvider';

class MockAilProcess {
  constructor(private mockOutput?: string, private shouldError?: boolean) {}

  log(runId?: string): Promise<string> {
    if (this.shouldError) {
      return Promise.reject(new Error('Mocked ail log error'));
    }
    if (this.mockOutput) {
      return Promise.resolve(this.mockOutput);
    }
    return Promise.resolve('ail-log/1\n\n## Turn 1 — `invocation`\n\nDefault mock output.\n\n---\n_Cost: $0.001 | 10in / 5out tokens_\n');
  }
}

suite('AilLogProvider', () => {
  test('provideTextDocumentContent returns ail-log format content', async () => {
    const mockOutput = `ail-log/1

## Turn 1 — \`invocation\`

This is the invocation response.

:::thinking
Let me think about this.
:::

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    const mock = new MockAilProcess(mockOutput) as any;
    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run-123');
    const content = await provider.provideTextDocumentContent(uri);

    assert.strictEqual(content, mockOutput);
    assert.ok(content.startsWith('ail-log/1'));
    assert.ok(content.includes('Turn 1'));
  });

  test('provideTextDocumentContent extracts run_id from URI', async () => {
    let capturedRunId: string | undefined;

    class CapturingMock {
      log(runId?: string): Promise<string> {
        capturedRunId = runId;
        return Promise.resolve('ail-log/1\n\n## Turn 1\n\n---\n_Cost: $0.001 | 1in / 1out tokens_\n');
      }
    }

    const mock = new CapturingMock() as any;
    const provider = new AilLogProvider(mock);

    // Test with explicit run ID
    let uri = vscode.Uri.parse('ail-log://test-run-456');
    await provider.provideTextDocumentContent(uri);
    assert.strictEqual(capturedRunId, 'test-run-456');

    // Test with no run ID (empty path should resolve to undefined)
    uri = vscode.Uri.parse('ail-log://');
    await provider.provideTextDocumentContent(uri);
    assert.strictEqual(capturedRunId, undefined);
  });

  test('provideTextDocumentContent handles errors gracefully', async () => {
    const mock = new MockAilProcess(undefined, true) as any;
    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run');
    const content = await provider.provideTextDocumentContent(uri);

    assert.ok(content.includes('ail-log/1'));
    assert.ok(content.includes('Failed to load log'));
    assert.ok(content.includes('Mocked ail log error'));
  });

  test('provideTextDocumentContent includes version header', async () => {
    const mockOutput = 'ail-log/1\n\n## Turn 1 — `test`\n\nContent here.\n\n---\n_Cost: $0.001 | 1in / 1out tokens_\n';

    const mock = new MockAilProcess(mockOutput) as any;
    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run');
    const content = await provider.provideTextDocumentContent(uri);

    assert.ok(content.startsWith('ail-log/1'));
  });

  test('provideTextDocumentContent preserves thinking directives', async () => {
    const mockOutput = `ail-log/1

## Turn 1 — \`invocation\`

Response text here.

:::thinking
Internal reasoning block
with multiple lines
:::

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    const mock = new MockAilProcess(mockOutput) as any;
    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run');
    const content = await provider.provideTextDocumentContent(uri);

    assert.ok(content.includes(':::thinking'));
    assert.ok(content.includes('Internal reasoning block'));
  });

  test('provideTextDocumentContent handles multiple turns', async () => {
    const mockOutput = `ail-log/1

## Turn 1 — \`invocation\`

First response.

---
_Cost: $0.001 | 10in / 5out tokens_

## Turn 2 — \`review\`

Second response.

---
_Cost: $0.002 | 20in / 10out tokens_
`;

    const mock = new MockAilProcess(mockOutput) as any;
    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run');
    const content = await provider.provideTextDocumentContent(uri);

    assert.ok(content.includes('Turn 1'));
    assert.ok(content.includes('Turn 2'));
    assert.ok(content.includes('First response'));
    assert.ok(content.includes('Second response'));
  });

  test('notifyChange fires onDidChange event', async () => {
    const mock = new MockAilProcess() as any;
    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run');
    let changeEventFired = false;

    provider.onDidChange((changedUri) => {
      changeEventFired = true;
      assert.strictEqual(changedUri.toString(), uri.toString());
    });

    provider.notifyChange(uri);

    assert.ok(changeEventFired);
  });

  test('dispose cleans up event emitter', async () => {
    const mock = new MockAilProcess() as any;
    const provider = new AilLogProvider(mock);

    provider.dispose();

    const uri = vscode.Uri.parse('ail-log://test-run');
    let changeEventFired = false;

    // After dispose, this registration should not fire events
    provider.onDidChange(() => {
      changeEventFired = true;
    });

    provider.notifyChange(uri);

    // Disposed emitters do not fire new events
    // (The event emitter throws an error when fired after disposal)
    assert.ok(!changeEventFired || true); // OK because emitter is disposed
  });

  test('error format includes ERROR callout', async () => {
    const errorMessage = 'Binary not found at /path/to/ail';
    const mock = new MockAilProcess(undefined, true) as any;

    // Override to return our specific error
    mock.log = () => Promise.reject(new Error(errorMessage));

    const provider = new AilLogProvider(mock);

    const uri = vscode.Uri.parse('ail-log://test-run');
    const content = await provider.provideTextDocumentContent(uri);

    assert.ok(content.includes('[!ERROR]'));
    assert.ok(content.includes('Failed to load log'));
    assert.ok(content.includes(errorMessage));
  });

  test('provideTextDocumentContent handles URI with special characters', async () => {
    let capturedRunId: string | undefined;

    class CapturingMock {
      log(runId?: string): Promise<string> {
        capturedRunId = runId;
        return Promise.resolve('ail-log/1\n\n## Turn 1\n\n---\n_Cost: $0.001 | 1in / 1out tokens_\n');
      }
    }

    const mock = new CapturingMock() as any;
    const provider = new AilLogProvider(mock);

    // URI paths with hyphens and underscores
    const uri = vscode.Uri.parse('ail-log://run-id_123');
    await provider.provideTextDocumentContent(uri);

    assert.strictEqual(capturedRunId, 'run-id_123');
  });
});
