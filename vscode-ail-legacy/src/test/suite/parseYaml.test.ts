/**
 * Tests for parseStepsFromYaml().
 */

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { parseStepsFromYaml } from "../../utils/parseYaml";

function writeTmp(content: string): string {
  const f = path.join(os.tmpdir(), `ail-test-${Date.now()}-${Math.random().toString(36).slice(2)}.ail.yaml`);
  fs.writeFileSync(f, content, "utf-8");
  return f;
}

suite("parseStepsFromYaml", () => {
  test("two prompt steps", () => {
    const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: review
    prompt: "Review the code"
  - id: refactor
    prompt: "Refactor based on review"
`);
    const steps = parseStepsFromYaml(f);
    assert.strictEqual(steps.length, 2);
    assert.strictEqual(steps[0].id, "review");
    assert.strictEqual(steps[0].type, "prompt");
    assert.strictEqual(steps[1].id, "refactor");
    assert.strictEqual(steps[1].type, "prompt");
    fs.unlinkSync(f);
  });

  test("mixed step types", () => {
    const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: check
    context:
      shell: "cargo test"
  - id: human_gate
    action: pause_for_human
`);
    const steps = parseStepsFromYaml(f);
    assert.strictEqual(steps.length, 2);
    assert.strictEqual(steps[0].type, "context");
    assert.strictEqual(steps[1].type, "action");
    fs.unlinkSync(f);
  });

  test("empty pipeline", () => {
    const f = writeTmp(`version: "0.0.1"\npipeline: []\n`);
    const steps = parseStepsFromYaml(f);
    assert.strictEqual(steps.length, 0);
    fs.unlinkSync(f);
  });

  test("nonexistent file", () => {
    const steps = parseStepsFromYaml("/nonexistent/path.ail.yaml");
    assert.strictEqual(steps.length, 0);
  });

  test("multi-line prompt", () => {
    const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: analyze
    prompt: |
      You are a senior engineer.
      Review the code for correctness and style.
`);
    const steps = parseStepsFromYaml(f);
    assert.strictEqual(steps.length, 1);
    assert.strictEqual(steps[0].id, "analyze");
    assert.strictEqual(steps[0].type, "prompt");
    fs.unlinkSync(f);
  });

  test("quoted step id", () => {
    const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: "my-step"
    prompt: "do something"
`);
    const steps = parseStepsFromYaml(f);
    assert.strictEqual(steps.length, 1);
    assert.strictEqual(steps[0].id, "my-step");
    fs.unlinkSync(f);
  });

  test("YAML comments do not affect parsing", () => {
    const f = writeTmp(`version: "0.0.1"
pipeline:
  # First step
  - id: review
    prompt: "Review this" # inline comment
  - id: summarize
    prompt: "Summarize"
`);
    const steps = parseStepsFromYaml(f);
    assert.strictEqual(steps.length, 2);
    assert.strictEqual(steps[0].id, "review");
    assert.strictEqual(steps[1].id, "summarize");
    fs.unlinkSync(f);
  });
});
