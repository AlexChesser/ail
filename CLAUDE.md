# AIL — Artificial Intelligence Loops

**ail** is a YAML-orchestrated pipeline runtime that fires a deterministic chain of
automated prompts after every human prompt — before control returns to the user.
It is the control plane for agent behaviour after the human stops typing.

## Workspace Layout

```
ail/                        # binary crate — CLI entry point only
  src/main.rs               # --once, materialize, validate handlers
  src/cli.rs                # Cli, Commands (clap derive)
ail-core/                   # library crate — all logic, no UI
  src/
    config/                 # discovery, dto, domain, validation, mod (load())
    error.rs                # AilError, ErrorContext, error_types constants
    executor.rs             # execute() — SPEC §4.2 core invariant
    materialize.rs          # materialize() — annotated YAML with # origin comments
    runner/                 # Runner trait, StubRunner, ClaudeCliRunner
    session/                # Session, TurnLog, TurnEntry
    template.rs             # resolve() — {{ variable }} syntax
  tests/spec/               # spec-coverage integration tests, one file per SPEC section
  tests/fixtures/           # minimal, solo_developer, invalid_* YAML fixtures
demo/                       # working demo pipeline (.ail.yaml + README)
SPEC.md                     # AIL Pipeline Language Specification (authoritative)
RUNNER-SPEC.md              # Claude CLI runner contract
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
cd demo && ../target/release/ail --once "add a fizzbuzz function" --pipeline .ail.yaml

# Validate a pipeline file
cargo run -- validate --pipeline demo/.ail.yaml

# Inspect resolved pipeline YAML
cargo run -- materialize --pipeline demo/.ail.yaml
```

## SPEC is the Contract — Always Audit on Functional Changes

`SPEC.md` and `RUNNER-SPEC.md` are the **primary published artifacts** of this project. They are not aspirational documentation — they are a rigorous, real-world-tested contract. The implementation exists to prove the spec is correct and achievable, not the other way around.

The author has already found places where the spec described the **opposite** of what actually works. Every spec change is therefore a correction of real knowledge, not a preference.

**Whenever you make a materially functional change to behavior, you MUST:**

1. Identify which SPEC.md sections are affected (check section numbers against the change)
2. Update SPEC.md and/or RUNNER-SPEC.md to reflect the actual behavior
3. Explicitly flag — in your response — any case where the spec previously described behavior incorrectly
4. Update `ail-core/CLAUDE.md` if module responsibilities or key types change

"Materially functional" means: any change to how steps execute, how sessions are stored, how runners are invoked, what events are written to the turn log, how template variables resolve, how tool permissions are passed, or how the pipeline is validated. Adding a field, renaming a behavior, changing a path, changing a default — all qualify.

When in doubt, update the spec. A spec that accurately describes what is built is the whole point.

## Architecture (Summary — see ARCHITECTURE.md for full rationale)

`ail` is a **control plane**, not a tool. The core design decisions:

- **Why Rust**: steady-state RSS of 2–5MB vs 80–120MB for Node. At 10k concurrent sessions the delta is ~$100k/year in infrastructure cost. This is the primary rationale — not preference.
- **Two-crate rule**: `ail-core` (library, no UI) and `ail` (binary). `ail` depends on `ail-core`. The inverse is a compile error. All correctness lives in `ail-core`.
- **DTO→Domain boundary**: `dto.rs` (serde, `Deserialize`) → `validation.rs` (typed errors) → `domain.rs` (no serde). Serde structs never become domain objects.
- **Runner trait is the seam**: `ClaudeCliRunner` is an implementation detail. The executor sees only `&dyn Runner`. New runners don't touch the executor.
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
3. Call `executor::execute()` for all declared steps; each step resumes via `--resume <last_runner_session_id>`.
4. Print invocation response, then last non-invocation step response.

## Template Variables (SPEC §11)

| Variable | Resolves to |
|---|---|
| `{{ session.invocation_prompt }}` | The original user prompt |
| `{{ step.<id>.response }}` | Response from a named step |
| `{{ last_response }}` | Most recent step response |
| `{{ pipeline.run_id }}` | UUID for this run |
| `{{ session.cwd }}` | Working directory |
| `{{ env.<VAR> }}` | Environment variable |

Unresolved variables **abort with a typed error** — never silently empty.

## Code Conventions

- No `unwrap()`/`expect()` outside tests
- No `println!`/`eprintln!` in `ail-core` — use `tracing::{info, warn, error}`
- `dto.rs` derives `Deserialize`; `domain.rs` does not — conversion in `validation.rs`
- `#[allow(clippy::result_large_err)]` required in: `validation.rs`, `config/mod.rs`, `runner/mod.rs`, `template.rs`, `executor.rs`
- All errors use `AilError` with a stable `error_type` string constant from `error::error_types`
- No co-authorship lines in git commits

## Test Organisation

- `ail-core/tests/spec/s<NN>_<name>.rs` — one file per SPEC section
- `ail-core/tests/fixtures/` — YAML test configs
- `ClaudeCliRunner` integration tests are `#[ignore]` — cannot run inside a Claude Code session (nested-session guard). CI must run them separately with `--include-ignored`.

## Known Constraints (v0.0.1)

- `--output-format stream-json` requires `--verbose` with `-p` (undocumented in RUNNER-SPEC.md)
- Must call `.env_remove("CLAUDECODE")` on the `Command` builder to avoid nested session guard
- `pause_for_human` is a no-op in `--once` mode
- `skill:` and `pipeline:` step bodies abort with `PIPELINE_ABORTED` (stubs)
- Interactive REPL not implemented
