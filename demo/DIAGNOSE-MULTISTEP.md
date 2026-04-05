# Diagnose Multi-Step Pipeline Execution

Paste this file into Claude Code locally (with ollama running) to identify why the second step of the `.include-review.yaml` pipeline fails to complete.

---

## Step 1: Build

```bash
cargo build --release 2>&1 | tail -5
```

Expected: `Compiling ail ...` then `Finished release profile`.

---

## Step 2: Run the pipeline and capture NDJSON output

```bash
./target/release/ail --once "say hello" \
  --pipeline demo/.include-review.yaml \
  --output-format json 2>/tmp/ail-stderr.txt > /tmp/ail-out.ndjson
echo "exit: $?"
```

---

## Step 3: Inspect event sequence

```bash
jq -c '.type' /tmp/ail-out.ndjson
```

Expected sequence:
```
"run_started"
"step_started"        ← invocation
"runner_event"        ← (many: stream_delta, tool_use, tool_result, completed)
"step_completed"
"step_started"        ← review
"runner_event"        ← (many)
"step_completed"
"pipeline_completed"
```

If `step_started` for `review` never appears, template resolution failed before the runner was called.
If `step_failed` appears, read its detail:

```bash
jq 'select(.type == "step_failed" or .type == "pipeline_error")' /tmp/ail-out.ndjson
```

---

## Step 4: Check template resolution

```bash
./target/release/ail materialize --pipeline demo/.include-review.yaml
```

This expands `{{ step.invocation.result }}` at parse time (statically). If it errors here, the template syntax is wrong. If it succeeds, the variable resolves at runtime from the turn log.

---

## Step 5: Check stderr for runner errors

```bash
cat /tmp/ail-stderr.txt
```

Look for tracing output indicating why the second runner invocation failed (e.g., ollama connection refused, session resume failure, auth error).

---

## Step 6: Check headless setting

```bash
grep -r "ail-chat.headless" .vscode/ 2>/dev/null || echo "not set (defaults to false)"
```

If `true`, all tool permissions are bypassed with `--dangerously-skip-permissions`. Permission buttons will never appear in this mode.

---

## Step 7: Verify tool_use events carry id and input

```bash
jq 'select(.type == "runner_event" and .event.type == "tool_use")' /tmp/ail-out.ndjson | head -20
```

After the fix in this release, each `tool_use` event should include `tool_use_id` and `input` fields. If they are absent, you are running an older binary.

---

## Reporting

Report back with:
1. The full event type sequence from Step 3
2. Any `step_failed` / `pipeline_error` detail from Step 3
3. Any relevant stderr lines from Step 5
4. Whether `step_started` appears for both `invocation` and `review`
