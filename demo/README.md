# v0.0.1 Demo

## What this demonstrates

SPEC §21 target: one `.ail.yaml`, one `dont_be_stupid` step, two Claude invocations, visibly working.

## How to run

From the `demo/` directory (outside a Claude Code session):

```bash
# Step 1 — validate the pipeline
ail --pipeline .ail.yaml validate

# Step 2 — single run
ail --pipeline .ail.yaml --once "Write a function that adds two numbers"
```

Expected output: two Claude invocations. The first runs the `--once` prompt. The second runs `dont_be_stupid`, whose prompt text is the review instruction above.

## Observable evidence

- `ail validate` prints: `Pipeline valid: 1 step(s)`, exit 0
- `ail --once "..."` prints the response from the `dont_be_stupid` step to stdout
- `.ail/runs/<uuid>.jsonl` contains two NDJSON entries (one per invocation)

## Known limitation

`--once` cannot be run from inside a Claude Code session (the claude CLI nested-session guard blocks it). Run from a normal terminal.
