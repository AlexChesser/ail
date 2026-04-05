# oy-my-ail — Solo Developer Quality Loop

A batteries-included, opinionated quality loop for solo developers — the AIL equivalent of oh-my-zsh.

## What this demonstrates

Where `demo/.ail.yaml` shows the simplest possible pipeline (one step, always runs), oy-my-ail shows a realistic developer workflow:

1. **Gather quality signal deterministically** — run lint and tests without spending tokens
2. **Act on it with Claude** — fix failures or simplify clean code, with full context

Features demonstrated:

| Feature | Where |
|---|---|
| `context: shell:` steps | `lint`, `tests` — runs commands, captures exit code + output |
| `on_result` with `exit_code:` | Both context steps — multi-branch first-match evaluation |
| Template variables | `{{ step.lint.exit_code }}`, `{{ step.lint.result }}`, `{{ step.tests.result }}`, `{{ step.invocation.prompt }}` |
| Multi-step pipeline | 3 steps: two context + one prompt |

## How to run

From the repo root (outside a Claude Code session):

```bash
# Step 1 — validate the pipeline
cargo run -- validate --pipeline demo/oy-my-ail.yaml

# Step 2 — single run
cargo run -- --once "add a fizzbuzz function" --pipeline demo/oy-my-ail.yaml
```

Or with the release binary:

```bash
ail validate --pipeline demo/oy-my-ail.yaml
ail --once "add a fizzbuzz function" --pipeline demo/oy-my-ail.yaml
```

## Observable evidence

- `ail validate` prints: `Pipeline valid: 3 step(s)`, exit 0
- `ail --once "..."` runs four invocations total:
  1. **invocation** — the human's prompt (add fizzbuzz)
  2. **lint** — `cargo clippy` (no token cost)
  3. **tests** — `cargo nextest run` (no token cost)
  4. **review_and_fix** — Claude receives lint + test output and either fixes failures or simplifies clean code
- `.ail/runs/<uuid>.jsonl` contains four NDJSON entries

## Differences from `dont_be_stupid`

| | `dont_be_stupid` | `oy-my-ail` |
|---|---|---|
| Steps | 1 | 3 |
| LLM calls | 1 | 1 |
| Context gathering | None | `cargo clippy` + `cargo nextest run` |
| Template variables | None | exit codes + captured output |
| Action | Always review | Fix failures, or simplify if clean |
| Token cost | Low | Same (context steps are free) |

## Known limitation

`--once` cannot be run from inside a Claude Code session (the claude CLI nested-session guard blocks it). Run from a normal terminal.
