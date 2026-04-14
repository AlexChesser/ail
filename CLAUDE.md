# AIL — Artificial Intelligence Loops

**ail** is a YAML-orchestrated pipeline runtime that fires a declared sequence of
automated steps after every human prompt — before control returns to the user.
Steps run in order; individual steps may be skipped or exit early via declared outcomes.
It is the control plane for agent behaviour after the human stops typing.

## Workspace Layout

```
ail/                        # binary crate — CLI entry point only
  src/main.rs               # --once, materialize, validate handlers
  src/cli.rs                # Cli, Commands (clap derive)
  src/command.rs            # CommandOutcome — command lifecycle types
ail-core/                   # library crate — all logic, no UI
  src/
    config/                 # discovery, dto, domain, validation, mod (load())
    error.rs                # AilError, ErrorContext, error_types constants
    executor.rs             # execute() — SPEC §4.2 core invariant
    materialize.rs          # materialize() — annotated YAML with # origin comments
    runner/                 # Runner trait, StubRunner, ClaudeCliRunner
      plugin/               # runtime plugin system — JSON-RPC protocol, discovery, ProtocolRunner
    session/                # Session, TurnLog, TurnEntry
    template.rs             # resolve() — {{ variable }} syntax
  tests/spec/               # spec-coverage integration tests, one file per SPEC section
  tests/fixtures/           # minimal, solo_developer, invalid_* YAML fixtures
demo/                       # working demo pipeline (.ail.yaml + README)
spec/                       # split spec files (primary published artifacts)
  core/s*.md                # AIL Pipeline Language Specification (one file per section)
  runner/r*.md              # Claude CLI runner contract (one file per section)
  README.md                 # navigation index — start here
SPEC.md                     # redirect stub → spec/core/
RUNNER-SPEC.md              # redirect stub → spec/runner/
ARCHITECTURE.md             # design rationale and principles
CHANGELOG.md                # v0.0.1 feature record and open questions
```

**Hard rule:** `ail-core` never imports from `ail`. The compiler enforces this boundary.

## Common Commands

```bash
# Build
cargo build
cargo build --release

# Test (preferred runner)
cargo nextest run

# Lint — must be clean before committing
cargo clippy -- -D warnings

# Format check
cargo fmt --check

# Run the demo (requires release build and claude CLI)
cd demo && ../target/release/ail "add a fizzbuzz function" --pipeline .ail.yaml

# Validate a pipeline file
cargo run -- validate --pipeline demo/.ail.yaml

# Inspect resolved pipeline YAML
cargo run -- materialize --pipeline demo/.ail.yaml

# Run with NDJSON event stream (for programmatic consumers)
cargo run -- --once "hello" --pipeline demo/.ail.yaml --output-format json

# Dry-run mode: full pipeline resolution without LLM calls
cargo run -- --dry-run "hello" --pipeline demo/.ail.yaml
```

## SPEC is the Contract — Always Audit on Functional Changes

The `spec/` folder is the **primary published artifact** of this project. `SPEC.md` and `RUNNER-SPEC.md` are redirect stubs — the live spec lives in `spec/core/s*.md` and `spec/runner/r*.md`. The navigation index is `spec/README.md`. These specs are not aspirational documentation — they are a rigorous, real-world-tested contract. The implementation exists to prove the spec is correct and achievable, not the other way around.

The author has already found places where the spec described the **opposite** of what actually works. Every spec change is therefore a correction of real knowledge, not a preference.

**Whenever you make a materially functional change to behavior, you MUST:**

1. Identify which spec sections are affected (check section numbers against the change; find files via `spec/README.md`)
2. Update the affected `spec/core/s*.md` and/or `spec/runner/r*.md` files to reflect the actual behavior
3. Explicitly flag — in your response — any case where the spec previously described behavior incorrectly
4. Update `ail-core/CLAUDE.md` if module responsibilities or key types change

"Materially functional" means: any change to how steps execute, how sessions are stored, how runners are invoked, what events are written to the turn log, how template variables resolve, how tool permissions are passed, or how the pipeline is validated. Adding a field, renaming a behavior, changing a path, changing a default — all qualify.

When in doubt, update the spec. A spec that accurately describes what is built is the whole point.

## Architecture (Summary — see ARCHITECTURE.md for full rationale)

`ail` is a **control plane**, not a tool. The core design decisions:

- **Why Rust**: steady-state RSS of 2–5MB vs 80–120MB for Node. At 10k concurrent sessions the delta is ~$100k/year in infrastructure cost. This is the primary rationale — not preference.
- **Two-crate rule**: `ail-core` (library, no UI) and `ail` (binary). `ail` depends on `ail-core`. The inverse is a compile error. All correctness lives in `ail-core`.
- **DTO→Domain boundary**: `dto.rs` (serde, `Deserialize`) → `validation.rs` (typed errors) → `domain.rs` (no serde). Serde structs never become domain objects.
- **Runner trait is the seam**: `ClaudeCliRunner` is an implementation detail. The executor sees only `&dyn Runner`. New runners don't touch the executor. Third-party runners can be added at runtime via the plugin system (JSON-RPC over stdin/stdout).
- **Stream parsing isolation**: all NDJSON parsing from Claude CLI lives in `runner/claude.rs`. Nothing else touches raw JSON. When Anthropic changes the wire format, the blast radius is one file.
- **RFC 9457-inspired errors**: `AilError { error_type (stable const), title, detail, context }`. No `unwrap()`/`panic` in production paths.
- **Observability from day one**: `tracing` spans and structured fields, never `println!`. The turn log is the durable audit trail; tracing is the live signal.

See `ARCHITECTURE.md` for the full rationale including the 15-factor design table, SOLID application, and the server mode roadmap.

## Core Concepts

| Term | Definition |
|---|---|
| **pipeline** | Ordered sequence of steps in a `.ail.yaml` file |
| **step** | Single unit: prompt, skill, sub-pipeline, or action |
| **invocation** | Implicit first step — the human's triggering prompt |
| **session** | One running instance of an underlying agent (e.g. Claude Code) |
| **runner** | Adapter that calls the underlying agent (`ClaudeCliRunner`, `StubRunner`) |
| **turn log** | Append-only NDJSON audit trail at `~/.ail/projects/<sha1_of_cwd>/runs/<run_id>.jsonl` |
| **passthrough mode** | No `.ail.yaml` found — ail is transparent, pipeline = invocation only |

## Pipeline File Discovery Order (SPEC §3.1)

1. Explicit `--pipeline <path>` flag
2. `.ail.yaml` in CWD
3. `.ail/default.yaml` in CWD
4. `~/.config/ail/default.yaml`

If nothing found → passthrough mode (safe zero-config default).

## --once Flow

1. Discover and load pipeline (or passthrough).
2. If no `invocation` step declared: run user prompt via `runner.invoke()`, append `TurnEntry(step_id="invocation")`, store `runner_session_id`.
3. Call `executor::execute()` for all declared steps; steps run isolated by default — set `resume: true` on a step to resume the prior session.
4. Print invocation response, then last non-invocation step response.

## Template Variables (spec/core/s11)

| Variable | Resolves to |
|---|---|
| `{{ step.invocation.prompt }}` | The original user prompt |
| `{{ step.invocation.response }}` | The runner's response before any pipeline steps ran |
| `{{ step.<id>.response }}` | Response from a named `prompt:` step |
| `{{ step.<id>.result }}` | Output of a `context:` step (stdout+stderr for `shell:`, tool output for `mcp:`) |
| `{{ step.<id>.stdout }}` | stdout of a `shell:` context step |
| `{{ step.<id>.stderr }}` | stderr of a `shell:` context step |
| `{{ step.<id>.exit_code }}` | Exit code of a `shell:` context step (string) |
| `{{ step.<id>.items }}` | JSON array items from a step with `output_schema: type: array` (SPEC §26) |
| `{{ step.<id>.modified }}` | Human-modified output from a `modify_output` HITL gate (SPEC §13.2) |
| `{{ last_response }}` | Most recent step response |
| `{{ pipeline.run_id }}` | UUID for this run |
| `{{ session.tool }}` | Runner name (e.g. `claude`) |
| `{{ session.cwd }}` | Working directory |
| `{{ env.<VAR> }}` | Environment variable |
| `{{ do_while.iteration }}` | Current 0-based iteration index (only inside `do_while:` body) |
| `{{ do_while.max_iterations }}` | Declared `max_iterations` value (only inside `do_while:` body) |
| `{{ step.<loop_id>::<step_id>.* }}` | Qualified reference to a do_while inner step from outside the loop |
| `{{ step.<loop_id>.index }}` | Number of iterations completed by a do_while loop (after loop exits) |
| `{{ step.<loop_id>::do_while[N].<step_id>.* }}` | Indexed iteration access — specific iteration's inner step (not yet implemented) |
| `{{ for_each.item }}` | Current item value (default name — used when `as:` is not set; always available) |
| `{{ for_each.<as_name> }}` | Current item value under the declared `as:` name (e.g. `{{ for_each.task }}` when `as: task`) |
| `{{ for_each.index }}` | Current 1-based item index (only inside `for_each:` body) |
| `{{ for_each.total }}` | Total number of items in the collection, after `max_items` cap (only inside `for_each:` body) |

Note: `{{ session.invocation_prompt }}` is a supported alias for `{{ step.invocation.prompt }}` in the implementation but is deprecated — prefer the canonical form.

Unresolved variables **abort with a typed error** — never silently empty.

## Code Conventions

- No `unwrap()`/`expect()` outside tests
- No `println!`/`eprintln!` in `ail-core` — use `tracing::{info, warn, error}`
- **Use `..Default::default()` for struct construction** — when building `Step`, `TurnEntry`, or other structs with many optional/defaultable fields, set only the fields that differ from the default. Never enumerate every field with `None`/`0`/`vec![]` explicitly.
- `dto.rs` derives `Deserialize`; `domain.rs` does not — conversion in `validation.rs`
- `#[allow(clippy::result_large_err)]` required in every module that returns `Result<_, AilError>`. Apply at file scope (`#![allow(...)]`). Current files: `config/{mod,inheritance,validation/mod,validation/step_body,validation/on_result,validation/system_prompt}.rs`, `template.rs`, `executor/{core,headless,controlled}.rs`, `executor/helpers/{invocation,runner_resolution,shell,system_prompt,condition}.rs`, `executor/dispatch/{prompt,context,skill,sub_pipeline}.rs`, `runner/{mod,factory,http,subprocess,claude/mod,claude/permission}.rs`, `runner/plugin/{mod,validation,discovery,protocol_runner}.rs`, `skill.rs`, `delete.rs`, `fs_util.rs`, `logs.rs`, `formatter.rs`
- All errors use `AilError` with a stable `error_type` string constant from `error::error_types`
- No co-authorship lines in git commits

## Test Organisation

- `ail-core/tests/spec/s<NN>_<name>.rs` — one file per SPEC section
- `ail-core/tests/fixtures/` — YAML test configs
- `ClaudeCliRunner` integration tests are `#[ignore]` — cannot run inside a Claude Code session (nested-session guard). CI must run them separately with `--include-ignored`.

## Known Constraints (v0.3)

- `--output-format stream-json` requires `--verbose` with `-p` — documented in `spec/runner/r02-claude-cli.md`
- Must call `.env_remove("CLAUDECODE")` on the `Command` builder to avoid nested session guard
- `pause_for_human` is a no-op in `--once` / headless mode; `modify_output` behavior is configurable via `on_headless` (skip/abort/use_default)
- `skill:` steps are implemented with a built-in registry (§6, §14); skill parameterisation is deferred
- `pipeline:` step bodies support both file-based sub-pipelines and named pipeline references (SPEC §9, §10)
- `do_while:` fully implemented (§27): parse-time validation, executor loop, template vars, step ID namespacing, break/abort_pipeline, shared depth guard (MAX_LOOP_DEPTH=8). `on_max_iterations` field defaults to `abort_pipeline` (configurable variant not yet implemented). Controlled-mode executor events deferred.
- `for_each:` fully implemented (§28): parse-time validation, runtime array iteration, item scope, template vars, break/abort_pipeline, max_items cap, shared depth guard with do_while. Controlled-mode executor events deferred.
- `output_schema` / `input_schema` (§26): JSON Schema validation at parse time and runtime. `schema-as-file-path` variant (§26.1) not yet implemented — schemas must be inline.
- `do_while[N]` indexed iteration access (§27.4) is specified but not implemented — template resolver only exposes the final iteration
- `pipeline:` as alternative to inline `steps:` is supported in both `do_while:` and `for_each:` loop bodies
- Interactive REPL deferred to v0.5
- TUI removed in v0.2; all output goes to stdout/stderr
- `ClaudeCliRunner::new(headless: bool)` — pass `true` for `--headless` mode (`--dangerously-skip-permissions`)
