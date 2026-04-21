#!/usr/bin/env node
/**
 * stub-ail.js — deterministic test double for the ail binary.
 *
 * Emits a predefined NDJSON event sequence to stdout, then exits.
 * Behaviour is controlled by the --stub-mode flag inserted into the args
 * by the test harness. AilProcess passes through all other flags unchanged.
 *
 * Modes:
 *   happy       (default) — emit a complete 1-step run, exit 0
 *   error-exit  — emit run_started, then exit with code 1 (no pipeline_completed)
 *   stdin-echo  — echo the first stdin line back as a step_failed error, then exit 0
 */

'use strict';

const args = process.argv.slice(2);
const modeIdx = args.indexOf('--stub-mode');
const mode = modeIdx !== -1 ? args[modeIdx + 1] : 'happy';

function emit(event) {
  process.stdout.write(JSON.stringify(event) + '\n');
}

if (mode === 'error-exit') {
  emit({ type: 'run_started', run_id: 'stub-run', pipeline_source: null, total_steps: 0 });
  process.exit(1);

} else if (mode === 'stdin-echo') {
  emit({ type: 'run_started', run_id: 'stub-run', pipeline_source: null, total_steps: 1 });
  emit({ type: 'step_started', step_id: 'echo', step_index: 0, total_steps: 1, resolved_prompt: null });

  // Read one line from stdin, echo it back as an error message
  let buf = '';
  process.stdin.resume();
  process.stdin.setEncoding('utf8');
  process.stdin.on('data', (chunk) => {
    buf += chunk;
    const nl = buf.indexOf('\n');
    if (nl !== -1) {
      const line = buf.slice(0, nl);
      process.stdin.pause();
      emit({ type: 'step_failed', step_id: 'echo', error: 'stdin: ' + line });
      emit({ type: 'pipeline_completed', outcome: 'completed' });
      process.exit(0);
    }
  });

} else {
  // happy path
  emit({ type: 'run_started', run_id: 'stub-run', pipeline_source: 'test.ail.yaml', total_steps: 1 });
  emit({ type: 'step_started', step_id: 'review', step_index: 0, total_steps: 1, resolved_prompt: 'Test prompt' });
  emit({ type: 'runner_event', event: { type: 'stream_delta', text: 'Hello from stub!' } });
  emit({ type: 'step_completed', step_id: 'review', cost_usd: 0.001, input_tokens: 10, output_tokens: 5, response: 'Hello from stub!' });
  emit({ type: 'pipeline_completed', outcome: 'completed' });
  process.exit(0);
}
