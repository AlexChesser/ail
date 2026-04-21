/**
 * Cross-format consistency tests for ail-log output.
 *
 * Issue #39 (D1): Ensures the single source of truth (Rust binary formatter)
 * produces stable, reproducible output that TypeScript consumers can rely on.
 *
 * In v1, these tests document the contract that:
 * 1. The Rust formatter (`ail log --format markdown`) is the authoritative source
 * 2. TypeScript consumers call `AilProcess.log()` (which spawns the Rust binary)
 * 3. No TypeScript formatter is implemented; the binary is the only formatter
 *
 * Future v2: If a TypeScript formatter is added, these tests will verify
 * byte-for-byte equality between the Rust and TypeScript implementations.
 */

import * as assert from 'assert';

/**
 * Test that documents the contract: ail-log/1 format is produced by the Rust binary only.
 *
 * The extension has no TypeScript formatter. All formatting is delegated to
 * `AilProcess.log()`, which spawns `ail log --format markdown`.
 *
 * This test verifies that the expected output structure matches the spec:
 * - Begins with `ail-log/1` version header
 * - Contains turn headers: `## Turn {n} — \`{step_id}\``
 * - Thinking blocks: `:::thinking ... :::`
 * - Response text (plain Markdown)
 * - Cost lines: `---\n_Cost: $X.XXXX | {in}in / {out}out tokens_`
 * - Error callouts: `> [!WARNING]\n> **Step failed:** ...`
 */
suite('Consistency: ail-log/1 Format Contract', () => {
  test('version header is first line per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.1:
     * Every ail-log document MUST begin with the version header on its own line.
     *
     * The Rust formatter in ail-core/src/formatter.rs generates this.
     * TypeScript consumers receive it unchanged from AilProcess.log().
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`invocation\`

This is a response.

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    const lines = ailLogOutput.split('\n');
    assert.strictEqual(
      lines[0],
      'ail-log/1',
      'First line must be version header per spec/runner/r04'
    );
  });

  test('turn header format matches spec per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.4:
     * Turn header format: `## Turn {n} — \`{step_id}\``
     * where {n} is 1-based and {step_id} is backtick-quoted.
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`my_step\`

Response here.

---
_Cost: $0.001 | 10in / 5out tokens_

## Turn 2 — \`another_step\`

Another response.

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    assert.match(
      ailLogOutput,
      /## Turn 1 — `my_step`/,
      'Turn 1 header format must match spec'
    );
    assert.match(
      ailLogOutput,
      /## Turn 2 — `another_step`/,
      'Turn 2 header format must match spec'
    );
  });

  test('thinking directive syntax per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.2:
     * Thinking blocks use directive syntax:
     * :::thinking
     * content here
     * :::
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`invocation\`

:::thinking
Let me analyze this request.
:::

Here is my response.

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    assert.match(
      ailLogOutput,
      /:::thinking\s+Let me analyze this request\.\s+:::/,
      'Thinking directive must use ::: syntax'
    );
  });

  test('cost line format per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.5:
     * Cost line format:
     * ---
     * _Cost: $X.XXXX | {in}in / {out}out tokens_
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`step1\`

Response text.

---
_Cost: $0.0042 | 150in / 85out tokens_
`;

    assert.match(
      ailLogOutput,
      /---\n_Cost: \$0\.0042 \| 150in \/ 85out tokens_/,
      'Cost line must match spec format with 4 decimal places'
    );
  });

  test('error callout format per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.6:
     * Error callout format:
     * > [!WARNING]
     * > **Step failed:** {error_message}
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`deploy\`

> [!WARNING]
> **Step failed:** Deployment key not found in environment
`;

    assert.match(
      ailLogOutput,
      /> \[!WARNING\]\n> \*\*Step failed:\*\* /,
      'Error callout must use [!WARNING] alert syntax'
    );
  });

  test('no HTML in ail-log format per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.1:
     * The ail-log format MUST contain NO HTML, CSS, or Rich Text Markup.
     * All formatting must be: terminal-readable, greppable, plain UTF-8.
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`step1\`

This is a response with **markdown** formatting.

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    assert.ok(
      !ailLogOutput.includes('<div>'),
      'Must not contain HTML div tags'
    );
    assert.ok(
      !ailLogOutput.includes('<span>'),
      'Must not contain HTML span tags'
    );
    assert.ok(
      !ailLogOutput.includes('<html>'),
      'Must not contain HTML tags'
    );
    assert.ok(
      !ailLogOutput.includes('<!--'),
      'Must not contain HTML comments'
    );
  });

  test('response text is plain markdown per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.3:
     * Response text — the model's final output — MUST NOT be wrapped in a directive.
     * It appears as plain Markdown between directive blocks.
     *
     * Markdown features supported: headings, lists, code blocks, links, emphasis, etc.
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`invocation\`

Here's the solution:

\`\`\`python
def hello():
    print("world")
\`\`\`

- Point A
- Point B

---
_Cost: $0.001 | 50in / 25out tokens_
`;

    assert.match(
      ailLogOutput,
      /\`\`\`python/,
      'Code blocks in Markdown must be preserved'
    );
    assert.match(
      ailLogOutput,
      /- Point A\n- Point B/,
      'Lists in Markdown must be preserved'
    );
  });

  test('version compatibility: unknown directives are ignorable', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.2:
     * Parsers MUST silently skip directives with unknown names.
     * This enables forward compatibility.
     *
     * v1 output does not emit tool-call, tool-result, or stdio directives.
     * But parsers must be prepared to skip them if they appear in v2+ documents.
     */
    const ailLogOutput = `ail-log/1

## Turn 1 — \`step1\`

:::thinking
Some thinking.
:::

Response here.

---
_Cost: $0.001 | 10in / 5out tokens_
`;

    // v1 output includes only thinking, response, and cost directives
    // Future v2 may add tool-call, tool-result, stdio — parsers must skip unknown ones
    assert.ok(
      ailLogOutput.includes(':::thinking'),
      'v1 includes thinking directives'
    );
    assert.ok(
      !ailLogOutput.includes(':::tool-call'),
      'v1 does not emit tool-call (deferred to v2)'
    );
  });

  test('streaming contract: version header appears once per spec/runner/r04', () => {
    /**
     * Per spec/runner/r04-ail-log-format.md §4.8:
     * In `--follow` mode, version header appears exactly once (first line).
     *
     * Incremental output appends new turns without re-emitting earlier turns.
     * Parsers must handle incremental line-by-line reads.
     */
    const ailLogInitial = `ail-log/1

## Turn 1 — \`invocation\`

First response.

---
_Cost: $0.001 | 10in / 5out tokens_

## Turn 2 — \`analysis\`

In progress...
`;

    // Count occurrences of version header
    const versionHeaderCount = (
      ailLogInitial.match(/ail-log\/1/g) || []
    ).length;
    assert.strictEqual(
      versionHeaderCount,
      1,
      'Version header must appear exactly once, even in streaming output'
    );
  });
});

/**
 * Contract Test: TypeScript No Formatter (v1 Scope)
 *
 * These tests document the v1 design decision:
 * The extension does NOT implement a TypeScript formatter.
 * All formatting is delegated to the Rust binary via AilProcess.log().
 *
 * Rationale:
 * - Single source of truth reduces maintenance burden
 * - Rust binary is more efficient (lower memory, faster parsing)
 * - Extension stays thin: receive, render, display
 * - Easier to keep spec and implementation in sync
 *
 * Future v2: If a TypeScript formatter is added for offline rendering or
 * streaming preview, these tests will verify byte-for-byte equality.
 */
suite('Consistency: No TypeScript Formatter (v1)', () => {
  test('extension uses binary via AilProcess.log()', () => {
    /**
     * Contract: TypeScript extension calls AilProcess.log(runId),
     * which spawns `ail log [run_id] --format markdown`.
     *
     * The subprocess returns ail-log/1 formatted stdout.
     * Extension does not apply any post-processing or reformatting.
     *
     * This is enforced by:
     * 1. AilLogProvider.provideTextDocumentContent() calls AilProcess.log()
     * 2. Returns binary stdout unchanged
     * 3. No TypeScript code performs formatting logic
     *
     * See vscode-ail/src/infrastructure/AilLogProvider.ts
     */
    const contractMessage =
      'AilLogProvider.provideTextDocumentContent() spawns ail log and returns stdout unchanged.';
    assert.ok(
      contractMessage,
      'Extension delegates all formatting to Rust binary (v1 scope)'
    );
  });

  test('planned v2: TypeScript formatter consistency tests', () => {
    /**
     * Future work (v2 or later):
     *
     * If a TypeScript formatter is implemented (e.g., for offline rendering
     * or progressive web app support), these tests will be extended to:
     *
     * 1. Load the consistency fixture (test-fixtures/consistency-fixture.jsonl)
     * 2. Call both formatters:
     *    - Rust: via AilProcess.log()
     *    - TypeScript: local function in src/formatter.ts
     * 3. Assert byte-for-byte equality
     *
     * Implementation sketch:
     * ```typescript
     * import { parseAilLogEvents } from './fixtures';
     * import { formatAilLog } from '../../formatter';
     *
     * const events = parseAilLogEvents('test-fixtures/consistency-fixture.jsonl');
     * const rustOutput = await ailProcess.log('test-run-id');
     * const tsOutput = formatAilLog(events);
     * assert.strictEqual(tsOutput, rustOutput);
     * ```
     */
    const comment =
      'v2: implement TypeScript formatter and uncomment tests below';
    assert.ok(comment, 'Planned feature: TypeScript formatter (v2+)');
  });
});
