/**
 * Unit tests for parseStepsFromYaml().
 * No VS Code API required — imports from src/utils/parseYaml directly.
 */

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { parseStepsFromYaml } from "../src/utils/parseYaml";

function writeTmp(content: string): string {
  const f = path.join(os.tmpdir(), `ail-test-${Date.now()}.ail.yaml`);
  fs.writeFileSync(f, content, "utf-8");
  return f;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

// Test 1: two prompt steps
{
  const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: review
    prompt: "Review the code"
  - id: refactor
    prompt: "Refactor based on review"
`);
  const steps = parseStepsFromYaml(f);
  assert.strictEqual(steps.length, 2, "Should parse 2 steps");
  assert.strictEqual(steps[0].id, "review");
  assert.strictEqual(steps[0].type, "prompt");
  assert.strictEqual(steps[1].id, "refactor");
  assert.strictEqual(steps[1].type, "prompt");
  fs.unlinkSync(f);
  console.log("✓ two prompt steps");
}

// Test 2: mixed step types
{
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
  console.log("✓ mixed step types");
}

// Test 3: empty pipeline
{
  const f = writeTmp(`version: "0.0.1"\npipeline: []\n`);
  const steps = parseStepsFromYaml(f);
  assert.strictEqual(steps.length, 0, "Empty pipeline should yield no steps");
  fs.unlinkSync(f);
  console.log("✓ empty pipeline");
}

// Test 4: nonexistent file
{
  const steps = parseStepsFromYaml("/nonexistent/path.ail.yaml");
  assert.strictEqual(steps.length, 0, "Missing file should yield no steps");
  console.log("✓ nonexistent file handled gracefully");
}

// Test 5: multi-line prompt (would silently fail with regex parser)
{
  const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: analyze
    prompt: |
      You are a senior engineer.
      Review the following code for correctness,
      performance, and style.
`);
  const steps = parseStepsFromYaml(f);
  assert.strictEqual(steps.length, 1, "Multi-line prompt should parse as one step");
  assert.strictEqual(steps[0].id, "analyze");
  assert.strictEqual(steps[0].type, "prompt");
  fs.unlinkSync(f);
  console.log("✓ multi-line prompt parsed correctly");
}

// Test 6: quoted step id (would strip quotes with regex parser but now exact)
{
  const f = writeTmp(`version: "0.0.1"
pipeline:
  - id: "my-step"
    prompt: "do something"
`);
  const steps = parseStepsFromYaml(f);
  assert.strictEqual(steps.length, 1);
  assert.strictEqual(steps[0].id, "my-step", "Quoted id should be unquoted");
  fs.unlinkSync(f);
  console.log("✓ quoted step id parsed correctly");
}

// Test 7: steps with YAML comments (would confuse regex parser)
{
  const f = writeTmp(`version: "0.0.1"
pipeline:
  # First step reviews the PR
  - id: review
    prompt: "Review this PR" # brief review
  - id: summarize
    prompt: "Summarize findings"
`);
  const steps = parseStepsFromYaml(f);
  assert.strictEqual(steps.length, 2, "Comments should not affect parsing");
  assert.strictEqual(steps[0].id, "review");
  assert.strictEqual(steps[1].id, "summarize");
  fs.unlinkSync(f);
  console.log("✓ YAML comments handled correctly");
}

console.log("✓ pipeline-tree.test.ts — all tests passed");
