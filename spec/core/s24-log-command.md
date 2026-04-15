# 24. The `ail log` Command

## 24.1 Purpose

The `ail log` command is the **single external interface to run data**. It outputs the formatted result of a completed or in-progress pipeline run in a human-readable, machine-parseable format. All consumers — VS Code extensions, CI systems, monitoring tools, external orchestrators — invoke `ail log` to read run data. There is no direct SQLite access from external processes.

The `ail log` command is distinct from `ail logs` (plural), which outputs session-level tabular summaries for multiple runs.

---

## 24.2 Synopsis

```
ail log [run_id] [flags]
```

### Arguments

| Argument | Type | Required | Semantics |
|----------|------|----------|-----------|
| `run_id` | string (UUID) | No | Identifier of the run to display. If omitted, resolves to the most recent run for the current working directory (identified by SHA-1 hash of the absolute CWD path). If no prior runs exist, error exit code 1. |

### Flags

| Flag | Type | Default | Semantics |
|------|------|---------|-----------|
| `--format <markdown\|json\|raw>` | choice | `markdown` | Output encoding (see §24.3) |
| `--follow` | flag | off | Stream new events as they arrive; process exits when run completes or fails (see §24.4) |

---

## 24.3 Output Formats

### markdown (Default)

Output is ail-log/1 format per `spec/runner/r04-ail-log-format.md`. Suitable for:

- Human reading in a terminal
- Piping to `less`, `grep`, or other text tools
- Syntax highlighting by an IDE or terminal emulator
- Rendering as Markdown in a preview pane

Example:
```bash
ail log
ail log 550e8400-e29b-41d4-a716-446655440000
```

### json

Output is NDJSON (newline-delimited JSON), one object per step. Each object matches the schema of `ail logs --format json` per-step entries:

```json
{
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "step_id": "invocation",
  "step_index": 0,
  "status": "completed",
  "duration_ms": 2345,
  "cost_usd": 0.0008,
  "input_tokens": 85,
  "output_tokens": 42,
  "response": "Here's my response..."
}
```

Suitable for:

- Programmatic consumers (CI, monitoring, external orchestrators)
- Integration with analytics systems
- Cross-format consistency with `ail logs --format json`

Example:
```bash
ail log --format json | jq '.[] | select(.status == "failed")'
```

### raw

Output is the stored JSONL payloads verbatim, without any transformation or formatting. Each line is a JSON object as stored in the `steps` table:

```json
{"run_id":"550e8400-...", "step_id":"invocation", "event_type":"step_completed", ...}
{"run_id":"550e8400-...", "step_id":"review", "event_type":"step_completed", ...}
```

Suitable for:

- Debugging the storage layer
- Batch re-import or data migration
- Direct SQLite event inspection

Example:
```bash
ail log --format raw > /tmp/run_export.jsonl
```

---

## 24.4 The `--follow` Flag

When `--follow` is specified, `ail log` does not exit after printing the current state. Instead, it streams new events as they arrive until the run completes.

### Behavior

1. On startup, the version header and all completed turns are emitted immediately (same as non-follow output)
2. The process polls the SQLite database every 500ms for new turns
3. Each new completed turn is appended to stdout as its turn block (per `r04-ail-log-format.md`)
4. The process exits with code `0` when the final step completes
5. On error (database unavailable, run encountered step_failed), the process exits with code `1` and emits an error callout as the last element

### Streaming Contract

- The version header `ail-log/1` appears **exactly once** on the first line
- All output is appended to stdout, never replaced
- Parser must handle line-by-line incremental reads
- Parser must treat EOF as end-of-stream (no explicit "done" marker)
- Process output is always flushed; incremental readers will see lines appear within 500ms of completion

### Timeout and Error Handling

- If the run does not progress for 5 minutes, `--follow` emits a warning to stderr (not stdout) and continues polling
- If the database becomes unavailable, `--follow` exits with code `2` and logs the error to stderr
- If the run encounters a step_failed event, `--follow` emits the error callout and exits with code `1`

Example:
```bash
$ ail log --follow 550e8400-e29b-41d4-a716-446655440000
ail-log/1

## Turn 1 — `invocation`
...
(waits for new turns)
## Turn 2 — `analysis`
...
(process exits code 0)
```

---

## 24.5 Exit Codes

| Code | Meaning | Cause | Stderr Behavior |
|------|---------|-------|-----------------|
| `0` | Success | Run found and formatted successfully; `--follow` completed | None (or internal tracing only) |
| `1` | Run not found | `run_id` not found in database; no prior runs for CWD | Error message to stderr |
| `2` | Database error | SQLite unavailable, corrupt, or inaccessible | Error message + diagnostic to stderr |
| `3` | Invalid arguments | Malformed `run_id`, unknown flag, missing required parameter | Usage message to stderr |

---

## 24.6 Stderr and Stdout Separation

### stdout

**All ail-log output** (markdown, json, raw) goes to stdout. This includes:

- ail-log/1 version header
- Turn blocks
- Cost lines
- Error callouts
- NDJSON rows (for `--format json` and `--format raw`)

stdout is **always clean** — suitable for piping to other tools or capturing to a file.

### stderr

**All errors, warnings, and diagnostics** go to stderr:

- Run not found
- Database errors
- Permission errors
- Argument validation errors
- `--follow` timeout warnings (after 5 minutes of no progress)
- Internal tracing (when running with `RUST_LOG=debug` or similar)

The separation ensures that tools consuming stdout are not polluted by diagnostic output.

---

## 24.7 Project Resolution

The `ail log` command is **always bound to the current working directory**. A run is identified by:

1. If `run_id` is provided: look up that exact UUID in the database
2. If `run_id` is omitted: compute SHA-1 hash of the absolute CWD path, then find the most recent run for that project hash

The logic prevents cross-project leakage: `ail log` executed in directory A never shows runs from directory B, even if both directories share a database.

Implementation:
```
project_hash = sha1(canonicalize(cwd))
latest_run = SELECT run_id FROM sessions 
  WHERE project_hash = ? 
  ORDER BY started_at DESC LIMIT 1
```

---

## 24.8 Relationship to `ail logs` (Plural)

The `ail logs` command is a separate, complementary command:

| Command | Scope | Format | Use Case |
|---------|-------|--------|----------|
| `ail log` | Single run, full detail | ail-log/1 Markdown, JSON, or raw | Deep inspection of one run; live tail |
| `ail logs` | Multiple runs, summary | Table or JSON | Browse run history; find a specific run ID |

These commands coexist and serve different purposes:

- `ail logs` lists recent runs with summary info (timestamp, status, step count)
- `ail log <run_id>` shows the full formatted content of a single run

The flags, behavior, and output schemas of `ail logs` are unchanged by `ail log` and are documented separately.

---

## 24.9 Implementation Status and Roadmap

### v1 Status — Complete (Implemented)

- `ail log [run_id]` with default format (markdown)
- `--format markdown` ✓
- `--format json` ✓
- `--format raw` ✓
- `--follow` flag ✓
- Exit code handling ✓
- stderr/stdout separation ✓
- Project resolution (CWD-bound) ✓

### v2 Planned (Not Yet Implemented)

The following flags are reserved for future use and will not cause errors if passed (graceful forward compatibility):

| Flag | Semantics | Target Version |
|------|-----------|-----------------|
| `--limit <n>` | Return only the last n turns | v2 |
| `--pipeline <path>` | Filter to steps matching a specific pipeline file | v2 |

Passing these flags to v1 will result in error exit code `3` with a "flag not yet implemented" message.

### Storage v2 Schema

When the `run_events` table is added (v2), the formatter will emit additional directives:

- `:::tool-call name="X"` — individual tool invocations
- `:::tool-result name="X"` — tool results
- `:::stdio stream="stdout|stderr"` — subprocess output

The v1 specification documents these directives for forward compatibility; v1 will not emit them.

---

## 24.10 Complete Example

### No run_id provided (most recent run for CWD):

```bash
$ cd ~/projects/myapp
$ ail log
ail-log/1

## Turn 1 — `invocation`
...
```

### With explicit run_id:

```bash
$ ail log 550e8400-e29b-41d4-a716-446655440000
ail-log/1

## Turn 1 — `invocation`
...
```

### JSON format:

```bash
$ ail log --format json | head -1 | jq .
{
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "step_id": "invocation",
  "status": "completed",
  ...
}
```

### Follow mode (live tail):

```bash
$ ail log --follow
ail-log/1

## Turn 1 — `invocation`
...
(waits for next turns)
## Turn 2 — `analysis`
...
(process exits)
```

### Error cases:

```bash
$ ail log nonexistent-id
ail: run not found: nonexistent-id
exit code: 1

$ ail log --format invalid
ail: invalid format: invalid (must be markdown, json, or raw)
exit code: 3
```

---

## 24.11 Summary

| Aspect | Specification |
|--------|---------------|
| **Command** | `ail log [run_id] [--format <fmt>] [--follow]` |
| **Default format** | markdown (ail-log/1) |
| **Project scoping** | CWD-bound; SHA-1 hash of absolute path |
| **Default run_id** | Most recent for current project |
| **stdout** | Clean ail-log output only |
| **stderr** | Errors and warnings only |
| **Exit codes** | 0=success, 1=not found, 2=DB error, 3=invalid args |
| **--follow behavior** | Stream until completion; exit on final step or error |
| **Version compatibility** | Unknown flags → error; future directives silently skipped by old parsers |

---

# 25. The `ail logs` Command (Plural)

> **Implementation status:** v0.1 — fully implemented. Distinct from `ail log` (singular) — see §24.8 for the comparison.

## 25.1 Purpose

`ail logs` provides a **multi-session tabular view** of pipeline execution history. It lists sessions (pipeline runs) with their status, cost, step count, and per-step summary. It is the primary tool for browsing history and finding a specific `run_id` to pass to `ail log`.

The command queries the SQLite database at `~/.ail/projects/<sha1_of_cwd>/ail.db` (same project-scoped location as `ail log`).

---

## 25.2 Synopsis

```
ail logs [flags]
```

### Flags

| Flag | Type | Default | Semantics |
|------|------|---------|-----------|
| `--session <prefix>` | string | — | Filter sessions where `run_id LIKE 'prefix%'` |
| `--query <text>` | string | — | Full-text search across step content (uses FTS5 `traces_fts` table) |
| `--format <text\|json>` | choice | `text` | Output encoding (see §25.3) |
| `--tail` | flag | off | Poll every second and print new sessions as they appear (see §25.4) |
| `--limit <n>` | integer | `20` | Maximum number of sessions to return, ordered by `started_at DESC` |

---

## 25.3 Output Formats

### text (Default)

One block per session, ordered newest-first. Each block shows:
- A header line with timestamp, `run_id`, status, total cost, and step count
- One indented line per step with `step_id`, `event_type`, optional cost, and token counts

Example:
```
[2026-04-04 09:12:33] run_id: a1b2c3d4...  status: completed  cost: $0.0024  steps: 3
  invocation       step_completed  $0.0008  85in/42out
  review           step_completed  $0.0016  142in/91out
  summarize        step_completed
```

Timestamps are rendered as `YYYY-MM-DD HH:MM:SS` in UTC.

### json

One JSON object per session printed to stdout (NDJSON — one line per session). Each object has the following schema:

```json
{
  "run_id": "a1b2c3d4-...",
  "pipeline_source": "/path/to/.ail.yaml",
  "started_at": 1743753153000,
  "completed_at": 1743753161000,
  "total_cost_usd": 0.0024,
  "status": "completed",
  "steps": [
    {
      "step_id": "invocation",
      "event_type": "step_completed",
      "prompt": "...",
      "response": "...",
      "cost_usd": 0.0008,
      "input_tokens": 85,
      "output_tokens": 42,
      "thinking": null,
      "recorded_at": 1743753156000,
      "latency_ms": 3200
    }
  ]
}
```

`started_at`, `completed_at`, and `recorded_at` are Unix millisecond timestamps. `latency_ms` is the duration from `step_started` to `step_completed` for each step; it is `null` when either event is missing.

---

## 25.4 `--tail` Mode

When `--tail` is specified, `ail logs` does not exit. It polls the database every second and prints new sessions as they appear.

### Behavior

1. On startup, all sessions matching the current filters (up to `--limit`) are printed.
2. A high-water mark is set to the most recent `started_at` seen so far.
3. Every second, the query is re-executed. Sessions with `started_at` newer than the high-water mark are printed and the mark is updated.
4. The process runs until killed (`Ctrl-C`).

Errors during polling are printed to stderr; the loop continues.

---

## 25.5 Full-Text Search

The `--query` flag performs full-text search across step content using the SQLite FTS5 `traces_fts` table. The FTS query is passed directly to the `MATCH` operator, so FTS5 query syntax applies (e.g., `--query "refactor AND fizzbuzz"`).

Sessions are returned only if at least one of their steps matches the FTS query. The filter is applied before the `--limit` is enforced.

---

## 25.6 Project Scoping

`ail logs` is always scoped to the **current working directory**. It queries the same `~/.ail/projects/<sha1_of_cwd>/ail.db` database as `ail log`. If no database exists for the current project, an empty result is returned (no error).

---

## 25.7 Relationship to `ail log` (Singular)

| Aspect | `ail log` | `ail logs` |
|---|---|---|
| Scope | One run, full detail | Multiple runs, summary |
| Default format | `markdown` (ail-log/1) | `text` (tabular) |
| `--follow` / `--tail` | `--follow`: streams events for one run | `--tail`: polls for new sessions |
| Output unit | Turn blocks per step | One block / JSON object per session |
| Use case | Deep inspection; live tail of a specific run | Browse history; find a `run_id` |

---

