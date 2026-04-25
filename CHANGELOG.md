# Changelog

## v0.3 — in progress

### What's new

- **Parallel step execution (SPEC §29, #117)** — `async: true` marks non-blocking steps; `depends_on: [...]` declares dependencies; `action: join` synchronizes and merges branch results. String join (default, labelled headers in declaration order) and structured join (JSON merge when all deps declare `output_schema`) with optional `output_schema` validation on the join. Error modes: `fail_fast` (default) and `wait_for_all`. Pipeline-wide concurrency cap via `defaults.max_concurrency`. Uses `std::thread::scope` — no async runtime added. Complete parse-time validation (orphan detection, forward references, cycles, concurrent session conflicts, structured-join compatibility). Turn log entries tagged with `concurrent_group`, `launched_at`, `completed_at`. Template resolution for `{{ step.<join>.<dep>.<field> }}` dotted-path access.
- **Sampling parameter control (SPEC §30, #120)** — three-scope `sampling:` block (pipeline / provider / per-step) with field-level merge; temperature, top_p, top_k, max_tokens, stop_sequences, thinking; runner-specific quantization.
- **`ail init` command (SPEC §32)** — new `ail-init` workspace crate scaffolds an ail workspace from three bundled starter templates (`starter`, `superpowers`, `oh-my-ail` with alias `oma`). Everything installs under `$CWD/.ail/` — one rule that fits all three templates and keeps the user's root clean. `--force` overwrites existing files (conflict report by default); `--dry-run` prints the plan without writing. TTY-gated arrow-key picker when no template is named on a TTY; text listing fallback otherwise. Templates are embedded from `demo/<name>/` via `include_dir!` so the demo folder remains the single source of truth; a `bundled_templates_validate` test calls `ail_core::config::load` against every pipeline YAML on every CI run to catch schema drift. Richer no-args landing page surfaces `ail init` as the first-step affordance. `TemplateSource` trait reserves the seam for a future registry source without touching install / picker.
- **URL-based template source (SPEC §33)** — `ail init <URL>` installs a template fetched over HTTP. Accepts three argument shapes: `github:owner/repo[@ref][/subpath]` shorthand, `https://github.com/owner/repo[/tree/<ref>[/<subpath>]]` web URL, and `https://<host>/.../template.yaml` generic manifest URL (escape hatch for any static host, including `raw.githubusercontent.com`). GitHub forms rewrite to `raw.githubusercontent.com/<owner>/<repo>/<ref>/<subpath>/template.yaml`; the manifest plus its `files:` list drive per-file fetches with enforced caps (1 MiB manifest, 2 MiB per file, 16 MiB cumulative, 128 files, 10 s connect / 30 s read). Extends `template.yaml` with an optional `files:` field — required for URL sources, omitted for bundled (directory-walk unchanged). New `AilError::InitFailed` / `ail:init/failed` error type with stable detail prefixes (`transport:`, `not-found:`, `rate-limited:`, `manifest-invalid:`, `files-unsafe:`, `limit-exceeded:`, `url-invalid:`) used consistently by all URL-install failures. Path-traversal / absolute-path / backslash / null-byte / Windows-drive rejections happen at parse time before any file URL is formed. `--force` / `--dry-run` / conflict reporting work identically for URL and bundled sources. No online registry, no private-repo auth, and no on-disk cache in v1 (seams are reserved). Mockito-backed tests cover every happy path and error surface without touching the real network.

## v0.2 — 2026-04-05

### What works (all v0.1 features plus)

- **Transparent passthrough (US-1)** — `ail "my prompt"` works without any flags. Positional `<PROMPT>` is now the canonical invocation form. `--once <PROMPT>` is retained as a backwards-compatible long-form alias. The two forms are mutually exclusive (clap `conflicts_with`).
- **Lean output mode (US-2, default)** — After printing the final response, appends a subtle `[ail: N steps in X.Xs]` footer when stdout is a TTY and the pipeline had at least one non-invocation step. Passthrough runs (0 steps) get no footer.
- **`--show-work` summary mode (US-2)** — After execution, prints `[pipeline]` then one `✓ <step_id>  — <first sentence>` line per completed step, followed by the footer. Useful for reviewing what the pipeline did.
- **`--watch` flag** — Renamed from `--show-responses`. `--show-responses` retained as a hidden alias for backwards compatibility.
- **TUI removed** — `ratatui`, `crossterm`, and `ail/src/tui/` fully deleted. `init_tracing()` always writes structured JSON logs to stderr.
- **No-args usage hint** — Running `ail` with no prompt and no subcommand prints a short usage hint and exits 0 (previously launched the TUI).

### Still stubbed

- `skill:` and `pipeline:` step bodies abort with `PIPELINE_ABORTED`
- Interactive REPL (deferred to v0.5)
- `pause_for_human` is a no-op in headless mode

---

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
