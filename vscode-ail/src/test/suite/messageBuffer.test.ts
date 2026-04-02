import * as assert from 'assert';
import { MessageBuffer } from '../../panels/MessageBuffer';

suite('MessageBuffer', () => {
  test('messages posted before markReady() are queued, not sent', () => {
    const sent: object[] = [];
    const buf = new MessageBuffer((m) => sent.push(m));
    buf.post({ cmd: 'init' });
    buf.post({ cmd: 'runStarted' });
    assert.strictEqual(sent.length, 0);
    assert.strictEqual(buf.ready, false);
  });

  test('markReady() flushes all queued messages in order', () => {
    const sent: object[] = [];
    const buf = new MessageBuffer((m) => sent.push(m));
    buf.post({ cmd: 'a' });
    buf.post({ cmd: 'b' });
    buf.post({ cmd: 'c' });
    buf.markReady();
    assert.deepStrictEqual(sent, [{ cmd: 'a' }, { cmd: 'b' }, { cmd: 'c' }]);
    assert.strictEqual(buf.ready, true);
  });

  test('messages posted after markReady() are sent immediately', () => {
    const sent: object[] = [];
    const buf = new MessageBuffer((m) => sent.push(m));
    buf.markReady();
    buf.post({ cmd: 'live' });
    assert.deepStrictEqual(sent, [{ cmd: 'live' }]);
  });

  test('double markReady() does not re-flush', () => {
    const sent: object[] = [];
    const buf = new MessageBuffer((m) => sent.push(m));
    buf.post({ cmd: 'once' });
    buf.markReady();
    assert.strictEqual(sent.length, 1);
    buf.markReady(); // second call should be a no-op
    assert.strictEqual(sent.length, 1);
  });

  test('markReady() with empty queue does not error', () => {
    const buf = new MessageBuffer(() => { /* no-op */ });
    assert.doesNotThrow(() => buf.markReady());
    assert.strictEqual(buf.ready, true);
  });
});
