# Changelog

## v0.1 — 2026-03-26

### What works (all v0.0.1 features plus)

- **`context: shell:` steps** — spawn `/bin/sh -c <cmd>`, capture stdout/stderr/exit_code separately, record to turn log. Non-zero exit codes are results not errors.
- **`on_result` multi-branch evaluation** — array of match arms, first-match evaluation. Match operators: `contains:`, `exit_code: <int>`, `exit_code: any` (non-zero only), `always:`. Actions: `continue`, `break`, `abort_pipeline`, `pause_for_human` (no-op).
- **`ExecuteOutcome`** — `execute()` returns `Ok(Completed)` or `Ok(Break { step_id })` on success; `Err(PIPELINE_ABORTED)` on `abort_pipeline` action.
- **Context step template variables** — `{{ step.<id>.result }}`, `{{ step.<id>.stdout }}`, `{{ step.<id>.stderr }}`, `{{ step.<id>.exit_code }}`.
- **File path resolution for `prompt:`** — if prompt starts with `./`, `../`, `~/`, or `/`, reads the file contents as the template.
- **`--headless` flag wired** — passes `--dangerously-skip-permissions` to Claude CLI. Previously parsed but ignored.
- **`ClaudeCliRunner::new(headless: bool)`** — constructor now requires headless flag.

### Still stubbed

- `skill:` and `pipeline:` step bodies abort with `PIPELINE_ABORTED`
- Interactive REPL
- `pause_for_human` is a no-op in headless mode

---

## v0.0.1 — 2026-03-07

### What works

- **`ail --once "<prompt>" --pipeline <file>`** — single non-interactive run. Executes the user's prompt through Claude, then runs every pipeline step in declaration order, resuming the same Claude session (`--resume <session_id>`) so each step has full conversation history.
- **`ail validate --pipeline <file>`** — validates a pipeline YAML file and prints the step count or a structured error. Exit 0 on valid, exit 1 on invalid.
- **`ail materialize --pipeline <file>`** — serialises the resolved pipeline back to annotated YAML with `# origin: [N] path` comments per step. Output round-trips through the parser.
- **Pipeline YAML parsing** — `version`, `pipeline[]`, `id`, `prompt`, `skill`, `pipeline`, `action` fields. Full DTO→domain validation: missing version, empty pipeline, duplicate step ids, missing primary field all return typed `AilError`.
- **Pipeline file discovery** — four-step search order: explicit `--pipeline`, `.ail.yaml` in CWD, `.ail/default.yaml` in CWD, `~/.config/ail/default.yaml`.
- **Template variable resolution** — `{{ step.invocation.prompt }}`, `{{ last_response }}`, `{{ step.<id>.response }}`, `{{ pipeline.run_id }}`, `{{ session.tool }}`, `{{ session.cwd }}`, `{{ env.<VAR> }}`. Unresolved variables abort with a typed error — never silently empty.
- **Turn log** — append-only NDJSON audit trail written to `.ail/runs/<run_id>.jsonl` after each step.
- **Structured logging** — all log output is JSON via `tracing-subscriber`. No unstructured `eprintln!` in `ail-core`.
- **RFC 9457-inspired error types** — every error has a stable `error_type` string constant, a human-readable `title`, and an instance-specific `detail`.

### Explicitly stubbed (entry points exist, no implementation behind them)

- `--headless` flag parses but has no effect beyond suppressing the invocation response print in `--once` mode. A future phase adds headless NDJSON output to stdout.
- `action: pause_for_human` in the executor is a no-op in `--once` mode.
- `skill:` and `pipeline:` step bodies abort with `PIPELINE_ABORTED` — stub only.
- Interactive REPL (`ail` with no flags) prints "not yet implemented" to stderr.

### Explicitly deferred to later versions (SPEC §22)

- Pipeline inheritance (`extends:` field) — SPEC §6
- Step conditions (`condition: always/never/expr`) — SPEC §12
- `on_result` rules (`contains:`, `abort_pipeline`, `break`, `pause_for_human`) — SPEC §5.3
- Tool permission control (`tool_permissions:`) — SPEC §9
- Model override per step (`model:`) — SPEC §10
- Skill execution (`skill:` step body) — SPEC §15
- Sub-pipeline execution (`pipeline:` step body) — SPEC §15
- TUI / interactive REPL
- Server mode and SDK generation — ARCHITECTURE §12

### Open questions surfaced during build (see LEARNINGS.md for detail)

- **[UNDOC]** `--output-format stream-json` requires `--verbose` when used with `-p`. RUNNER-SPEC.md does not document this.
- **[UNDOC]** `clippy::result_large_err` fires on all `Result<_, AilError>` returns because `AilError` contains `String`. Suppressed with `#[allow]` in four modules. Future decision: box `AilError` at call site or restructure `detail: Box<str>`.
- **[SPEC]** §4.4 requires the NDJSON audit log to be persisted before the next step runs. Current implementation attempts persistence but continues with a warning on write failure.
- **[SPEC]** §18 origin comment format (`# origin: [N] path`) is not prescribed — implementation choice. May need updating if SPEC formalises it.
- **[ARCH]** Integration tests for `ClaudeCliRunner` cannot run inside a Claude Code session (nested-session guard). Annotated `#[ignore]` — CI needs to handle this separately.
