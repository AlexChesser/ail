/**
 * Tests for AilLogStream — subprocess lifecycle and line-by-line streaming.
 *
 * These tests mock child_process.spawn to verify:
 * - Stream start/stop lifecycle
 * - Line-by-line output reading
 * - Process exit handling (code 0, code 1+, error)
 * - Cleanup via dispose()
 */

import * as assert from 'assert';
import * as sinon from 'sinon';
import { spawn } from 'child_process';
import { EventEmitter } from 'events';
import { AilLogStream } from '../../infrastructure/AilLogStream';

// Mock stream interface mimicking Node.js readable stream
class MockReadableStream extends EventEmitter {
  resume(): void {
    // no-op
  }
}

suite('AilLogStream', () => {
  let spawnStub: sinon.SinonStub;

  setup(() => {
    // Stub child_process.spawn to return mock process
    spawnStub = sinon.stub();
  });

  teardown(() => {
    spawnStub.restore();
  });

  test('start() spawns ail log --follow with correct arguments', async () => {
    // Create mock process
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new MockReadableStream();
    mockProcess.stderr = new MockReadableStream();
    mockProcess.kill = sinon.stub();

    // Mock spawn to return the mock process
    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    // Start the stream (non-blocking via async)
    const promise = stream.start();

    // Verify spawn was called with correct args
    await new Promise((resolve) => {
      // Give the promise time to set up the listeners
      setImmediate(() => {
        assert.strictEqual(spawnStub.callCount, 1);
        const call = spawnStub.firstCall;
        assert.deepStrictEqual(call.args[0], '/path/to/ail');
        assert.deepStrictEqual(call.args[1], ['log', '--follow', 'test-run-id', '--format', 'markdown']);
        resolve(undefined);
      });
    });

    // Clean up
    mockProcess.emit('close', 0);
    await promise;
  });

  test('start() reads lines from stdout via callback', async () => {
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new EventEmitter();
    mockProcess.stderr = new MockReadableStream();
    mockProcess.kill = sinon.stub();

    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    const promise = stream.start();

    // Wait for readline to be set up, then emit fake lines
    await new Promise((resolve) => {
      setImmediate(() => {
        // Emit 'line' events (as readline.Interface would)
        (mockProcess.stdout as any).emit('line', 'ail-log/1');
        (mockProcess.stdout as any).emit('line', '');
        (mockProcess.stdout as any).emit('line', '## Turn 1 — `invocation`');
        (mockProcess.stdout as any).emit('line', 'Response text');

        // Wait a bit for callbacks to fire
        setImmediate(() => {
          assert.strictEqual(onNewLine.callCount, 4);
          assert.strictEqual(onNewLine.firstCall.args[0], 'ail-log/1');
          assert.strictEqual(onNewLine.secondCall.args[0], '');
          assert.strictEqual(onNewLine.thirdCall.args[0], '## Turn 1 — `invocation`');
          assert.strictEqual(onNewLine.lastCall.args[0], 'Response text');

          // Emit close to resolve the promise
          mockProcess.emit('close', 0);
          resolve(undefined);
        });
      });
    });

    await promise;
  });

  test('start() resolves when process exits with code 0', async () => {
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new MockReadableStream();
    mockProcess.stderr = new MockReadableStream();
    mockProcess.kill = sinon.stub();

    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    const promise = stream.start();

    // Emit close with code 0
    setImmediate(() => {
      mockProcess.emit('close', 0);
    });

    // Should resolve without error
    await assert.doesNotReject(() => promise);
  });

  test('start() rejects when process exits with code 1', async () => {
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new MockReadableStream();
    mockProcess.stderr = new MockReadableStream();
    mockProcess.kill = sinon.stub();

    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    const promise = stream.start();

    // Emit close with code 1 (error)
    setImmediate(() => {
      mockProcess.emit('close', 1);
    });

    // Should reject with error message
    await assert.rejects(
      () => promise,
      /exited with code 1/
    );
  });

  test('start() rejects on spawn error', async () => {
    const mockProcess = new EventEmitter() as any;
    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    const promise = stream.start();

    // Emit spawn error
    setImmediate(() => {
      mockProcess.emit('error', new Error('spawn failed'));
    });

    // Should reject with error message
    await assert.rejects(
      () => promise,
      /spawn failed/
    );
  });

  test('dispose() kills the process', () => {
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new MockReadableStream();
    mockProcess.stderr = new MockReadableStream();
    const killStub = sinon.stub();
    mockProcess.kill = killStub;
    mockProcess.once = sinon.stub().returnsThis();

    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    // Start but don't await
    void stream.start();

    // Give it time to set up
    setImmediate(() => {
      stream.dispose();

      // Verify kill was called with SIGTERM
      assert(killStub.called, 'process.kill should have been called');
      assert.strictEqual(killStub.firstCall.args[0], 'SIGTERM');
    });
  });

  test('dispose() is safe to call multiple times', () => {
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new MockReadableStream();
    mockProcess.stderr = new MockReadableStream();
    mockProcess.kill = sinon.stub();
    mockProcess.once = sinon.stub().returnsThis();

    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine);

    // Start but don't await
    void stream.start();

    setImmediate(() => {
      // Call dispose multiple times
      stream.dispose();
      stream.dispose();
      stream.dispose();

      // Should not throw
      assert(true, 'dispose() should be idempotent');
    });
  });

  test('respects cwd option when spawning', async () => {
    const mockProcess = new EventEmitter() as any;
    mockProcess.stdout = new MockReadableStream();
    mockProcess.stderr = new MockReadableStream();
    mockProcess.kill = sinon.stub();

    spawnStub.returns(mockProcess);

    const onNewLine = sinon.spy();
    const stream = new AilLogStream('test-run-id', '/path/to/ail', onNewLine, '/some/cwd');

    const promise = stream.start();

    setImmediate(() => {
      const call = spawnStub.firstCall;
      const options = call.args[2];
      assert.strictEqual(options.cwd, '/some/cwd');

      mockProcess.emit('close', 0);
    });

    await promise;
  });
});
