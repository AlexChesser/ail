# AIL v0.0.1 Build Prompt

---

## Read Before Writing Any Code

Three documents govern every decision in this build. Read them in full before
doing anything else. When a decision is ambiguous, return to them. When this
prompt and the documents conflict, the documents take precedence — flag the
conflict rather than silently resolving it.

- **SPEC.md** — the pipeline language. What is being built.
- **ARCHITECTURE.md** — implementation principles and constraints. How to build it.
- **RUNNER-SPEC.md** — the Claude CLI runner contract. The external interface
  being wrapped.

**Before starting Phase 1, confirm you have read all three.** State what the
v0.0.1 demo target is (SPEC §21) and what the core execution invariant is
(SPEC §4.2) in your own words. This is not a recitation exercise — it
establishes that the documents were read and understood before code was written.

---

## What Is Being Built

`ail` is a Rust CLI tool — a control plane that wraps an agentic coding tool
(Claude CLI in v0.0.1) and ensures a deterministic pipeline of follow-up
prompts runs after every agent response, before control returns to the human.

The core invariant (SPEC §4.2):
> For every completion event produced by the underlying agent, the pipeline
> defined in the active `.ail.yaml` file must execute in full, in order,
> before control returns to the human.

v0.0.1 is a proof of concept — not a production system, not a full
implementation of the spec. The definition of done for v0.0.1 is this demo
pipeline working end-to-end:

```yaml
version: "0.0.1"
pipeline:
  - id: dont_be_stupid
    prompt: "Review the above output. Fix anything obviously wrong or
             unnecessarily complex."
```

Two Claude invocations. One `.ail.yaml` file. Visibly working. Nothing more.

---

## Non-Negotiable Constraints

These come directly from ARCHITECTURE.md. They apply from the first line of
code. Do not defer them.

**Two-crate structure.** `ail-core/` contains domain types, the pipeline
executor, and runner adapters. No TUI, no HTTP, no UI of any kind. `ail/`
contains the binary entry point, CLI argument parsing, and TUI. `ail` depends
on `ail-core`. `ail-core` never imports from `ail`. This is a compiler
boundary, not a convention.

**No `unwrap()` or `expect()` in production code paths.** Every failure is
a typed `Result<T, AilError>`. Panics belong only in test code and genuine
programming-error invariants, with a comment explaining why the invariant holds.

**Structured logging from day one.** Use the `tracing` crate. Never `println!`
or `eprintln!` in production code. Log statements use structured fields, not
format strings.

**YAML structs are not domain objects.** Serde structs are DTOs that live only
at the parsing boundary. They are immediately validated and transformed into
domain types. Nothing in the domain layer derives `Deserialize`. This boundary
must be structurally visible in the code: `dto.rs` derives `Deserialize`,
`domain.rs` does not, transformation lives in `validation.rs`.

**Agent-first CLI surface.** Every interaction must have a non-interactive
equivalent. `--once "<prompt>"` is required from v0.0.1.

**Test naming encodes spec references.** Every test in
`ail-core/tests/spec_coverage.rs` uses the module hierarchy:
`mod spec { mod s<N>_<name> { fn <feature_description>() {} } }`.
A reader must be able to map any test directly to a SPEC section by reading
the module path alone.

**File size is a signal.** When a `.rs` file exceeds ~200 lines, consider
whether it should be split. Small, readable files are a design goal.

---

## How to Work Through the Phases

### The rhythm between phases

Each phase ends with the same four-step sequence. Do not skip any of them.

**Step 1 — Tests pass.**
```bash
cargo nextest run
cargo clippy -- -D warnings
cargo fmt --check
```
Fix all failures before continuing.

**Step 2 — Append a reflection block to `LEARNINGS.md`.**
Create this file in Phase 1 if it does not exist. Each block uses this format:

```markdown
## Phase N — <phase title>

### Discoveries not covered by the reference documents
- ...

### Assumptions that proved wrong
- ...

### Decisions made that future phases should know about
- ...

### Flags for human review
- [SPEC] <anything suggesting SPEC.md needs updating>
- [ARCH] <anything suggesting ARCHITECTURE.md needs updating>
- [UNDOC] <any decision not covered by either document>
```

If nothing belongs in a section, write "None." Do not omit sections.

**Step 3 — Commit.**
Format: `phase-N: <what this phase delivers>`
Every commit must build cleanly. No WIP commits.

**Step 4 — State the phase exit condition explicitly.**
Before beginning the next phase, write:
- "Phase N complete."
- The exact verification command run and its output.
- Any [SPEC], [ARCH], or [UNDOC] flags raised, and whether they were
  resolved or need human review.

### When something unexpected happens

Stop. Do not silently work around it. Document it in `LEARNINGS.md` under
the current phase before continuing. If the discovery changes what you are
about to build, say so explicitly.

This is a zero-to-one build based on informed speculation. Unknown unknowns
are expected. The reflection steps exist to surface them before they become
structural debt.

### When the spec and reality conflict

The spec describes what `ail` should eventually do. When something cannot be
implemented as described, that is a finding to surface — not a reason to
build something different without comment. Flag it `[SPEC]` and implement the
closest correct version, documenting the divergence.

---

## Phases

Work through phases in strict dependency order. State the plan for each phase
before writing any code for it.

---

### Phase 1 — Workspace Skeleton

**What it delivers:** A compiling, running Rust workspace.

**Why first:** Everything else depends on a buildable workspace with the
two-crate structure established from the start. Retrofitting this boundary
later is painful and error-prone.

**Tasks:**

Create a Cargo workspace at the repository root with two members: `ail-core`
and `ail`.

`ail-core` — library crate:
- `src/lib.rs` exports one function: `pub fn version() -> &'static str`
  returning `"0.0.1"`.
- Add `tracing` as a dependency.

`ail` — binary crate:
- Depends on `ail-core`.
- Uses `clap` (derive API) for argument parsing.
- `--version`: print `ail 0.0.1`, exit 0.
- No arguments: print `ail 0.0.1`, exit 0. (Interactive REPL is out of
  v0.0.1 scope, but the entry point must exist as a clean stub.)
- Wire `tracing-subscriber` in `main()` to emit structured JSON to stdout.
- Emit one `tracing::info!` event on startup: `event = "startup"`.

Create `ail-core/tests/spec_coverage.rs`:
```rust
mod spec {
    mod s21_mvp {
        /// SPEC §21 — the v0.0.1 binary compiles and runs
        #[test]
        fn binary_compiles() {
            // Structural test: if this file compiles, the crate structure
            // is correct. Observable verification is manual in Phase 1.
        }
    }
}
```

Create `LEARNINGS.md` at the repository root with a Phase 1 reflection block.

**Observable verification:**
```bash
cargo build --release
./target/release/ail --version
# expected: ail 0.0.1

cargo nextest run
# expected: all tests pass
```

**Commit:** `phase-1: workspace skeleton, two-crate structure, structured logging`

---

### Phase 2 — Error Type Foundation

**What it delivers:** `AilError` — the typed error type that all subsequent
fallible code depends on.

**Why now:** Establishing the error type before writing any fallible logic
prevents retrofitting it across existing code later. Every subsequent phase
writes fallible code. This must exist first.

**Tasks:**

In `ail-core/src/error.rs`, define `AilError` (ARCHITECTURE.md §4,
RFC 9457 inspired):

```rust
pub struct AilError {
    pub error_type: &'static str,  // namespaced stable string
    pub title: &'static str,       // human-readable category
    pub detail: String,            // instance-specific description
    pub context: Option<ErrorContext>,
}

pub struct ErrorContext {
    pub pipeline_run_id: Option<String>,
    pub step_id: Option<String>,
    pub source: Option<String>,    // stringified source error
}
```

Implement:
- `std::fmt::Display` — single-line: `[{error_type}] {title}: {detail}`
- `std::error::Error`
- `pub mod error_types` constants module:
  ```rust
  pub const CONFIG_INVALID_YAML: &str       = "ail:config/invalid-yaml";
  pub const CONFIG_FILE_NOT_FOUND: &str     = "ail:config/file-not-found";
  pub const CONFIG_VALIDATION_FAILED: &str  = "ail:config/validation-failed";
  pub const TEMPLATE_UNRESOLVED: &str       = "ail:template/unresolved-variable";
  pub const RUNNER_INVOCATION_FAILED: &str  = "ail:runner/invocation-failed";
  pub const PIPELINE_ABORTED: &str          = "ail:pipeline/aborted";
  ```

Add unit tests in `error.rs` under `#[cfg(test)]`:
- Display output contains `error_type`.
- Display output contains the `detail` string.
- `ErrorContext` being `None` does not affect Display.

Add to `spec_coverage.rs`:
```rust
mod s17_error_handling {
    /// SPEC §17 — errors carry a stable type string and instance detail
    #[test]
    fn ail_error_display_contains_type_and_detail() { ... }
}
```

**Observable verification:**
```bash
cargo nextest run
# all tests pass including error tests
```

**Commit:** `phase-2: AilError, RFC 9457 inspired, error_type constants`

---

### Phase 3 — CLI Argument Surface

**What it delivers:** The complete v0.0.1 CLI interface. All commands exist;
most are clean stubs.

**Why now:** Establishing the full CLI surface before implementing any command
is cheaper than refactoring argument structure after commands have real logic
behind them. The surface also documents intent clearly.

**Tasks:**

Define the argument structure in `ail/src/cli.rs` using clap derive macros.
Wire into `main.rs`. All unimplemented commands emit a `tracing::info!` event
and print a one-line message to stderr. No panics.

v0.0.1 CLI surface:

| Invocation | Behaviour |
|---|---|
| `ail` | REPL mode stub |
| `ail --once "<prompt>"` | single-turn non-interactive |
| `ail --pipeline <path>` | explicit pipeline file override |
| `ail --headless` | disable TUI, output structured JSON |
| `ail --version` | print version, exit 0 |
| `ail materialize-chain` | subcommand stub |
| `ail materialize-chain --out <path>` | write output to file |
| `ail validate` | subcommand stub |
| `ail validate --pipeline <path>` | validate a specific file |

`--help` output must describe each flag accurately using SPEC language where
applicable.

Add unit tests in `cli.rs` under `#[cfg(test)]` using clap's built-in test
utilities, verifying that each flag and subcommand parses correctly.

**Observable verification:**
```bash
ail --help
# all flags visible, descriptions accurate

ail --once "hello"
# stub message, exit 0

ail materialize-chain
# stub message, exit 0

cargo nextest run
# all tests pass
```

**Commit:** `phase-3: full v0.0.1 CLI surface, all commands stubbed`

---

### Phase 4 — Pipeline Discovery and Parsing

**What it delivers:** `ail` can locate, parse, and validate a `.ail.yaml`
file into typed domain objects.

**Why now:** `materialize-chain` and `validate` both need to load a pipeline.
The DTO-to-domain boundary established here also sets the structural pattern
that every subsequent layer follows.

**Tasks:**

Create `ail-core/src/config/` with these modules:

`discovery.rs` — implements the four-step discovery order from SPEC §3.1:
1. Explicit path (passed as `Option<PathBuf>` from `--pipeline` flag)
2. `.ail.yaml` in the current working directory
3. `.ail/default.yaml` in the current working directory
4. `~/.config/ail/default.yaml`

Returns `Option<PathBuf>`. Locates only — does not read.

`dto.rs` — serde structs for raw YAML. These are DTOs only. They derive
`Deserialize` and nothing else domain-meaningful. They represent the file's
shape, not the domain's shape.

`domain.rs` — domain types. These derive nothing from serde. Minimum set:
- `Pipeline` — contains `Vec<Step>` and source metadata (`PathBuf`)
- `Step` — contains `StepId`, `StepBody`, and optional fields for Phase 4
- `StepId` — newtype over `String`
- `StepBody` — enum: `Prompt(String)` | `Skill(PathBuf)` |
  `SubPipeline(PathBuf)` | `Action(ActionKind)`
- `ActionKind` — enum: `PauseForHuman`
- `Pipeline::passthrough() -> Pipeline` — zero-step pipeline, the safe
  default when no `.ail.yaml` file is found (SPEC §3.1)

`validation.rs` — `validate(dto: PipelineFileDto) -> Result<Pipeline, AilError>`

Validates:
- `version` field is present and non-empty
- `pipeline` array is non-empty
- step IDs are unique within the file
- each step has exactly one primary field (`prompt`, `skill`, `pipeline`,
  or `action`)

`mod.rs` — public API:
- `load(path: &Path) -> Result<Pipeline, AilError>`
- `discover(explicit: Option<PathBuf>) -> Option<PathBuf>`

The DTO-to-domain boundary must be structurally visible: `dto.rs` imports
serde, `domain.rs` does not, transformation happens only in `validation.rs`.

Wire `ail validate`:
- Discover and load pipeline.
- Print `Pipeline valid: N step(s)` on success, exit 0.
- Print structured error on failure, exit non-zero.

**Test fixtures.** Create `ail-core/tests/fixtures/`:
- `minimal.ail.yaml` — the SPEC §21 demo pipeline
- `solo_developer.ail.yaml` — the SPEC §19.2 example
- `invalid_no_version.ail.yaml`
- `invalid_empty_pipeline.ail.yaml`
- `invalid_duplicate_ids.ail.yaml`
- `invalid_no_primary_field.ail.yaml`

Add to `spec_coverage.rs`:
```rust
mod s3_file_format {
    mod s3_1_discovery {
        #[test] fn explicit_path_takes_precedence() { ... }
        #[test] fn falls_back_to_ail_yaml_in_cwd() { ... }
        #[test] fn returns_none_when_no_file_found() { ... }
    }
    mod s3_2_top_level_structure {
        #[test] fn minimal_pipeline_parses_to_domain_type() { ... }
        #[test] fn missing_version_returns_validation_error() { ... }
        #[test] fn empty_pipeline_returns_validation_error() { ... }
    }
}
mod s5_step_specification {
    mod s5_1_core_fields {
        #[test] fn prompt_field_parses_to_prompt_body() { ... }
        #[test] fn step_id_is_newtype_not_raw_string() { ... }
        #[test] fn duplicate_step_ids_return_validation_error() { ... }
        #[test] fn step_with_no_primary_field_is_invalid() { ... }
    }
}
```

**Observable verification:**
```bash
ail validate --pipeline ail-core/tests/fixtures/minimal.ail.yaml
# expected: "Pipeline valid: 1 step(s)", exit 0

ail validate --pipeline ail-core/tests/fixtures/invalid_no_version.ail.yaml
# expected: structured error, exit non-zero

cargo nextest run
# all tests pass
```

**Commit:** `phase-4: pipeline discovery, YAML parsing, DTO→domain validation`

---

### Phase 5 — `materialize-chain` Command

**What it delivers:** `ail materialize-chain` produces annotated YAML output
for a single-file pipeline.

**Why now:** This command depends only on the Phase 4 parser — no runner, no
session, no execution. Building it now validates that the domain model
round-trips correctly before any execution logic exists. It also becomes the
primary debugging tool (SPEC §18) for all subsequent phases.

**Tasks:**

In `ail-core/src/materialize.rs`:

```rust
pub fn materialize(pipeline: &Pipeline) -> String
```

For Phase 5 (single file, no inheritance), output is the pipeline serialised
back to YAML with each step annotated with an origin comment:
```yaml
# origin: [1] path/to/file.ail.yaml
- id: dont_be_stupid
  prompt: "..."
```

The round-trip property must hold: output from `materialize()` must parse
cleanly through `load()` (comments are ignored by the YAML parser).

Wire `ail materialize-chain`:
1. Discover and load pipeline.
2. Call `materialize()`.
3. Write to stdout, or to `--out <path>` if provided.

Add to `spec_coverage.rs`:
```rust
mod s18_materialize_chain {
    /// SPEC §18 — output includes origin annotation per step
    #[test]
    fn single_file_pipeline_output_has_origin_comment() { ... }

    /// SPEC §18 — output is valid parseable YAML
    #[test]
    fn materialized_output_is_valid_yaml() { ... }

    /// SPEC §18 — round-trip: materialize → parse → materialize is stable
    #[test]
    fn materialized_output_round_trips_through_parser() { ... }
}
```

**Observable verification:**
```bash
ail materialize-chain \
  --pipeline ail-core/tests/fixtures/minimal.ail.yaml
# valid annotated YAML to stdout, exit 0

ail materialize-chain \
  --pipeline ail-core/tests/fixtures/minimal.ail.yaml \
  --out /tmp/materialized.yaml

# Round-trip check:
ail validate --pipeline /tmp/materialized.yaml
# expected: "Pipeline valid: 1 step(s)", exit 0

cargo nextest run
# all tests pass
```

**Commit:** `phase-5: materialize-chain, single-file pipeline, round-trip verified`

---

### Phase 6 — Turn Log and Session

**What it delivers:** The `TurnLog` type and `Session` struct — the ordered
record of what happened in a pipeline run.

**Why now:** Template variable resolution (Phase 7) reads from session state.
The executor (Phase 9) writes to it. Both depend on these types existing.
The turn log is also the audit trail required by ARCHITECTURE.md §5.

**Naming note (ARCHITECTURE.md §2.5):** The type is `TurnLog`, not `events`
(implies event-driven architecture or observability traces) and not `history`
(implies cross-session persistence). A turn log is the bounded, ordered record
of one pipeline run.

**Tasks:**

Create `ail-core/src/session/`:

`turn_log.rs`:
```rust
pub struct TurnLog { entries: Vec<TurnEntry> }

pub struct TurnEntry {
    pub step_id: String,
    pub prompt: String,
    pub response: Option<String>,
    pub timestamp: std::time::SystemTime,
    pub cost_usd: Option<f64>,
}
```

`TurnLog` methods: `new()`, `append(entry: TurnEntry)`,
`last_response() -> Option<&str>`, `response_for_step(id: &str) -> Option<&str>`.

Every `append()` call:
- Emits a `tracing::info!` span with structured fields: `run_id`, `step_id`,
  `cost_usd`.
- Serialises the entry to a JSON line and appends it to
  `.ail/runs/<run_id>.jsonl`. Creates the directory if absent. Never
  truncates. Append-only.

`session.rs`:
```rust
pub struct Session {
    pub run_id: String,              // UUID v4, generated on Session::new()
    pub pipeline: Pipeline,
    pub invocation_prompt: String,
    pub turn_log: TurnLog,
    pub tool_allowlist: Vec<String>, // in-memory only, never persisted
}
```

`Session::new(pipeline: Pipeline, invocation_prompt: String) -> Session`
generates a UUID v4 `run_id` using the `uuid` crate.

Add to `spec_coverage.rs`:
```rust
mod s4_execution_model {
    mod session {
        /// SPEC §4 — each pipeline run has a unique run_id
        #[test]
        fn session_new_generates_unique_run_id() { ... }

        /// SPEC §4 — entries are ordered and retrievable
        #[test]
        fn turn_log_entries_are_ordered() { ... }

        /// SPEC §4 — last_response returns the most recent entry
        #[test]
        fn last_response_returns_most_recent_entry() { ... }

        /// SPEC §4 — turn log persists to append-only NDJSON file
        #[test]
        fn turn_log_append_writes_ndjson_line_to_disk() { ... }

        /// SPEC §4 — two sessions produce different run_ids
        #[test]
        fn two_sessions_have_distinct_run_ids() { ... }
    }
}
```

**Observable verification:**
```bash
cargo nextest run
# all session tests pass

cargo nextest run turn_log_append_writes_ndjson_line_to_disk -- --nocapture
# .ail/runs/<uuid>.jsonl exists with one valid JSON line
```

**Commit:** `phase-6: TurnLog, Session, NDJSON audit trail`

---

### Phase 7 — Template Variable Resolution

**What it delivers:** `{{ variable }}` syntax resolves at execution time
against session state.

**Why now:** The executor (Phase 9) calls this for every prompt before
invoking the runner. It must be fully tested before executor logic is written
— otherwise executor tests cannot control or verify prompt content.

**Tasks:**

In `ail-core/src/template.rs`:

```rust
pub fn resolve(template: &str, session: &Session) -> Result<String, AilError>
```

Supported variables (SPEC §11):

| Variable | Source |
|---|---|
| `{{ step.invocation.prompt }}` | `session.invocation_prompt` |
| `{{ step.invocation.response }}` | turn log entry where `step_id == "invocation"` |
| `{{ last_response }}` | `session.turn_log.last_response()` |
| `{{ step.<id>.response }}` | `session.turn_log.response_for_step(id)` |
| `{{ session.tool }}` | `"claude"` (hardcoded for v0.0.1) |
| `{{ session.cwd }}` | `std::env::current_dir()` as string |
| `{{ pipeline.run_id }}` | `session.run_id` |
| `{{ env.VAR_NAME }}` | `std::env::var("VAR_NAME")` |

Error behaviour (SPEC §11 — silent empty is never permitted):
- Unrecognised variable syntax → `AilError` (`TEMPLATE_UNRESOLVED`)
- Step ID not present in turn log → `AilError`
- `env.VAR_NAME` where `VAR_NAME` is not set → `AilError`

Add to `spec_coverage.rs`:
```rust
mod s11_template_variables {
    #[test] fn template_with_no_variables_is_unchanged() { ... }
    #[test] fn last_response_resolves_from_turn_log() { ... }
    #[test] fn named_step_response_resolves_correctly() { ... }
    #[test] fn pipeline_run_id_resolves_to_session_value() { ... }
    #[test] fn env_var_resolves_when_set() { ... }
    #[test] fn env_var_errors_when_not_set() { ... }
    #[test] fn unknown_step_id_returns_error_not_empty_string() { ... }
    #[test] fn unrecognised_syntax_returns_error_not_empty_string() { ... }
}
```

**Observable verification:**
```bash
cargo nextest run s11_template_variables
# all 8 tests pass
```

**Commit:** `phase-7: template variable resolution, §11 variables, error on unresolved`

---

### Phase 8 — Runner Trait and Claude CLI Adapter

**What it delivers:** `ail` can invoke Claude CLI and receive a structured
response via the stream-json interface.

**Why now:** The executor (Phase 9) depends on the `Runner` trait. The
trait boundary must exist before the executor is written so that the executor
is built against an abstraction. The `StubRunner` built here is also what
all executor unit tests use.

---

> ⚠️ **This phase is a spike.** RUNNER-SPEC.md describes the Claude CLI
> stream-json interface based on documentation and observation, but this is
> the first point at which those assumptions are validated by running code.
>
> Before writing any code for this phase, write a LEARNINGS.md entry listing
> every assumption being made about the Claude CLI interface — expected event
> types, the completion signal, the flag combinations, stdin/stdout behaviour.
> Then, as implementation proceeds, record what matched and what diverged.
>
> The purpose of writing assumptions down before testing them is to make
> divergences legible. A surprise you didn't write down cannot be learned from.

---

**Tasks:**

In `ail-core/src/runners/`:

`mod.rs` — the `Runner` trait and shared output types:

```rust
pub trait Runner: Send + Sync {
    fn invoke(
        &self,
        prompt: &str,
        on_event: &dyn Fn(RunnerEvent),
    ) -> Result<RunnerOutput, AilError>;
    fn name(&self) -> &str;
    fn capabilities(&self) -> RunnerCapabilities;
}

pub enum RunnerEvent {
    TextDelta(String),
    ToolUse(ToolUseEvent),
    PermissionRequest(PermissionEvent),
    Complete(RunnerOutput),
    Error(AilError),
}

pub struct RunnerOutput {
    pub response: String,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
}

pub struct RunnerCapabilities {
    pub structured_output: bool,
    pub bidirectional_stdin: bool,
    pub tool_permissions: bool,
    pub session_continuity: bool,
}
```

`stub.rs` — `StubRunner` implementing `Runner`. Accepts configurable canned
response strings at construction. This is a real `Runner` implementation,
not a mock framework. All non-integration tests use it.

`claude/mod.rs` — `ClaudeCliRunner` implementing `Runner`:
- Spawns `claude --output-format stream-json -p "<prompt>"`
- Reads stdout as NDJSON line by line
- Parses each line via `claude/stream.rs`
- Calls `on_event` for each parsed event
- Returns `RunnerOutput` when a `result` event is received

`claude/stream.rs` — **the only module in the codebase that knows about
Claude CLI NDJSON event shapes** (ARCHITECTURE.md §9). No other module touches
raw JSON from Claude CLI. Parses `&str` lines → `Result<RunnerEvent, AilError>`.

Wire `ail --once "<prompt>"`:
1. Create a `Session` with `Pipeline::passthrough()`.
2. Invoke `ClaudeCliRunner` with the prompt.
3. Append the response as a `TurnEntry`.
4. Print the response to stdout.
5. Exit 0.

Add to `spec_coverage.rs`:
```rust
mod s20_runners {
    /// Runner trait is implemented correctly by StubRunner
    #[test]
    fn stub_runner_returns_configured_response() { ... }

    /// RunnerOutput carries response text
    #[test]
    fn runner_output_contains_response_text() { ... }

    /// Runner declares its capabilities
    #[test]
    fn runner_declares_capabilities_struct() { ... }
}
```

Add an integration test gated behind `#[ignore]`:
```rust
/// Requires: `claude` on PATH, `ANTHROPIC_API_KEY` set
#[test]
#[ignore = "integration — requires claude CLI and ANTHROPIC_API_KEY"]
fn claude_cli_returns_response_for_simple_prompt() {
    // Invoke ClaudeCliRunner with "say hello in one word".
    // Assert: response is non-empty.
    // Assert: cost_usd is Some.
}
```

**Observable verification:**
```bash
# Unit tests only (no Claude CLI needed):
cargo nextest run s20_runners
# 3 tests pass

# Integration test:
ANTHROPIC_API_KEY=<key> cargo nextest run --ignored \
  claude_cli_returns_response_for_simple_prompt
# test passes

# Observable end-to-end:
ail --once "say hello in one word"
# Claude invoked, single-word response printed, exit 0
```

**Commit:** `phase-8: Runner trait, ClaudeCliRunner spike, StubRunner, stream parsing isolated`

---

### Phase 9 — Pipeline Executor

**What it delivers:** The core invariant fires. A pipeline with one step
executes after a prompt. This is the v0.0.1 proof of concept.

**Why now:** All six dependencies are in place — error types, CLI surface,
domain model, session and turn log, template resolution, and the runner
abstraction. The executor is the final piece.

**Tasks:**

In `ail-core/src/executor.rs`:

```rust
pub fn execute_pipeline(
    session: &mut Session,
    runner: &dyn Runner,
) -> Result<ExecutionResult, AilError>

pub enum ExecutionResult {
    Completed,
    BreakExit { message: Option<String> },
    PausedForHuman { message: String },
}
```

For each step in `session.pipeline.steps`:

1. **Evaluate condition.** v0.0.1 supports `always` and `never` only.
   `never` skips the step without error.

2. **Resolve template variables** via `template::resolve()`. An unresolved
   variable aborts the step with `AilError`.

3. **Invoke runner** with the resolved prompt.

4. **Append `TurnEntry`** to the session turn log.

5. **Evaluate `on_result` rules.** v0.0.1 supports:
   - Match operators: `contains: "TEXT"`, `always`
   - Actions: `continue` (default), `abort_pipeline` → `Err(AilError)`,
     `break` → `Ok(BreakExit)`, `pause_for_human` → `Ok(PausedForHuman)`

The executor is a function of its inputs. No global state. I/O paths are
through the `runner` parameter and `session.turn_log`. This makes it fully
testable with `StubRunner`.

Wire the complete `ail --once "<prompt>"` flow:
1. Discover and load pipeline (passthrough if none found).
2. Create `Session`.
3. Invoke runner with the user's prompt → append invocation `TurnEntry`.
4. Call `execute_pipeline()`.
5. Print each step's response as it completes.
6. Exit 0 on `Completed` or `BreakExit`.
7. Exit non-zero on `Err`.

Add to `spec_coverage.rs`:
```rust
mod s4_execution_model {
    mod executor {
        /// SPEC §4.2 — all steps execute in order before control returns
        #[test]
        fn all_steps_execute_in_order() { ... }

        /// SPEC §12.1 — condition:never skips the step
        #[test]
        fn condition_never_skips_step() { ... }

        /// SPEC §5.3 — on_result contains match continues execution
        #[test]
        fn on_result_contains_match_continues() { ... }

        /// SPEC §5.3 — on_result no match triggers configured action
        #[test]
        fn on_result_no_match_triggers_action() { ... }

        /// SPEC §5.3 — break exits cleanly as Ok, not Err
        #[test]
        fn break_exits_as_ok_not_err() { ... }

        /// SPEC §5.3 — abort_pipeline exits as AilError
        #[test]
        fn abort_pipeline_exits_as_ail_error() { ... }

        /// SPEC §11 — resolved prompt uses prior step response
        #[test]
        fn step_prompt_resolves_last_response_variable() { ... }
    }
}
```

**Observable verification — the core invariant fires:**

```bash
cat > /tmp/demo.ail.yaml << 'EOF'
version: "0.0.1"
pipeline:
  - id: dont_be_stupid
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
EOF

ail --once "write a hello world function in python" \
    --pipeline /tmp/demo.ail.yaml
```

Expected: two Claude invocations are visible. The first is the user's prompt.
The second is the `dont_be_stupid` step running against the first response.
Exit code 0.

```bash
cargo nextest run
# all non-ignored tests pass
```

**Commit:** `phase-9: pipeline executor, core invariant fires, v0.0.1 demo works`

---

### Phase 10 — Reflection, Cleanup, and v0.0.1 Tag

**What it delivers:** A clean, tagged, reviewable v0.0.1. No new features —
only consolidation.

**Why now:** The proof of concept works. Before tagging, take stock of what
was learned across nine phases and ensure the codebase is in a state a
collaborator can understand without needing to ask questions.

**Tasks:**

**Read `LEARNINGS.md` in full.** For each [SPEC], [ARCH], or [UNDOC] flag:
- Summarise in a Phase 10 LEARNINGS.md entry.
- State whether it was resolved during the build or needs human review.
- Do not edit `SPEC.md` or `ARCHITECTURE.md` — flag them for human review.

**Full quality sweep:**
```bash
cargo nextest run
cargo clippy -- -D warnings
cargo fmt --check
```
All must pass before the tag.

**`spec_coverage.rs` audit.** Every v0.0.1 feature from SPEC §21 has a
passing test. Every feature not yet implemented is stubbed as:
```rust
#[test]
#[ignore = "not yet implemented — SPEC §<section>"]
fn feature_name() { todo!() }
```

**Write `CHANGELOG.md`:**
```markdown
## v0.0.1 — <date>

### What works
- ...

### Explicitly stubbed (entry points exist, no implementation behind them)
- ...

### Explicitly deferred to later versions (SPEC §22)
- ...

### Open questions surfaced during build (see LEARNINGS.md for detail)
- ...
```

**Tag:**
```bash
git tag v0.0.1
```

**Observable verification:**
```bash
# The SPEC §21 demo, end-to-end:
cat > demo.ail.yaml << 'EOF'
version: "0.0.1"
pipeline:
  - id: dont_be_stupid
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
EOF

ail --once "write a fizzbuzz function" --pipeline demo.ail.yaml
# Two Claude responses visible. Exit 0.

cargo nextest run
# All non-ignored tests pass.
```

**Commit:** `phase-10: v0.0.1 cleanup, spec coverage audit, CHANGELOG, tag`

---

## Starting Checklist

Before writing a single line of code, confirm each item and state the
confirmation explicitly.

- [ ] SPEC.md read in full — state the core invariant (§4.2) in your own words
- [ ] ARCHITECTURE.md read in full — state the two-crate rule in your own words
- [ ] RUNNER-SPEC.md read in full — state what the stream-json interface delivers
- [ ] Rust toolchain installed: `rustup show`
- [ ] `cargo-nextest` installed: `cargo install cargo-nextest`
- [ ] Claude CLI installed and `ANTHROPIC_API_KEY` set (required for Phase 8
      integration test only — Phases 1–7 and all unit tests run without it)
- [ ] Git repository initialised with an initial empty commit

Once all items are confirmed, state the plan for Phase 1 before writing any
code: what will be created, why, and what the passing verification looks like.
