# Task 04: ClaudeCliRunner Config Pattern

## Findings Addressed
- **LSP-001** (medium): headless flag is a Claude CLI concept leaking into the construction contract

## Problem Summary

`ClaudeCliRunner::new(headless: bool)` takes a Claude-specific concept as a bare constructor parameter. Callers must know they are constructing a `ClaudeCliRunner` specifically to pass it. A config struct encapsulates all Claude-specific construction parameters.

## Can Be Done Independently

This task has no dependencies on tasks 01-03.

## Implementation Steps

### Step 1: Add `ClaudeCliRunnerConfig` to `ail-core/src/runner/claude.rs`

```rust
#[derive(Debug, Clone)]
pub struct ClaudeCliRunnerConfig {
    pub claude_bin: String,
    pub headless: bool,
}

impl Default for ClaudeCliRunnerConfig {
    fn default() -> Self {
        Self { claude_bin: "claude".to_string(), headless: false }
    }
}

impl ClaudeCliRunnerConfig {
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }
    pub fn claude_bin(mut self, bin: impl Into<String>) -> Self {
        self.claude_bin = bin.into();
        self
    }
    pub fn build(self) -> ClaudeCliRunner {
        ClaudeCliRunner { claude_bin: self.claude_bin, headless: self.headless }
    }
}
```

### Step 2: Add `from_config` and deprecate old constructors

```rust
impl ClaudeCliRunner {
    pub fn from_config(config: ClaudeCliRunnerConfig) -> Self {
        Self { claude_bin: config.claude_bin, headless: config.headless }
    }

    #[deprecated(note = "Use ClaudeCliRunnerConfig::default().build()")]
    pub fn new(headless: bool) -> Self { ... }

    #[deprecated(note = "Use ClaudeCliRunnerConfig::default().claude_bin(...).build()")]
    pub fn with_bin(bin: impl Into<String>, headless: bool) -> Self { ... }
}
```

### Step 3: Update construction sites

**`ail/src/main.rs` line 112:**
```rust
let runner = ail_core::runner::claude::ClaudeCliRunnerConfig::default()
    .headless(cli.headless)
    .build();
```

**`ail/src/tui/backend.rs` line 56:**
```rust
let runner = ClaudeCliRunnerConfig::default().headless(headless).build();
```

**`ail-core/tests/spec/s08_runner_adapter.rs` line 28:**
```rust
let runner = ClaudeCliRunnerConfig::default().build();
```

**`ail-core/src/runner/claude.rs` internal tests (lines 484, 499):**
```rust
let runner = ClaudeCliRunnerConfig::default().build();
```

### Step 4: Update `Default` impl for `ClaudeCliRunner`

```rust
impl Default for ClaudeCliRunner {
    fn default() -> Self { ClaudeCliRunnerConfig::default().build() }
}
```

### Step 5: Update documentation

- `CLAUDE.md` line 165 references `ClaudeCliRunner::new(headless: bool)` — update

## What NOT to Change

- `headless` in `ail/src/cli.rs` — CLI flag definition stays
- `headless` parameter in TUI call chain — separate concern (backend-level socket setup)
- `InvokeOptions.base_url` / `auth_token` — per-invocation, not per-runner-instance
- `ClaudeCliRunner` struct fields — unchanged

## Verification

```bash
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo nextest run
```

Deprecated warnings from old-constructor usage confirm all sites migrated. Once migrated, deprecated methods can be removed.

## Critical Files
- `ail-core/src/runner/claude.rs` — add config struct, deprecate old constructors
- `ail/src/main.rs` — update --once construction (line 112)
- `ail/src/tui/backend.rs` — update TUI construction (line 56)
- `ail-core/tests/spec/s08_runner_adapter.rs` — update test (line 28)
