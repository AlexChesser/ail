# v0.1 Manual Testing Plan

## Context

The v0.1 implementation is complete (91/91 tests, clippy clean, fmt clean). This plan covers manual verification of all v0.1 features before the work is considered done. The goal is to confirm that the CLI end-to-end behavior matches the spec — automated tests cover unit/integration, but manual testing covers the full binary path.

**Constraint:** `--once` with `ClaudeCliRunner` cannot run inside a Claude Code session (nested-session guard). Tests marked [OUTSIDE] must be run in a plain terminal.

---

## Pre-flight

```bash
# From workspace root
cargo build --release
cargo nextest run          # confirm 91 tests pass
cargo clippy -- -D warnings
cargo fmt --check
```

---

## Test 1: Validate — context_shell fixture

```bash
cargo run -- --pipeline ail-core/tests/fixtures/context_shell.ail.yaml validate
```

**Expected:** `Pipeline valid: 1 step(s)` and exit 0.

---

## Test 2: Validate — on_result_multi_branch fixture (the benchmarking pipeline)

```bash
cargo run -- --pipeline ail-core/tests/fixtures/on_result_multi_branch.ail.yaml validate
```

**Expected:** `Pipeline valid: 3 step(s)` and exit 0.

---

## Test 3: Validate — invalid fixture (context with no source)

```bash
cargo run -- --pipeline ail-core/tests/fixtures/invalid_context_no_source.ail.yaml validate
```

**Expected:** Error output containing `ail:config/validation-failed` and exit 1.

---

## Test 4: Materialize — context_shell fixture

```bash
cargo run -- --pipeline ail-core/tests/fixtures/context_shell.ail.yaml materialize
```

**Expected:** YAML output with:
- `context:` block containing `shell: "cargo clippy -- -D warnings"`
- `# origin:` comment on the step

---

## Test 5: Materialize — on_result_multi_branch fixture

```bash
cargo run -- --pipeline ail-core/tests/fixtures/on_result_multi_branch.ail.yaml materialize
```

**Expected:** YAML output with:
- Three steps (`lint`, `tests`, `fix_and_verify`)
- `context: shell:` blocks on lint and tests
- `on_result:` arrays with `exit_code:` and `action:` entries
- `prompt:` with template variables on fix_and_verify
- `# origin:` comments

---

## Test 6: Materialize — legacy demo pipeline still works

```bash
cargo run -- --pipeline demo/.ail.yaml materialize
```

**Expected:** YAML output with the single `dont_be_stupid` step. Confirms v0.0.1 pipelines still parse.

---

## Test 7: [OUTSIDE] End-to-end — demo pipeline

Run in a plain terminal (not Claude Code session):

```bash
cd demo
../target/release/ail --pipeline .ail.yaml --once "Write a function that adds two numbers"
```

**Expected:** Claude response with a function, then the `dont_be_stupid` review step runs and prints a review. Two responses visible.

---

## Test 8: [OUTSIDE] End-to-end — benchmarking pipeline

Create a temp Rust project and run the benchmarking pipeline against it:

```bash
# Setup
cd /tmp && cargo init ail-test-project && cd ail-test-project

# Run — use the on_result_multi_branch fixture as the pipeline
/path/to/target/release/ail \
  --pipeline /path/to/ail-core/tests/fixtures/on_result_multi_branch.ail.yaml \
  --once "add a fizzbuzz function"
```

**Expected behavior:**
1. Invocation step: Claude writes a fizzbuzz function
2. `lint` step: runs `cargo clippy -- -D warnings`, captures exit code
3. `on_result` on lint: exit_code 0 → continue, non-zero → continue (both continue)
4. `tests` step: runs `cargo test --quiet`, captures exit code
5. `on_result` on tests: exit_code 0 → **break** (skips fix_and_verify), non-zero → continue
6. If tests failed: `fix_and_verify` prompt step runs with lint/test output injected via template variables

**Key things to verify:**
- Non-zero exit codes don't crash the pipeline (they're results, not errors)
- `break` action actually skips remaining steps
- Template variables (`{{ step.lint.result }}`, `{{ step.tests.exit_code }}`) resolve correctly in the prompt

---

## Test 9: [OUTSIDE] Headless flag

```bash
../target/release/ail --pipeline demo/.ail.yaml --headless --once "say hello"
```

**Expected:** The `--dangerously-skip-permissions` flag is passed to claude CLI. Verify by checking that no permission prompts appear (or add `RUST_LOG=debug` to see the command args in structured logs).

---

## Test 10: [OUTSIDE] File path resolution for prompt

Create a prompt file and a pipeline that references it:

```bash
# Create temp files
cat > /tmp/my_prompt.md << 'EOF'
Review the code above. List any bugs.
EOF

cat > /tmp/file_prompt_test.ail.yaml << 'EOF'
version: "0.1"
pipeline:
  - id: review
    prompt: "/tmp/my_prompt.md"
EOF

# Run
/path/to/target/release/ail \
  --pipeline /tmp/file_prompt_test.ail.yaml \
  --once "write hello world in python"
```

**Expected:** The review step uses the contents of `/tmp/my_prompt.md` as its prompt, not the literal path string.

---

## Test 11: Pipeline discovery (no --pipeline flag)

```bash
# In a directory with .ail.yaml
cd demo
../target/release/ail --once "hello"

# In a directory without .ail.yaml
cd /tmp
/path/to/target/release/ail --once "hello"
```

**Expected:**
- With `.ail.yaml` present: pipeline is discovered and steps run
- Without: passthrough mode — just the invocation, no pipeline steps

---

## Pass/Fail Checklist

| # | Test | In-session? | Pass? |
|---|---|---|---|
| 1 | validate context_shell | Yes | |
| 2 | validate on_result_multi_branch | Yes | |
| 3 | validate invalid fixture | Yes | |
| 4 | materialize context_shell | Yes | |
| 5 | materialize on_result_multi_branch | Yes | |
| 6 | materialize demo pipeline | Yes | |
| 7 | e2e demo pipeline | No | |
| 8 | e2e benchmarking pipeline | No | |
| 9 | headless flag | No | |
| 10 | file path resolution | No | |
| 11 | pipeline discovery | No | |

Tests 1–6 can be run right now. Tests 7–11 require a plain terminal.

---

# v0.1 Implementation Plan: Benchmarking-Ready

## Context

The spec cleanup (Phase A) is complete. This plan implements Phase B — the minimum feature set to run the SWE-bench benchmarking experiment described in `docs/blog/the-yaml-of-the-mind.md`. The target pipeline is defined in `spec/core/s20-mvp.md` (v0.1 scope):

```yaml
version: "0.1"
pipeline:
  - id: lint
    context:
      shell: "cargo clippy -- -D warnings"
    on_result:
      - exit_code: 0
        action: continue
      - exit_code: any
        action: continue
  - id: tests
    context:
      shell: "cargo test --quiet"
    on_result:
      - exit_code: 0
        action: break
      - exit_code: any
        action: continue
  - id: fix_and_verify
    prompt: |
      Lint result (exit {{ step.lint.exit_code }}):
      {{ step.lint.result }}
      Test result (exit {{ step.tests.exit_code }}):
      {{ step.tests.result }}
      Fix all failures. Explain what you changed.
```

## Design Decisions

**D1: Context steps** — Add `Context(ContextSource)` variant to `StepBody`, with `enum ContextSource { Shell(String) }`. Mirrors YAML structure, extensible to `Mcp` later. Context steps never touch the `Runner` trait.

**D2: TurnEntry expansion** — Add `stdout: Option<String>`, `stderr: Option<String>`, `exit_code: Option<i32>` as flat optional fields. Context steps set these; prompt steps leave them `None`.

**D3: on_result domain types** — `Step` gains `on_result: Option<Vec<ResultBranch>>`. `ResultBranch { matcher: ResultMatcher, action: ResultAction }`. v0.1 matchers: `Contains(String)`, `ExitCode(Exact(i32) | Any)`, `Always`. v0.1 actions: `Continue`, `Break`, `AbortPipeline`, `PauseForHuman`.

**D4: Executor outcome** — `execute()` returns `Result<ExecuteOutcome, AilError>` where `ExecuteOutcome = Completed | Break { step_id }`. `abort_pipeline` returns `Err(PIPELINE_ABORTED)`. `break` returns `Ok(Break)`.

**D5: Shell execution** — Executor spawns `Command::new("/bin/sh").args(["-c", cmd])` directly. Uses `child.wait_with_output()` for deadlock-safe stdout+stderr capture. No runner involvement.

**D6: `exit_code: any` semantics** — Matches any non-zero exit code per spec §5.4. Does NOT match exit code 0.

**D7: File path resolution** — In executor, before template resolution: if prompt starts with `./`, `../`, `~/`, or `/`, read file contents as the template. Failure → `CONFIG_FILE_NOT_FOUND`.

**D8: Headless flag** — Add `headless: bool` to `ClaudeCliRunner` constructor. When true, adds `--dangerously-skip-permissions`. Already parsed in `cli.rs`, just needs wiring.

**D9: Single-match on_result syntax** — v0.1 scope is multi-branch array syntax only. Single-match (`contains:` / `if_true:` / `if_false:`) is deferred. The benchmarking pipeline uses array syntax exclusively.

---

## Phase 1: Domain Types (on_result + context)

**Goal:** Extend DTO, domain, and validation to parse `context: shell:` steps and `on_result` multi-branch arrays. No execution changes.

### `ail-core/src/config/dto.rs`
- Add `ContextDto { shell: Option<String> }` with `#[derive(Deserialize)]`
- Add `OnResultBranchDto { contains: Option<String>, exit_code: Option<ExitCodeDto>, always: Option<bool>, action: Option<String> }` with `#[derive(Deserialize)]`
- `ExitCodeDto`: use `#[serde(untagged)]` enum — `Integer(i32)` and `Keyword(String)` — to handle both `exit_code: 0` and `exit_code: any`
- Add to `StepDto`: `context: Option<ContextDto>`, `on_result: Option<Vec<OnResultBranchDto>>`

### `ail-core/src/config/domain.rs`
- Add `Context(ContextSource)` variant to `StepBody`
- Add `enum ContextSource { Shell(String) }` (derives `Debug`)
- Add `on_result: Option<Vec<ResultBranch>>` to `Step`
- Add `struct ResultBranch { pub matcher: ResultMatcher, pub action: ResultAction }` (derives `Debug`)
- Add `enum ResultMatcher { Contains(String), ExitCode(ExitCodeMatch), Always }` (derives `Debug`)
- Add `enum ExitCodeMatch { Exact(i32), Any }` (derives `Debug`)
- Add `enum ResultAction { Continue, Break, AbortPipeline, PauseForHuman }` (derives `Debug`)

### `ail-core/src/config/validation.rs`
- Add `context` to primary-field count (now 5: prompt, skill, pipeline, action, context)
- Convert `ContextDto` → `StepBody::Context(ContextSource::Shell(cmd))` with validation that `shell` is `Some`
- Convert `on_result` array: validate exactly one matcher per branch, known action string → `ResultAction`, `ExitCodeDto::Keyword("any")` → `ExitCode(Any)`, `ExitCodeDto::Integer(n)` → `ExitCode(Exact(n))`

### Fixtures
- `context_shell.ail.yaml` — single context:shell step
- `on_result_multi_branch.ail.yaml` — the benchmarking pipeline
- `invalid_context_no_source.ail.yaml` — context with no shell/mcp

### Tests (`ail-core/tests/spec/s05_5_context_steps.rs` — new file)
- `context_shell_step_parses_correctly`
- `context_step_without_source_fails_validation`
- `context_and_prompt_on_same_step_fails`
- `on_result_multi_branch_parses`
- `exit_code_integer_parses` / `exit_code_any_parses`
- `on_result_unknown_action_fails`

---

## Phase 2: TurnEntry + Template Variables

**Goal:** TurnEntry stores context step output. Templates resolve `.result`, `.stdout`, `.stderr`, `.exit_code`.

### `ail-core/src/session/turn_log.rs`
- Add to `TurnEntry`: `pub stdout: Option<String>`, `pub stderr: Option<String>`, `pub exit_code: Option<i32>`
- Add `TurnLog` methods:
  - `result_for_step(id) -> Option<String>` — returns `stdout + "\n" + stderr` if stdout is Some, falls back to `response_for_step(id)` for prompt steps
  - `stdout_for_step(id) -> Option<&str>`
  - `stderr_for_step(id) -> Option<&str>`
  - `exit_code_for_step(id) -> Option<i32>`

### `ail-core/src/template.rs`
- Add patterns in variable resolution:
  - `step.<id>.result` → `turn_log.result_for_step(id)`
  - `step.<id>.stdout` → `turn_log.stdout_for_step(id)`
  - `step.<id>.stderr` → `turn_log.stderr_for_step(id)`
  - `step.<id>.exit_code` → `turn_log.exit_code_for_step(id)` as string

### Fix all TurnEntry construction sites
- `executor.rs` (line ~78) — add `stdout: None, stderr: None, exit_code: None`
- `ail/src/main.rs` (line ~88) — same
- Any test helpers that construct `TurnEntry`

### Tests
- `result_for_step_returns_stdout_plus_stderr`
- `template_step_id_result_resolves` / `.stdout` / `.stderr` / `.exit_code`

---

## Phase 3: Context Step Execution

**Goal:** Executor runs `context: shell:` steps, spawning `/bin/sh -c <cmd>`, capturing output, recording to turn log.

### `ail-core/src/executor.rs`
- Add match arm for `StepBody::Context(ContextSource::Shell(cmd))`:
  1. `record_step_started(&step_id, cmd)`
  2. Spawn `Command::new("/bin/sh").args(["-c", cmd]).stdout(Stdio::piped()).stderr(Stdio::piped())`
  3. Use `child.wait_with_output()` for safe capture
  4. Extract exit code, stdout string, stderr string
  5. Append `TurnEntry` with `stdout`, `stderr`, `exit_code` set; `response: None`, `cost_usd: None`, `runner_session_id: None`
  6. If spawn fails → `AilError` with `RUNNER_INVOCATION_FAILED`

### Tests
- `context_shell_captures_stdout` — `echo hello` → stdout = "hello\n"
- `context_shell_captures_stderr` — command writing to stderr
- `context_shell_captures_exit_code_zero` / `nonzero`
- `context_shell_does_not_call_runner` — verify no runner interaction
- `context_then_prompt_pipeline` — context step feeds template vars to subsequent prompt step

---

## Phase 4: on_result Evaluation + Flow Control

**Goal:** After each step, evaluate `on_result` branches. Change executor return type to `ExecuteOutcome`.

### `ail-core/src/executor.rs`
- Add `pub enum ExecuteOutcome { Completed, Break { step_id: String } }`
- Change `execute()` return: `Result<(), AilError>` → `Result<ExecuteOutcome, AilError>`
- After each step (prompt and context), if `step.on_result.is_some()`:
  - Call `evaluate_on_result(branches, &turn_entry) -> Option<ResultAction>`
  - Match action: `Continue` → proceed, `Break` → return `Ok(Break)`, `AbortPipeline` → return `Err(PIPELINE_ABORTED)`, `PauseForHuman` → log and continue (v0.1 headless no-op)
- `evaluate_on_result` logic:
  - `Contains(text)` — case-insensitive check on `response` or combined `stdout+stderr`
  - `ExitCode(Exact(n))` — `entry.exit_code == Some(n)`
  - `ExitCode(Any)` — `entry.exit_code.is_some() && entry.exit_code != Some(0)`
  - `Always` — true
  - First match wins. No match → `Continue`

### `ail/src/main.rs`
- Update `execute()` call site: `Ok(ExecuteOutcome::Completed)` and `Ok(ExecuteOutcome::Break { .. })` both exit successfully

### Update existing tests
- All tests calling `execute()` and checking `Ok(())` change to `Ok(ExecuteOutcome::Completed)` or match on `.is_ok()`

### Tests (implement the 4 ignored tests in `s05_3_on_result.rs` + new ones)
- `on_result_contains_match_continues`
- `on_result_abort_pipeline_exits_as_ail_error`
- `on_result_break_exits_as_ok_not_err`
- `on_result_pause_for_human_suspends`
- `on_result_exit_code_0_continue`
- `on_result_exit_code_any_matches_nonzero`
- `on_result_exit_code_any_does_not_match_zero`
- `on_result_first_match_wins`
- `on_result_always_matches`
- `on_result_break_skips_remaining_steps`

---

## Phase 5: File Path Resolution + Headless Flag

**Goal:** `prompt:` values can be file paths. `--headless` wires through to runner.

### `ail-core/src/executor.rs`
- In `Prompt` arm, before template resolution:
  - If prompt starts with `./`, `../`, `~/`, or `/`: read file, use contents as template
  - Resolve `~` via `dirs::home_dir()`
  - File not found → `AilError` with `CONFIG_FILE_NOT_FOUND`

### `ail-core/src/runner/claude.rs`
- Change `ClaudeCliRunner` to accept `headless: bool` in constructor
- When `headless`, push `"--dangerously-skip-permissions"` to args

### `ail/src/main.rs`
- Pass `cli.headless` to `ClaudeCliRunner::new(headless)`

### Tests
- `prompt_file_path_loads_contents` — temp file with template, verify resolution
- `prompt_file_not_found_returns_error`
- `prompt_inline_string_unchanged` — regression test

---

## Phase 6: Materialize + Documentation

**Goal:** `materialize` handles new step types. Docs reflect v0.1 state.

### `ail-core/src/materialize.rs`
- Add `StepBody::Context(ContextSource::Shell(cmd))` output
- Add `on_result:` array serialization

### Documentation updates
- `ail-core/CLAUDE.md` — update module table, key types, template variables
- `CLAUDE.md` — update template variables, known constraints
- `CHANGELOG.md` — add v0.1 entry
- `spec/core/s20-mvp.md` — mark v0.1 items as implemented

### Tests
- `materialize_context_shell_step`
- `materialize_on_result_branches`

---

## Phase Dependency Graph

```
Phase 1 (Domain types)
   ↓
Phase 2 (TurnEntry + templates)
   ↓
Phase 3 (Context execution)
   ↓
Phase 4 (on_result evaluation)
   ↓
Phase 5 (File paths + headless)
   ↓
Phase 6 (Materialize + docs)
```

Each phase leaves the codebase compiling, tests green, clippy clean.

## Known Challenges

1. **ExitCodeDto deserialization** — `exit_code: 0` (int) vs `exit_code: any` (string). Solve with `#[serde(untagged)]` enum in dto.rs.
2. **Executor return type change** — Mechanical update to ~5 existing tests that check `Ok(())`.
3. **TurnEntry field expansion** — 3 new Option fields; update all construction sites (3 locations).
4. **`step.<id>.result` ambiguity** — Must work for both context steps (stdout+stderr) and prompt steps (response). `result_for_step()` checks stdout first, falls back to response.

## Verification

After all phases:
1. `cargo nextest run` — all tests pass (no ignored tests remain for v0.1 features)
2. `cargo clippy -- -D warnings` — clean
3. `cargo fmt --check` — clean
4. `cargo run -- validate --pipeline <benchmarking-pipeline.yaml>` — parses correctly
5. `cargo run -- materialize --pipeline <benchmarking-pipeline.yaml>` — outputs valid YAML with context and on_result
6. Manual integration test: `cargo run --release -- --once "add fizzbuzz" --pipeline <benchmarking-pipeline.yaml>` in a Rust project (requires claude CLI)

## Critical Files

| File | Changes |
|---|---|
| `ail-core/src/config/dto.rs` | `ContextDto`, `OnResultBranchDto`, `ExitCodeDto`, new StepDto fields |
| `ail-core/src/config/domain.rs` | `ContextSource`, `ResultBranch`, `ResultMatcher`, `ResultAction`, `ExitCodeMatch`, Step.on_result |
| `ail-core/src/config/validation.rs` | Context + on_result DTO→domain conversion |
| `ail-core/src/session/turn_log.rs` | TurnEntry fields, new accessor methods |
| `ail-core/src/template.rs` | `.result`, `.stdout`, `.stderr`, `.exit_code` resolution |
| `ail-core/src/executor.rs` | Context execution, on_result evaluation, `ExecuteOutcome`, file path resolution |
| `ail-core/src/runner/claude.rs` | `headless` constructor param |
| `ail-core/src/materialize.rs` | Context + on_result serialization |
| `ail/src/main.rs` | Wire headless, handle `ExecuteOutcome` |
