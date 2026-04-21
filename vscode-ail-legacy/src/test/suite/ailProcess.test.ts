/**
 * AilProcess integration tests.
 *
 * AilProcess is the only file in the extension that calls spawn() for pipeline
 * runs. These tests use a stub Node.js script (src/test/fixtures/stub-ail.js)
 * instead of the real binary to verify the full spawn → NDJSON parse →
 * event emit → resolve flow.
 *
 * No vscode module stub is needed — AilProcess has no VS Code API dependencies.
 */

import * as assert from 'assert';
import * as path from 'path';
import { AilProcess } from '../../infrastructure/AilProcess';
import { AilEvent } from '../../types';
import { RunnerEvent } from '../../application/events';

// ── Stub binary path ──────────────────────────────────────────────────────────

// __dirname in compiled output: out/src/test/suite/
// stub-ail.js is plain JS (no TS compilation), so it lives at src/test/fixtures/.
// Walk up 4 levels from out/src/test/suite/ to reach vscode-ail/, then navigate to src/.
const STUB_AIL = path.resolve(__dirname, '..', '..', '..', '..', 'src', 'test', 'fixtures', 'stub-ail.js');

function stubBinary(mode?: string): string {
  // Invoke stub via node so it works cross-platform without chmod +x
  // AilProcess calls spawn(binaryPath, args), so we use a wrapper approach:
  // set binaryPath to 'node' and pass stub path + mode as leading args via env.
  // Actually simpler: just use 'node' as the binary with the stub path injected
  // via the InvokeOptions. But AilProcess doesn't support leading args...
  //
  // Solution: write a tiny shell wrapper. Actually simplest: just return the path
  // to a wrapper script that injects 'node' and the stub path.
  // Even simpler: on Linux, node scripts with a shebang work as executables IF
  // they are chmod +x. Let's use 'node' as binaryPath and stub-ail.js as a hack.
  //
  // Best solution: We can use AilProcess with binaryPath='node' won't work since
  // AilProcess adds --once, --pipeline etc. as first args.
  //
  // Real solution: make stub-ail.js ignore unknown flags (which it does!),
  // and invoke it via `node stub-ail.js`. We pass binaryPath as a shell script
  // that calls `node stub-ail.js`.
  //
  // For Linux tests: use a wrapper sh script.
  return 'PLACEHOLDER'; // replaced below
}

// Use a factory that creates a wrapper command string. Since spawn() on Linux
// can execute Node scripts directly if they have a shebang, but we can't rely
// on that here. Instead, we'll create AilProcess with 'node' as binary and
// inject the stub path before the normal args by overriding the constructor.
//
// Actually, the cleanest approach: create a tiny wrapper at test time.

function makeProcess(mode?: string): AilProcess {
  // Build: node /path/to/stub-ail.js [--stub-mode <mode>] [... AilProcess injects --once etc.]
  // AilProcess.invoke calls: spawn(binaryPath, ['--once', prompt, '--pipeline', path, '--output-format', 'json'])
  // We want:                 spawn('node',     [stubPath, '--once', prompt, '--pipeline', ...])
  //
  // We can't change binaryPath to 'node' and inject stubPath, because AilProcess
  // prepends its own args. Instead, we subclass AilProcess to wrap the spawn call.
  //
  // Simplest compromise: use a shell script as binaryPath.
  // On Linux we can make a script: #!/bin/sh\nexec node /path/to/stub-ail.js "$@"
  //
  // OR: we just use 'node' as binaryPath with a special env var that stub-ail.js reads.
  //
  // BEST approach: stub-ail.js ignores flags it doesn't know (--once, --pipeline, etc.)
  // so we can just use a wrapper script.
  //
  // For cross-platform simplicity, we'll use a temporary wrapper approach.
  // On this Linux CI box we write a small sh file at a predictable tmp path.

  const wrapperPath = path.join(require('os').tmpdir(), 'stub-ail-wrapper-' + (mode || 'happy') + '.sh');
  const modeArg = mode ? `--stub-mode ${mode}` : '';
  require('fs').writeFileSync(wrapperPath, `#!/bin/sh\nexec node "${STUB_AIL}" ${modeArg} "$@"\n`);
  require('fs').chmodSync(wrapperPath, '755');
  return new AilProcess(wrapperPath, '/tmp');
}

// ── Tests ─────────────────────────────────────────────────────────────────────

suite('AilProcess: happy path', () => {

  test('invoke() resolves after binary exits', async () => {
    const proc = makeProcess();
    await proc.invoke('test prompt', '/fake/.ail.yaml', {});
    // If it resolves without throwing, the test passes
  });

  test('raw events arrive in correct order', async () => {
    const proc = makeProcess();
    const rawEvents: AilEvent[] = [];
    proc.onRawEvent((e) => rawEvents.push(e));

    await proc.invoke('test prompt', '/fake/.ail.yaml', {});

    assert.ok(rawEvents.length >= 3, `Expected ≥3 events, got ${rawEvents.length}`);
    assert.strictEqual(rawEvents[0].type, 'run_started');
    assert.strictEqual(rawEvents[rawEvents.length - 1].type, 'pipeline_completed');
  });

  test('run_started event has expected fields', async () => {
    const proc = makeProcess();
    let runStarted: AilEvent | undefined;
    proc.onRawEvent((e) => { if (e.type === 'run_started') runStarted = e; });

    await proc.invoke('test prompt', '/fake/.ail.yaml', {});

    assert.ok(runStarted, 'run_started event should be emitted');
    if (runStarted?.type === 'run_started') {
      assert.strictEqual(typeof runStarted.run_id, 'string');
      assert.strictEqual(typeof runStarted.total_steps, 'number');
    }
  });

  test('stream_delta maps to RunnerEvent stream_delta', async () => {
    const proc = makeProcess();
    const events: RunnerEvent[] = [];
    proc.onEvent((e) => events.push(e));

    await proc.invoke('test prompt', '/fake/.ail.yaml', {});

    const delta = events.find((e) => e.type === 'stream_delta');
    assert.ok(delta, 'stream_delta RunnerEvent should arrive');
    if (delta?.type === 'stream_delta') {
      assert.strictEqual(delta.text, 'Hello from stub!');
    }
  });

  test('step_started raw event has step_id and step_index', async () => {
    const proc = makeProcess();
    let stepStarted: AilEvent | undefined;
    proc.onRawEvent((e) => { if (e.type === 'step_started') stepStarted = e; });

    await proc.invoke('test prompt', '/fake/.ail.yaml', {});

    assert.ok(stepStarted, 'step_started event should arrive');
    if (stepStarted?.type === 'step_started') {
      assert.strictEqual(stepStarted.step_id, 'review');
      assert.strictEqual(stepStarted.step_index, 0);
    }
  });
});

suite('AilProcess: error-exit mode', () => {

  test('non-zero exit emits RunnerEvent error', async () => {
    const proc = makeProcess('error-exit');
    const events: RunnerEvent[] = [];
    proc.onEvent((e) => events.push(e));

    await proc.invoke('test prompt', '/fake/.ail.yaml', {});

    const errEvent = events.find((e) => e.type === 'error');
    assert.ok(errEvent, 'error RunnerEvent should be emitted on non-zero exit');
    if (errEvent?.type === 'error') {
      assert.ok(errEvent.message.includes('1'), `expected error message to include exit code, got: "${errEvent.message}"`);
    }
  });

  test('invoke() still resolves (errors are surfaced via events, not rejection)', async () => {
    const proc = makeProcess('error-exit');
    // Should not reject
    await proc.invoke('test prompt', '/fake/.ail.yaml', {});
  });
});

suite('AilProcess: stdin-echo mode', () => {

  test('writeStdin() delivers a message to the process', async () => {
    const proc = makeProcess('stdin-echo');
    const rawEvents: AilEvent[] = [];
    proc.onRawEvent((e) => rawEvents.push(e));

    // Kick off the run; the stub blocks waiting for stdin
    const invocationPromise = proc.invoke('test prompt', '/fake/.ail.yaml', {});

    // Wait briefly for the process to start and write to stdout
    await new Promise<void>((resolve) => setTimeout(resolve, 100));

    // Write a control message to stdin
    proc.writeStdin({ type: 'hitl_response', step_id: 'echo', text: 'approved' });

    await invocationPromise;

    // The stub echoes the stdin line back as a step_failed error
    const failedEvent = rawEvents.find((e) => e.type === 'step_failed');
    assert.ok(failedEvent, 'step_failed event should have been emitted');
    if (failedEvent?.type === 'step_failed') {
      assert.ok(failedEvent.error.includes('hitl_response'),
        `Expected error to contain "hitl_response", got: "${failedEvent.error}"`);
    }
  });
});

suite('AilProcess: cancel', () => {

  test('cancel() while running kills the process and invoke resolves', async () => {
    // Use error-exit mode (fast), but cancel before it can exit naturally
    // We really just want to verify cancel() doesn't throw and invoke() resolves.
    const proc = makeProcess('happy');
    const invocationPromise = proc.invoke('test prompt', '/fake/.ail.yaml', {});
    proc.cancel();
    await invocationPromise; // should resolve, not hang
  });
});
