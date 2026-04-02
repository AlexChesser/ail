/**
 * Tests for the NDJSON stream parser.
 */

import * as assert from "assert";
import { Readable } from "stream";
import { parseNdjsonStream } from "../../ndjson";

suite("ndjson", () => {
  function streamFrom(chunks: string[]): Readable {
    const r = new Readable({ read() {} });
    for (const c of chunks) r.push(c);
    r.push(null);
    return r;
  }

  test("single valid event", (done) => {
    const events: unknown[] = [];
    const r = streamFrom(['{"type":"run_started","run_id":"abc"}\n']);
    parseNdjsonStream(r, (e) => events.push(e), () => {});
    r.on("end", () => {
      assert.strictEqual(events.length, 1);
      assert.deepStrictEqual((events[0] as { type: string }).type, "run_started");
      done();
    });
  });

  test("multiple events", (done) => {
    const events: unknown[] = [];
    const payload = '{"type":"a"}\n{"type":"b"}\n{"type":"c"}\n';
    const r = streamFrom([payload]);
    parseNdjsonStream(r, (e) => events.push(e), () => {});
    r.on("end", () => {
      assert.strictEqual(events.length, 3);
      done();
    });
  });

  test("empty lines skipped", (done) => {
    const events: unknown[] = [];
    const r = streamFrom(['\n\n{"type":"x"}\n\n']);
    parseNdjsonStream(r, (e) => events.push(e), () => {});
    r.on("end", () => {
      assert.strictEqual(events.length, 1);
      done();
    });
  });

  test("malformed JSON skipped gracefully", (done) => {
    const events: unknown[] = [];
    const errors: Error[] = [];
    const r = streamFrom(['not-json\n{"type":"ok"}\n']);
    parseNdjsonStream(r, (e) => events.push(e), (e) => errors.push(e));
    r.on("end", () => {
      assert.strictEqual(events.length, 1);
      done();
    });
  });

  test("partial lines buffered correctly", (done) => {
    const events: unknown[] = [];
    const r = streamFrom(['{"type":', '"split"}\n']);
    parseNdjsonStream(r, (e) => events.push(e), () => {});
    r.on("end", () => {
      assert.strictEqual(events.length, 1);
      done();
    });
  });
});
