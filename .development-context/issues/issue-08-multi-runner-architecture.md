# Issue #8: Multi-Runner Architecture Refactor ✓ DONE

## Context

The `Runner` trait already exists in `ail-core/src/runner/mod.rs` (lines 133–150) with `invoke()` and `invoke_streaming()`. `ClaudeCliRunner` and `StubRunner` both implement it. The executor already accepts `&dyn Runner`.

The gap: `ail/src/main.rs:476–478` directly instantiates `ClaudeCliRunnerConfig::default().build()` — there is no factory, no config-driven runner selection, and no way to override per step. This refactor adds a `RunnerFactory` and a per-step `runner:` field to the pipeline YAML.

---

## Design

**Strategy pattern** — factory resolves runner by name. Selection hierarchy:
1. Per-step `runner:` field in YAML
2. `AIL_DEFAULT_RUNNER` environment variable
3. Fallback: `"claude"` → `ClaudeCliRunner`

No existing executor or trait code changes required for the basic case. A factory-aware executor path is added alongside the existing one.

---

## Implementation Steps

### Step 1: Add `RUNNER_NOT_FOUND` error type

**File:** `ail-core/src/error.rs`

Add to `error_types` module:
```rust
pub const RUNNER_NOT_FOUND: &str = "ail:runner/not-found";
```

---

### Step 2: Create `RunnerFactory`

**File:** `ail-core/src/runner/factory.rs` (new)

```rust
#![allow(clippy::result_large_err)]

use crate::error::{error_types, AilError};
use super::{Runner, stub::StubRunner, claude::{ClaudeCliRunner, ClaudeCliRunnerConfig}};

pub struct RunnerFactory;

impl RunnerFactory {
    pub fn build(runner_name: &str, headless: bool) -> Result<Box<dyn Runner>, AilError> {
        match runner_name.trim().to_lowercase().as_str() {
            "claude" => Ok(Box::new(ClaudeCliRunnerConfig::default().headless(headless).build())),
            "stub"   => Ok(Box::new(StubRunner::new("stub response"))),
            other    => Err(AilError {
                error_type: error_types::RUNNER_NOT_FOUND,
                title: "Unknown runner".to_string(),
                detail: format!("Runner '{other}' is not recognized"),
                context: None,
            }),
        }
    }

    pub fn build_default(headless: bool) -> Result<Box<dyn Runner>, AilError> {
        let name = std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string());
        Self::build(&name, headless)
    }
}
```

Unit tests in same file:
- `test_build_claude_runner`
- `test_build_stub_runner`
- `test_build_unknown_runner_returns_error`
- `test_build_default_respects_env_var`

---

### Step 3: Export factory from runner module

**File:** `ail-core/src/runner/mod.rs`

Add: `pub mod factory;`

---

### Step 4: Extend config with per-step `runner:` field

**File:** `ail-core/src/config/dto.rs`
```rust
// In StepDto:
pub runner: Option<String>,
```

**File:** `ail-core/src/config/domain.rs`
```rust
// In Step:
pub runner: Option<String>,
```

**File:** `ail-core/src/config/validation.rs`
```rust
// In dto→domain conversion for Step:
runner: dto.runner,
```

---

### Step 5: Update CLI to use factory

**File:** `ail/src/main.rs:476–478` (and ~508 for TUI mode)

```rust
// OLD:
let runner = ClaudeCliRunnerConfig::default().headless(cli.headless).build();

// NEW:
use ail_core::runner::factory::RunnerFactory;
let runner = RunnerFactory::build_default(cli.headless)?;
```

Errors propagate naturally — existing `Err(e) => { eprintln!("{e}"); std::process::exit(1); }` pattern handles them.

---

### Step 6: Add factory-aware executor path (optional, for per-step runners)

**File:** `ail-core/src/executor.rs`

Add alongside existing `execute()` — do NOT modify existing function:

```rust
pub fn execute_with_factory(
    session: &mut Session,
    factory: &RunnerFactory,
    headless: bool,
) -> Result<ExecuteOutcome, AilError> {
    // For each step: check step.runner, build runner, call existing step execution logic
}
```

This is only needed once a pipeline actually uses per-step `runner:` fields. Can defer to v0.3+ if timeline is tight — Steps 1–5 deliver the environment-variable-based runner selection which is the primary ask.

---

### Step 7: Integration test

**File:** `ail-core/tests/spec/s08_multi_runner.rs` (new)

- Pipeline using `stub` runner via `AIL_DEFAULT_RUNNER=stub`
- Error case: `AIL_DEFAULT_RUNNER=nonexistent` → `RUNNER_NOT_FOUND`
- Per-step override (if Step 6 is implemented)

---

## Spec Updates Required

**`spec/runner/r03-targets.md`** (or equivalent runner target spec):
- Add `RunnerFactory` pattern documentation
- Document how to register a new runner (impl `Runner` + add match arm)
- Document `AIL_DEFAULT_RUNNER` env var

**`spec/core/s05-step-specification.md`** (or step spec):
- Add optional `runner:` field to step YAML schema
- Document fallback rules
- Example:
  ```yaml
  pipeline:
    - id: analyze
      prompt: "Analyze this."
      runner: claude
    - id: refactor
      prompt: "Refactor it."
      runner: aider  # future
  ```

Check `spec/README.md` for exact file paths before editing.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `ail-core/src/error.rs` | Edit | Add `RUNNER_NOT_FOUND` constant |
| `ail-core/src/runner/factory.rs` | Create | Factory implementation |
| `ail-core/src/runner/mod.rs` | Edit | Export factory module |
| `ail-core/src/config/dto.rs` | Edit | Add `runner: Option<String>` to StepDto |
| `ail-core/src/config/domain.rs` | Edit | Add `runner: Option<String>` to Step |
| `ail-core/src/config/validation.rs` | Edit | Pass `runner` through in conversion |
| `ail/src/main.rs` | Edit | Use `RunnerFactory::build_default()` |
| `ail-core/tests/spec/s08_multi_runner.rs` | Create | Integration tests |
| `spec/runner/r*.md` + `spec/core/s*.md` | Edit | Spec updates (see above) |

---

## Verification

```bash
# Default behavior unchanged
cargo run -- --once "Hello" --pipeline demo/.ail.yaml

# Stub runner via env var
AIL_DEFAULT_RUNNER=stub cargo run -- --once "Hello" --pipeline demo/.ail.yaml

# Unknown runner fails cleanly
AIL_DEFAULT_RUNNER=nonexistent cargo run -- --once "Hello" --pipeline demo/.ail.yaml

# Tests
cargo nextest run
cargo clippy -- -D warnings
```

---

## Success Criteria

- Adding a new runner = impl `Runner` + one match arm in factory
- Runner selection: per-step field → `AIL_DEFAULT_RUNNER` env → `claude`
- Backward compatible: existing code using `execute(session, runner)` unchanged
- All existing tests pass
