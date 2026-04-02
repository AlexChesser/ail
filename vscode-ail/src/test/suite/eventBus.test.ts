import * as assert from 'assert';
import { EventBus } from '../../application/EventBus';

suite('EventBus', () => {
  test('emit() delivers event to handler registered for matching type', () => {
    const bus = new EventBus();
    let received: unknown = null;
    bus.on('step_started', (e) => { received = e; });
    bus.emit({ type: 'step_started', stepId: 'foo', stepIndex: 0, totalSteps: 1 });
    assert.deepStrictEqual(received, { type: 'step_started', stepId: 'foo', stepIndex: 0, totalSteps: 1 });
  });

  test('on() returns a Disposable; disposing it prevents future deliveries', () => {
    const bus = new EventBus();
    let callCount = 0;
    const d = bus.on('pipeline_completed', () => { callCount++; });
    bus.emit({ type: 'pipeline_completed' });
    assert.strictEqual(callCount, 1);
    d.dispose();
    bus.emit({ type: 'pipeline_completed' });
    assert.strictEqual(callCount, 1); // no second call after dispose
  });

  test('multiple handlers for the same type all fire in registration order', () => {
    const bus = new EventBus();
    const calls: number[] = [];
    bus.on('step_completed', () => calls.push(1));
    bus.on('step_completed', () => calls.push(2));
    bus.on('step_completed', () => calls.push(3));
    bus.emit({ type: 'step_completed', stepId: 'bar' });
    assert.deepStrictEqual(calls, [1, 2, 3]);
  });

  test('handler for step_started does not fire when step_completed is emitted', () => {
    const bus = new EventBus();
    let wrongFired = false;
    bus.on('step_started', () => { wrongFired = true; });
    bus.emit({ type: 'step_completed', stepId: 'baz' });
    assert.strictEqual(wrongFired, false);
  });

  test('emitting with no registered handlers does not throw', () => {
    const bus = new EventBus();
    assert.doesNotThrow(() => {
      bus.emit({ type: 'pipeline_completed' });
    });
  });
});
