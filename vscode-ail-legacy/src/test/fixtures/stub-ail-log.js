#!/usr/bin/env node
/**
 * stub-ail-log.js — test double for the `ail log` subcommand.
 *
 * Emits ail-log/1 format output to stdout.
 * Behaviour is controlled by the --stub-mode flag.
 *
 * Modes:
 *   happy       (default) — emit a complete 1-step run log, exit 0
 *   error       — exit with code 1 and stderr message
 *   no-run      — exit with code 1 (run not found)
 */

'use strict';

const args = process.argv.slice(2);
const modeIdx = args.indexOf('--stub-mode');
const mode = modeIdx !== -1 ? args[modeIdx + 1] : 'happy';

if (mode === 'error') {
  console.error('database error');
  process.exit(1);

} else if (mode === 'no-run') {
  console.error('run not found');
  process.exit(1);

} else {
  // happy path — emit a complete 1-step run in ail-log/1 format
  const output = `ail-log/1
## Turn 1 — \`invocation\`

Hello, this is the invocation response.

:::thinking
Let me think about this problem.
:::

---
_Cost: $0.001 | 10in / 5out tokens_
`;
  process.stdout.write(output);
  process.exit(0);
}
