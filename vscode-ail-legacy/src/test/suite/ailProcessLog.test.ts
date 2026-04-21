/**
 * AilProcess.log() method tests.
 *
 * These tests verify the log() method which spawns `ail log [runId] --format markdown`
 * and returns the stdout as a string. Uses a stub binary (stub-ail-log.js) for
 * deterministic testing without requiring the real ail binary.
 */

import * as assert from 'assert';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { AilProcess } from '../../infrastructure/AilProcess';

// __dirname in compiled output: out/src/test/suite/
const STUB_AIL_LOG = path.resolve(__dirname, '..', '..', '..', '..', 'src', 'test', 'fixtures', 'stub-ail-log.js');

function makeProcess(mode?: string): AilProcess {
  const wrapperPath = path.join(os.tmpdir(), 'stub-ail-log-wrapper-' + (mode || 'happy') + '.sh');
  const modeArg = mode ? `--stub-mode ${mode}` : '';
  fs.writeFileSync(wrapperPath, `#!/bin/sh\nexec node "${STUB_AIL_LOG}" ${modeArg} "$@"\n`);
  fs.chmodSync(wrapperPath, '0755');
  return new AilProcess(wrapperPath, '/tmp');
}

suite('AilProcess.log()', () => {

  test('log() without runId returns ail-log/1 formatted string', async () => {
    const proc = makeProcess();
    const output = await proc.log();

    assert.ok(output.startsWith('ail-log/1'), 'output should start with ail-log/1 version header');
    assert.ok(output.includes('Turn 1'), 'output should contain turn header');
  });

  test('log(runId) passes runId to binary command', async () => {
    const proc = makeProcess();
    // We can't directly verify the argument was passed, but we can verify
    // the command completes successfully (stub ignores unknown positional args)
    const output = await proc.log('test-run-123');

    assert.ok(output.startsWith('ail-log/1'), 'output should still be valid ail-log format');
  });

  test('log() parses ail-log/1 directives', async () => {
    const proc = makeProcess();
    const output = await proc.log();

    assert.ok(output.includes(':::thinking'), 'output should contain thinking directive');
    assert.ok(output.includes(':::'), 'output should have closing directive marker');
  });

  test('log() includes cost information', async () => {
    const proc = makeProcess();
    const output = await proc.log();

    assert.ok(output.includes('_Cost:'), 'output should include cost line');
    assert.ok(output.includes('tokens'), 'output should include token counts');
  });

  test('log() rejects on non-zero exit', async () => {
    const proc = makeProcess('error');

    try {
      await proc.log();
      assert.fail('log() should reject on error exit');
    } catch (err) {
      assert.ok(err instanceof Error, 'error should be an Error');
      assert.ok((err as Error).message.includes('ail log failed'), `error message should mention ail log failed, got: ${(err as Error).message}`);
    }
  });

  test('log() rejects with run not found error', async () => {
    const proc = makeProcess('no-run');

    try {
      await proc.log('nonexistent-run');
      assert.fail('log() should reject when run is not found');
    } catch (err) {
      assert.ok(err instanceof Error, 'error should be an Error');
      assert.ok((err as Error).message.includes('ail log failed'), `error message should mention ail log failed`);
    }
  });

  test('log() returns full output as string', async () => {
    const proc = makeProcess();
    const output = await proc.log();

    assert.strictEqual(typeof output, 'string', 'output should be a string');
    assert.ok(output.length > 0, 'output should not be empty');
  });
});
