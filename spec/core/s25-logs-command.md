## 25. The `ail logs` Command (Plural)

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
