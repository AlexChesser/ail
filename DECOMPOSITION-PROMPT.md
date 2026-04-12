# Module Decomposition: Prepare ail-core for issue #105

## Goal

Perform a purely mechanical refactoring of ail-core to decompose three monolithic
files into smaller, concern-scoped modules. NO behavior changes, NO new features,
NO signature changes to public APIs. Every existing test must pass unchanged
afterward. This is prep work so that upcoming features (loops, on_error, conditions,
parallel execution, budgets) each land in their own focused file.

## Branch

Develop on branch `claude/refactor-pipeline-modules-slAkT`. Push when done.

## Build / Test / Lint commands

```bash
cargo build
cargo nextest run
cargo clippy -- -D warnings
cargo fmt --check
```

All four must be clean before committing. Run them after each major move.

## What to decompose

### 1. `ail-core/src/executor/helpers.rs` (516 lines) → `ail-core/src/executor/helpers/`

Split into a module directory. Current contents and where they go:

| Current function(s) | New file | Why |
|---|---|---|
| `run_invocation_step` | `helpers/invocation.rs` | Invocation-step lifecycle — standalone concern |
| `resolve_step_provider`, `build_step_runner_box`, `resolve_effective_runner_name` | `helpers/runner_resolution.rs` | Runner selection/construction |
| `run_shell_command` | `helpers/shell.rs` | Shell subprocess execution |
| `evaluate_on_result`, `build_tool_policy` | `helpers/on_result.rs` | on_result branch evaluation + tool policy |
| `resolve_step_system_prompts`, `resolve_prompt_file` | `helpers/system_prompt.rs` | System prompt resolution and file loading |

Create `helpers/mod.rs` that re-exports everything currently exported from the
old `helpers.rs`. The `pub(super)` visibility and `pub` visibility must be
preserved exactly — nothing that was `pub(super)` becomes `pub`, nothing that
was `pub` becomes `pub(super)`. Keep the `#[cfg(test)] mod tests` blocks with
their respective functions (move each test block into the file that owns the
function it tests).

### 2. `ail-core/src/executor/core.rs` (516 lines) → `ail-core/src/executor/core.rs` + `ail-core/src/executor/dispatch/`

The `StepObserver` trait, `BeforeStepAction` enum, `NullObserver`, and
`execute_core()` stay in `core.rs`. Extract the step-type-specific execution
logic from the match arms (lines ~329–451) into dispatch modules:

| New file | Extracted from | Contains |
|---|---|---|
| `dispatch/mod.rs` | — | Re-exports |
| `dispatch/prompt.rs` | `StepBody::Prompt` arm (lines ~330–398) | Template resolution, runner invocation, TurnEntry construction for prompt steps |
| `dispatch/context.rs` | `StepBody::Context(Shell)` arm (lines ~400–411) | Shell context step execution |
| `dispatch/sub_pipeline.rs` | `StepBody::SubPipeline` arm (lines ~418–437) + the existing `execute_sub_pipeline` fn (wherever it lives in core.rs) | Sub-pipeline recursion, depth guard, child session creation |

After extraction, `execute_core()` becomes a thin router:

```rust
let entry = match &step.body {
    StepBody::Prompt(t) => dispatch::prompt::execute(
        t, &step, session, runner, &step_id,
        step_index, total_steps, pipeline_base_dir, observer,
    )?,
    StepBody::Context(ContextSource::Shell(cmd)) => dispatch::context::execute_shell(
        cmd, session, &step_id, observer,
    )?,
    StepBody::Action(ActionKind::PauseForHuman) => {
        unreachable!("handled above")
    }
    StepBody::SubPipeline { path, prompt } => dispatch::sub_pipeline::execute(
        path, prompt.as_deref(), &step_id,
        session, runner, depth, pipeline_base_dir, observer,
    )?,
    StepBody::Skill(_) => { /* existing error return */ },
};
```

The exact function signatures are up to you — the key constraint is that each
dispatch function receives only what it needs (no god-struct context objects)
and returns `Result<TurnEntry, AilError>`. Prompt dispatch will need the
observer for hooks (`on_prompt_ready`, `augment_options`, `invoke`,
`on_prompt_completed`). Context and sub-pipeline dispatch need observer only
for `on_non_prompt_completed` / `on_step_failed`.

The on_result evaluation loop (lines ~456–510) stays in `core.rs` — it's
post-dispatch and applies to all step types.

### 3. `ail-core/src/config/validation.rs` (1120 lines) → `ail-core/src/config/validation/`

Split into a module directory:

| New file | Contains |
|---|---|
| `validation/mod.rs` | `validate()` entry point, `cfg_err!` macro, `tools_to_policy` helper |
| `validation/step_body.rs` | Step body parsing logic (the primary_count check + body construction, currently lines ~214–266) — extract as `fn parse_step_body(step_dto: &StepDto, id_str: &str) -> Result<StepBody, AilError>` |
| `validation/on_result.rs` | `parse_result_branches()` (currently lines ~32–105) |
| `validation/system_prompt.rs` | `parse_append_system_prompt()` (currently lines ~107–137) |

The `#[cfg(test)] mod tests` block at the bottom should be split: tests that
exercise `validate()` stay in `mod.rs`, tests for `parse_result_branches` move
to `on_result.rs`, tests for `parse_append_system_prompt` move to
`system_prompt.rs`. If a test exercises multiple functions, keep it in `mod.rs`.

## Constraints — read carefully

1. **No behavior changes.** Every public function signature, every error message
   string, every `pub(super)`/`pub(crate)` boundary must be identical after the
   refactor. This is a pure structural move.

2. **All existing tests must pass unchanged.** The spec tests in
   `ail-core/tests/spec/` import from public APIs — those must not break. The
   unit tests inside the moved modules need their imports updated but the test
   logic must not change.

3. **Preserve the `#![allow(clippy::result_large_err)]` directive** at the top
   of every file that returns `Result<_, AilError>`. The CLAUDE.md lists which
   files need it. Every new file that returns `Result<_, AilError>` needs it too.

4. **Update `ail-core/CLAUDE.md`** module responsibility table to reflect the
   new file structure. Update `executor.rs` row to describe the new layout.
   Add rows for each new file. Remove rows for files that no longer exist.

5. **Do NOT update the spec.** This is not a functional change. Spec files are
   untouched.

6. **Commit messages:** one commit per logical move (e.g., "refactor: decompose
   executor/helpers into per-concern modules"). No co-authorship lines.

7. **Import style:** use `super::` and `crate::` imports, matching the existing
   codebase style. Don't introduce `use self::` or other patterns not already
   present.

8. **File-level docs:** each new file should have a `//!` doc comment (one
   line is fine) describing what it contains, matching the style already used
   in `core.rs`, `helpers.rs`, etc.

## Verification checklist (do all of these before pushing)

- [ ] `cargo build` — clean
- [ ] `cargo nextest run` — all pass (same count as before)
- [ ] `cargo clippy -- -D warnings` — clean
- [ ] `cargo fmt --check` — clean
- [ ] `ail-core/CLAUDE.md` updated
- [ ] No behavior changes (diff review: only `mod`, `use`, and file moves)
- [ ] Git log shows clean, descriptive commits
