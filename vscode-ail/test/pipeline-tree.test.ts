/**
 * Unit tests for PipelineTreeProvider step parsing.
 * No VS Code API required — tests the pure YAML-parsing logic directly.
 */

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

// Inline the parsing logic to test it independently of VS Code API.
interface ParsedStep {
  id: string;
  type: string;
  line: number;
}

function parseStepsFromYaml(filePath: string): ParsedStep[] {
  let content: string;
  try {
    content = fs.readFileSync(filePath, "utf-8");
  } catch {
    return [];
  }

  const steps: ParsedStep[] = [];
  const lines = content.split("\n");
  let inPipeline = false;
  let currentId: string | undefined;
  let currentLine = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (trimmed === "pipeline:") {
      inPipeline = true;
      continue;
    }

    if (inPipeline) {
      if (/^\S/.test(line) && trimmed !== "") {
        inPipeline = false;
        continue;
      }

      const idMatch = trimmed.match(/^-?\s*id:\s*(.+)/);
      if (idMatch) {
        currentId = idMatch[1].trim().replace(/['"]/g, "");
        currentLine = i;
      }

      if (currentId) {
        let type: string | undefined;
        if (trimmed.match(/^prompt:/)) type = "prompt";
        else if (trimmed.match(/^context:/)) type = "context";
        else if (trimmed.match(/^action:/)) type = "action";
        else if (trimmed.match(/^pipeline:/)) type = "pipeline";

        if (type) {
          steps.push({ id: currentId, type, line: currentLine });
          currentId = undefined;
        }
      }
    }
  }

  return steps;
}

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

console.log("✓ pipeline-tree.test.ts — all tests passed");
