# TUI Phase 1 (MVP) Implementation Plan

## Context

`ail` currently has no TUI — the interactive REPL is a stub at `ail/src/main.rs:146-148`. The binary crate is just two files (`main.rs`, `cli.rs`) with `clap`/`tracing` dependencies. Meanwhile, `ail-core` is entirely synchronous: `executor::execute()` blocks during the step loop, `ClaudeCliRunner::invoke()` blocks while reading NDJSON from `claude` CLI, and all intermediate streaming events (`system`/`assistant`/`user` at `claude.rs:128-131`) are discarded.

This plan builds the Phase 1 MVP TUI described in `.development-context/tui/tui-planning-prompt.md` §3 — an observable, interruptible single-agent pipeline cockpit using ratatui.

---

## Architecture Decisions

### D1: Thread bridge (not async)
Run the TUI event loop on the main thread. Spawn executor work on a `std::thread`. Communicate via `std::sync::mpsc` channels. No tokio needed.

### D2: Additive ail-core changes via new types and default-impl trait methods
- New `RunnerEvent` enum + `invoke_streaming()` method with default impl on `Runner` trait
- New `ExecutionControl` struct + `execute_with_control()` function alongside existing `execute()`
- New `ExecutorEvent` enum sent via channel from `execute_with_control()`
- `Clone` derive on `RunResult`

These are all additive. Existing code paths (`--once`, headless, tests) are unchanged.

### D3: Step-disable via HashSet passed to executor
`disabled_steps: HashSet<String>` lives in TUI `AppState`, passed to `execute_with_control()`. TUI-only state, never persisted.

### D4: Mid-session injection (Path B) scoped to "queue for next step"
True mid-session injection (appending user turns to a live subprocess) requires killing and re-invoking with `--resume`. MVP scopes Path B as prepending guidance to the next step's prompt. Full injection is a documented stretch goal.

### D5: Provider/model configuration — YAML-first, CLI as override

The spec already defines per-step `provider:` and `model:` fields (s05) and a top-level `defaults:` block (s03, s15), but none are implemented. We implement the minimum viable subset so the pipeline YAML itself declares what model/provider to use. This prevents accidental expensive runs — if the YAML says Ollama, it's Ollama.

**YAML config (new):**
```yaml
defaults:
  model: gemma3:1b
  provider:
    base_url: http://localhost:11434
    auth_token: ollama

pipeline:
  - id: review
    prompt: "Review the code"
    # inherits defaults.model and defaults.provider
  - id: final_check
    prompt: "Final review"
    model: claude-sonnet-4-20250514   # per-step override
```

**CLI flags as overrides** (take precedence over YAML):
```bash
ail --model gemma3:1b --provider-url http://localhost:11434 --provider-token ollama
```

**Flow:** YAML `defaults` → per-step `model:`/`provider:` override → CLI flag override. The resolved model+provider flows through `InvokeOptions` to the runner. `ClaudeCliRunner` uses `--model` arg and per-subprocess `Command::env()` calls.

This is a **materially functional change** so it requires spec updates to s03, s05, s15, and r02.

---

## New Dependencies

**`ail/Cargo.toml`:**
```toml
ratatui = "0.29"
crossterm = "0.28"
```

No new deps for `ail-core`.

---

## File Structure

```
ail/src/
  main.rs              # modified: wire tui::run() at the REPL stub
  cli.rs               # unchanged
  tui/
    mod.rs             # entry point, crossterm setup/teardown, main event loop
    app.rs             # AppState, AppAction, update logic
    backend.rs         # BackendEvent, BackendCommand, spawn_backend()
    input.rs           # crossterm Event -> AppAction mapping
    theme.rs           # glyphs, colors, style constants (§2 visual language)
    ui/
      mod.rs           # draw() dispatcher
      layout.rs        # responsive three-region layout (§2.4 width tiers)
      sidebar.rs       # pipeline sidebar rendering
      viewport.rs      # main viewport + HUD overlay
      statusbar.rs     # status bar rendering
      prompt.rs        # input area rendering
      modal.rs         # interrupt modal, HITL gate, destructive warning
```

---

## Additive ail-core Changes

### 0. Provider/model support across ail-core and ail

**Precedence chain:** YAML `defaults:` → per-step `model:` → CLI flags (highest priority).

#### 0a. `ail-core/src/config/dto.rs` — parse new YAML fields

```rust
#[derive(Deserialize)]
pub struct PipelineFileDto {
    pub version: Option<String>,
    pub defaults: Option<DefaultsDto>,    // NEW
    pub pipeline: Option<Vec<StepDto>>,
}

#[derive(Deserialize)]
pub struct DefaultsDto {                   // NEW
    pub model: Option<String>,
    pub provider: Option<ProviderDto>,
}

#[derive(Deserialize)]
pub struct ProviderDto {                   // NEW
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
}

// Add to StepDto:
pub model: Option<String>,                // NEW (per-step override)
```

#### 0b. `ail-core/src/config/domain.rs` — domain types

```rust
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
}

// Add to Pipeline:
pub defaults: ProviderConfig,             // NEW (from top-level defaults:)

// Add to Step:
pub model: Option<String>,               // NEW (per-step override)
```

#### 0c. `ail-core/src/config/validation.rs` — dto→domain conversion

Convert `DefaultsDto` + `ProviderDto` → `ProviderConfig` on `Pipeline`.
Convert `StepDto.model` → `Step.model`.

#### 0d. `ail-core/src/runner/mod.rs` — flow model+provider through InvokeOptions

```rust
pub struct InvokeOptions {
    pub resume_session_id: Option<String>,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub model: Option<String>,             // NEW
    pub base_url: Option<String>,          // NEW
    pub auth_token: Option<String>,        // NEW
}
```

#### 0e. `ail-core/src/runner/claude.rs` — use model+provider from InvokeOptions

In `invoke()`, after existing args:
```rust
if let Some(ref model) = options.model {
    args.push("--model".into());
    args.push(model.clone());
}
```

On the `Command` builder:
```rust
let mut cmd = Command::new(&self.claude_bin);
cmd.args(&args).env_remove("CLAUDECODE");
if let Some(ref url) = options.base_url {
    cmd.env("ANTHROPIC_BASE_URL", url);
}
if let Some(ref token) = options.auth_token {
    cmd.env("ANTHROPIC_AUTH_TOKEN", token);
}
```

Env vars are scoped to the child process via `Command::env()` — never exported globally.

#### 0f. `ail-core/src/executor.rs` — resolve model+provider per step

When building `InvokeOptions` for each step, merge:
1. `session.pipeline.defaults` (YAML defaults)
2. `step.model` (per-step override)
3. A new `cli_provider: Option<ProviderConfig>` field on `Session` (CLI overrides, highest priority)

```rust
let model = cli_provider.model
    .or(step.model.clone())
    .or(session.pipeline.defaults.model.clone());
```

#### 0g. `ail/src/cli.rs` — CLI override flags

```rust
#[arg(long)]
pub model: Option<String>,

#[arg(long)]
pub provider_url: Option<String>,

#[arg(long)]
pub provider_token: Option<String>,
```

#### 0h. `ail/src/main.rs` — wire CLI flags into Session

Build a `ProviderConfig` from CLI flags and attach to the Session so the executor can apply it as highest-priority override.

**Usage — YAML-only (zero CLI flags needed):**
```yaml
defaults:
  model: gemma3:1b
  provider:
    base_url: http://localhost:11434
    auth_token: ollama
pipeline:
  - id: review
    prompt: "Review the code"
```

**Usage — CLI override (takes precedence over any YAML):**
```bash
ail --model gemma3:1b --provider-url http://localhost:11434 --provider-token ollama --once "hello"
```

### 1. `ail-core/src/runner/mod.rs` — new types + trait method

```rust
#[derive(Debug, Clone)]
pub enum RunnerEvent {
    StreamDelta { text: String },
    ToolUse { tool_name: String },
    ToolResult { tool_name: String },
    CostUpdate { cost_usd: f64, input_tokens: u64, output_tokens: u64 },
    Completed(RunResult),
    Error(String),
}

// Add Clone derive to RunResult

// Add to Runner trait:
fn invoke_streaming(
    &self, prompt: &str, options: InvokeOptions,
    tx: std::sync::mpsc::Sender<RunnerEvent>,
) -> Result<RunResult, AilError> {
    let result = self.invoke(prompt, options)?;
    let _ = tx.send(RunnerEvent::Completed(result.clone()));
    Ok(result)
}
```

### 2. `ail-core/src/runner/claude.rs` — override invoke_streaming()

Parse the `system`/`assistant`/`user` NDJSON events currently discarded at lines 128-131 and send them as `RunnerEvent::StreamDelta` through the channel. Extract `content` field from assistant events for text deltas.

### 3. `ail-core/src/executor.rs` — new control types + function

```rust
pub struct ExecutionControl {
    pub pause_requested: Arc<AtomicBool>,
    pub kill_requested: Arc<AtomicBool>,
}

pub enum ExecutorEvent {
    StepStarted { step_id: String, step_index: usize, total_steps: usize },
    StepCompleted { step_id: String, cost_usd: Option<f64> },
    StepSkipped { step_id: String, reason: String },
    StepFailed { step_id: String, error: String },
    HitlGateReached { step_id: String },
    RunnerEvent(RunnerEvent),
    PipelineCompleted(ExecuteOutcome),
    PipelineError(String),
}

pub fn execute_with_control(
    session: &mut Session,
    runner: &dyn Runner,
    control: &ExecutionControl,
    disabled_steps: &HashSet<String>,
    event_tx: mpsc::Sender<ExecutorEvent>,
    hitl_rx: mpsc::Receiver<String>,
) -> Result<ExecuteOutcome, AilError>
```

Reuses the step-processing logic from `execute()` (extract shared helper), adds: flag checks between steps, skip logic for disabled steps, channel sends for lifecycle events, forwarding of `RunnerEvent`s from `invoke_streaming()`, blocking on `hitl_rx` when `PauseForHuman` is reached.

---

## Implementation Milestones

### M-pre: Provider/model config for Ollama testing
This is a prerequisite for comfortable TUI development — enables free local testing. It's also the first implementation of the specced provider system (s03, s05, s15).

**ail-core changes (steps 1-5):**
1. `dto.rs`: Add `DefaultsDto`, `ProviderDto` structs; add `defaults` to `PipelineFileDto`; add `model` to `StepDto`
2. `domain.rs`: Add `ProviderConfig` struct; add `defaults: ProviderConfig` to `Pipeline`; add `model` to `Step`
3. `validation.rs`: Convert new dto fields to domain types
4. `runner/mod.rs`: Add `model`, `base_url`, `auth_token` fields to `InvokeOptions` (keep `Default` impl working)
5. `runner/claude.rs`: Use new `InvokeOptions` fields to set `--model` arg and `Command::env()` for `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN`
6. `executor.rs`: Resolve model+provider per step (defaults → step override → CLI override). Add `cli_provider` to `Session` or as parameter.

**ail binary changes (steps 7-8):**
7. `cli.rs`: Add `--model`, `--provider-url`, `--provider-token` flags
8. `main.rs`: Build `ProviderConfig` from CLI flags, attach to Session for executor to use as highest-priority override

**Spec + docs (step 9):**
9. Update `spec/core/s03*.md` (file format — `defaults:` block now parsed), `spec/core/s05*.md` (step `model:` field), `spec/core/s15*.md` (provider config), `spec/runner/r02*.md` (`--model` flag, env vars), `ail-core/CLAUDE.md`

**Tests (step 10):**
10. Add fixture YAML with `defaults:` block + per-step `model:` override. Unit test: model resolution precedence (defaults < step < CLI). Integration test (`#[ignore]`): Ollama invocation.

- **Verify (YAML-driven):** Create a test `.ail.yaml` with `defaults: { model: gemma3:1b, provider: { base_url: ..., auth_token: ollama } }`, run `ail --once "say hello"` — uses Ollama with zero CLI flags.
- **Verify (CLI override):** `ail --model gemma3:1b --provider-url http://localhost:11434 --provider-token ollama --once "say hello"` — overrides any YAML config.
- **Verify (no regression):** `ail --once "say hello"` with no flags and no `defaults:` in YAML — uses default Anthropic API as before.

### M0: Skeleton — "hello ratatui" (no ail-core changes)
1. Add `ratatui`/`crossterm` to `ail/Cargo.toml`
2. Create `tui/mod.rs`: `pub fn run(cli: &Cli) -> Result<()>` with crossterm raw mode + alternate screen
3. Create `tui/app.rs`: `AppState { running: bool }`
4. Create `tui/theme.rs`: glyph constants (`○ ● ✓ ✗ ⊘ ⊖ ◉ ◇`), color constants per §2
5. Create `tui/input.rs`: map `q`/`Ctrl-C` to quit
6. Create `tui/ui/mod.rs`: `draw()` renders centered "ail v0.1" text
7. Wire `main.rs:146` to call `tui::run(&cli)` instead of the stub
- **Verify:** `cargo run` launches TUI, `q` exits. `cargo run -- --once "hello"` still works.

### M1: Static layout with responsive regions
1. Create `tui/ui/layout.rs`: compute `Rect` regions for sidebar/viewport/statusbar/prompt based on terminal width (4 tiers from §2.4)
2. Create stub renderers: `sidebar.rs`, `viewport.rs`, `statusbar.rs`, `prompt.rs` — each takes a `Rect` and renders placeholder text
3. Handle `crossterm::event::Event::Resize` to re-layout
- **Verify:** Resize terminal, observe layout tiers degrade correctly

### M2: Pipeline sidebar from real YAML
1. Add `Pipeline` and `Vec<StepState>` (id, glyph, color) to `AppState`
2. Load pipeline via `ail_core::config::load()` + discovery logic (reuse pattern from `main.rs:60-70`)
3. Render real step names with `○` glyphs in `sidebar.rs`
- **Verify:** Run in a directory with `.ail.yaml`, see real step names

### M3: Prompt input with text editing
1. Add `input_buffer`, `cursor_pos`, `prompt_history` to `AppState`
2. Implement key handling: char insert, backspace, delete, cursor movement, word-jump, Home/End, Up/Down history, Enter submit, Shift+Enter newline
3. Render input buffer with cursor in `prompt.rs`
4. On Enter: set `pending_prompt` in `AppState`
- **Verify:** Type, edit, recall history, submit

### M4: Backend thread bridge (critical path)
1. **ail-core changes**: Add `RunnerEvent`, `invoke_streaming()`, `ExecutionControl`, `ExecutorEvent`, `execute_with_control()`, `Clone` on `RunResult`
2. Create `tui/backend.rs`:
   - `BackendCommand`: `SubmitPrompt(String)`, `Pause`, `Resume`, `KillStep`, `HitlResponse(String)`
   - `spawn_backend()`: creates channels, spawns thread, creates Session + Runner, loops on commands
   - On `SubmitPrompt`: resets step states, calls `execute_with_control()`, sends `ExecutorEvent`s
3. Update `tui/mod.rs`: spawn backend on startup, `try_recv()` on each tick, dispatch events to `AppState`
4. Update `AppState`: `execution_phase` (Idle/Running/Paused/HitlGate), update step glyphs from executor events
5. Sidebar glyphs update live: `○` → `●` → `✓`/`✗`
- **Verify:** Type prompt, watch sidebar update as steps execute. Works with `StubRunner` first, then `ClaudeCliRunner`

### M5: Streaming viewport
1. Implement `ClaudeCliRunner::invoke_streaming()` override — parse NDJSON `assistant` events, send `StreamDelta`
2. Add `viewport_lines: Vec<StyledLine>` to `AppState`, route `StreamDelta` events to it
3. Render viewport with auto-scroll; scroll-up pauses auto-scroll, new output resumes it
4. Echo user prompt in viewport before execution begins
- **Verify:** Watch Claude's response stream in real-time

### M6: Status bar live updates
1. Track `elapsed_timer`, `cumulative_cost`, `cumulative_tokens`, `current_step` in `AppState`
2. Render: `▶ claude | step 2/4: name | $0.0032 | 1,847 tok | 12.4s`
3. Idle format: `○ claude | idle | session: abc | last run: $0.01 | 4 steps`
- **Verify:** Status bar updates during and after execution

### M7: Sidebar focus + navigation + space-to-disable
1. Add `focus: Focus` (Prompt/Sidebar), `sidebar_cursor`, `disabled_steps: HashSet<String>` to `AppState`
2. Tab cycles focus; j/k/arrows navigate sidebar; q returns to prompt
3. Space toggles disabled (`⊖` ↔ `○`); no-op on completed/running/HITL steps
4. Pass `disabled_steps` to backend when submitting prompt
5. Visual: cursor highlight on focused step, `⊖` glyph for disabled
- **Verify:** Tab to sidebar, navigate, space to disable, run pipeline — disabled step skipped

### M8: Step Detail HUD
1. Enter on sidebar step opens HUD overlay in viewport area
2. Render step config: type, prompt text (raw templates before execution, expanded after), tools, on_result, timeout
3. For completed steps: duration, cost, tokens, `[view output]` hint
4. Scrollable for long prompts; Escape/movement dismisses
5. Shared modal chrome in `modal.rs`
- **Verify:** Inspect step config via HUD, see expanded templates after execution

### M9: Session navigation (Ctrl+P / Ctrl+N)
1. Buffer step outputs separately: `step_outputs: HashMap<String, Vec<StyledLine>>`
2. Route incoming `StreamDelta`/`StepStarted` to the correct step buffer
3. `Ctrl+P` shows previous step output, `Ctrl+N` next/live
4. `viewing_step` indicator in status bar, sidebar highlights viewed step
- **Verify:** Multi-step pipeline → Ctrl+P to review earlier steps

### M10: HITL gate presentation
1. `HitlGateReached` event → set `execution_phase: HitlGate`, glyph `◉`
2. Render gate message in bordered box in viewport (distinct from agent output)
3. Status bar: `⏸ HITL — step: name`
4. Prompt activates with contextual hint
5. Enter (empty) = approve/continue; Enter (with text) = send feedback via `BackendCommand::HitlResponse`
6. Backend thread unblocks on `hitl_rx.recv()` and resumes execution
- **Verify:** Pipeline with `action: pause_for_human` → interactive gate appears, respond, pipeline continues

### M11: Interrupt system
1. **11a** Escape → set `pause_requested` atomic. Executor checks between events, enters paused state. TUI buffers incoming events.
2. **11b** Render PAUSED banner + three-option modal
3. **11c** Path A: second Escape → clear flag, flush buffer, resume
4. **11d** Path B: type guidance + Enter → queue for next step prompt, insert `── ✎ user guidance injected ──` marker
5. **11e** Path C: Ctrl+K → set `kill_requested`, executor sends SIGKILL to child, step glyph `✗`
6. **11f** Destructive action warning modal (2+ steps, >500 tokens threshold)
- **Verify:** Escape mid-step → modal → all three paths work correctly

### M12: Terminal width degradation (polish)
- Already mostly handled by M1 layout tiers
- Add: step status in status bar when sidebar hidden (80-99 cols)
- Add: minimal mode below 80 cols (viewport + prompt only)

---

## Verification Plan

1. **Unit tests:** `AppState` update logic (glyph transitions, focus cycling, disable toggle) — pure functions, no TUI rendering
2. **Integration with StubRunner:** Run full TUI loop with `StubRunner` to verify thread bridge, event flow, and state transitions without needing `claude` CLI
3. **Manual test:** Run against demo pipeline (`demo/.ail.yaml`) with `ClaudeCliRunner` — observe streaming, sidebar updates, status bar, interrupt
4. **Width test:** Manually resize terminal through all four tiers during execution
5. **Existing tests:** `cargo nextest run` must stay green — all ail-core tests pass unchanged
6. **Clippy + fmt:** `cargo clippy -- -D warnings` and `cargo fmt --check` clean

---

## Critical Files

| File | Change Type |
|---|---|
| `ail-core/src/config/dto.rs` | Add `DefaultsDto`, `ProviderDto`, step `model` field |
| `ail-core/src/config/domain.rs` | Add `ProviderConfig`, pipeline `defaults`, step `model` |
| `ail-core/src/config/validation.rs` | Convert new dto→domain fields |
| `ail-core/src/runner/mod.rs` | Add model/provider to `InvokeOptions`; add streaming types (M4) |
| `ail-core/src/runner/claude.rs` | Use model/provider from `InvokeOptions` in Command builder |
| `ail-core/src/executor.rs` | Resolve model+provider per step; add control types (M4) |
| `ail/src/cli.rs` | Add `--model`, `--provider-url`, `--provider-token` flags |
| `ail/src/main.rs` | Wire provider flags; wire `tui::run()` |
| `ail/Cargo.toml` | Add ratatui, crossterm |
| `ail/src/tui/**` (new) | All TUI code (~12 new files) |
| `ail-core/src/runner/mod.rs` | Additive: `RunnerEvent`, `invoke_streaming()`, `Clone` on `RunResult` |
| `ail-core/src/runner/claude.rs` | Additive: `invoke_streaming()` override |
| `ail-core/src/executor.rs` | Additive: `ExecutionControl`, `ExecutorEvent`, `execute_with_control()` |
| `ail-core/CLAUDE.md` | Update module responsibilities + key types for new additions |
| `spec/runner/r02*.md` | Update: document `--model` flag, provider env vars |
| `spec/core/s04*.md` | Update execution model to document `execute_with_control()` |
| `spec/core/s13*.md` | Update HITL gates to document TUI presentation |

---

## Dependency Graph

```
M-pre (provider/model config — enables free local testing)
 └─ M0 (skeleton)
     └─ M1 (layout)
         ├─ M2 (sidebar from YAML)
         │   └─ M7 (focus + nav + disable)
         │       └─ M8 (HUD)
         ├─ M3 (prompt input)
         └─ M4 (backend bridge) ← critical path
             ├─ M5 (streaming viewport)
             ├─ M6 (status bar live)
             ├─ M9 (session navigation)
             ├─ M10 (HITL gates)
             └─ M11 (interrupt system)
M12 (width degradation — polish, mostly done in M1)
```

**Recommended build order:** M-pre → M0 → M1 → M2 + M3 (parallel) → M4 → M5 + M6 + M7 (parallel) → M8 + M9 + M10 (parallel) → M11 → M12

**Testing workflow after M-pre:** All subsequent TUI milestones can be manually tested with:
```bash
ail --model gemma3:1b --provider-url http://localhost:11434 --provider-token ollama
```
This runs the TUI against local Ollama — zero API cost, unlimited iterations.
