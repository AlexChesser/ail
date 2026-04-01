/**
 * Unit tests for ndjson.ts — no VS Code API required.
 */

import * as assert from "assert";
import { Readable } from "stream";
import { parseNdjsonStream } from "../src/ndjson";
import { AilEvent } from "../src/types";

function makeReadable(content: string): Readable {
  const r = new Readable({ read() {} });
  r.push(content, "utf-8");
  r.push(null);
  return r;
}

function collectEvents(content: string): Promise<AilEvent[]> {
  return new Promise((resolve, reject) => {
    const events: AilEvent[] = [];
    const stream = makeReadable(content);
    parseNdjsonStream(
      stream,
      (e) => events.push(e),
      (err) => reject(err)
    );
    stream.on("end", () => resolve(events));
  });
}

async function run() {
  // Test 1: single valid event
  {
    const events = await collectEvents(
      '{"type":"run_started","run_id":"abc","pipeline_source":null,"total_steps":1}\n'
    );
    assert.strictEqual(events.length, 1);
    assert.strictEqual(events[0].type, "run_started");
    console.log("✓ single valid event");
  }

  // Test 2: multiple events
  {
    const events = await collectEvents(
      '{"type":"step_started","step_id":"review","step_index":0,"total_steps":1}\n' +
        '{"type":"step_completed","step_id":"review","cost_usd":0.003}\n' +
        '{"type":"pipeline_completed","outcome":"completed"}\n'
    );
    assert.strictEqual(events.length, 3);
    assert.strictEqual(events[0].type, "step_started");
    assert.strictEqual(events[1].type, "step_completed");
    assert.strictEqual(events[2].type, "pipeline_completed");
    console.log("✓ multiple events");
  }

  // Test 3: empty lines are skipped
  {
    const events = await collectEvents(
      '{"type":"pipeline_completed","outcome":"completed"}\n\n\n'
    );
    assert.strictEqual(events.length, 1);
    console.log("✓ empty lines skipped");
  }

  // Test 4: malformed JSON is skipped, not crashed
  {
    const events = await collectEvents(
      'not-json\n{"type":"pipeline_completed","outcome":"completed"}\n'
    );
    assert.strictEqual(events.length, 1, "malformed line should be skipped");
    assert.strictEqual(events[0].type, "pipeline_completed");
    console.log("✓ malformed JSON skipped gracefully");
  }

  // Test 5: partial line buffered then completed
  {
    const events: AilEvent[] = [];
    await new Promise<void>((resolve) => {
      const r = new Readable({ read() {} });
      parseNdjsonStream(r, (e) => events.push(e));
      // Push in two chunks — the line boundary is in the second chunk
      r.push('{"type":"pipeline_completed","outco');
      r.push('me":"completed"}\n');
      r.push(null);
      r.on("end", () => resolve());
    });
    assert.strictEqual(events.length, 1, "partial line should be buffered and completed");
    assert.strictEqual(events[0].type, "pipeline_completed");
    console.log("✓ partial lines buffered correctly");
  }

  console.log("✓ ndjson.test.ts — all tests passed");
}

void run().catch((err) => {
  console.error("FAIL:", err);
  process.exit(1);
});
