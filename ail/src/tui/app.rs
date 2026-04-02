use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use ail_core::config::discovery::PipelineEntry;
use ail_core::config::domain::Pipeline;
use ail_core::executor::ExecutorEvent;
use ail_core::runner::{PermissionRequest, PermissionResponse};

/// State of a single pipeline step as displayed in the sidebar.
#[derive(Debug, Clone)]
pub struct StepDisplay {
    pub id: String,
    pub glyph: StepGlyph,
}

/// The visual state of a step glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StepGlyph {
    NotReached,
    Running,
    Completed,
    Failed,
    Skipped,    // condition-skipped (⊘)
    Disabled,   // user-disabled (⊖)
    HitlPaused, // ◉
}

/// High-level execution phase shown in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ExecutionPhase {
    Idle,
    Running,
    Paused,
    HitlGate,
    Completed,
    Failed,
}

/// A description of a side effect to be executed by the event loop.
///
/// Methods that previously performed side effects directly (channel sends, atomic writes)
/// now return `Vec<SideEffect>` so they can be tested without real channels or flags.
/// The event loop calls `execute_effects` to apply them.
#[derive(Debug, Clone, PartialEq)]
pub enum SideEffect {
    SendPermissionResponse(PermissionResponse),
    SetPauseFlag(bool),
    SetKillFlag,
}

/// Action to take on a pending prompt, returned by `resolve_pending_prompt`.
pub enum PromptAction {
    SendHitl(String),
    SubmitToBackend {
        prompt: String,
        disabled_steps: HashSet<String>,
    },
    None,
}

// ── Sub-structs ────────────────────────────────────────────────────────────────

/// Prompt input state (M3).
pub struct PromptState {
    pub input_buffer: Vec<char>,
    pub cursor_pos: usize,
    /// Sticky column for Up/Down navigation; cleared by any horizontal or edit action.
    pub prompt_preferred_col: Option<usize>,
    pub prompt_history: Vec<String>,
    pub history_index: Option<usize>,
    /// Set when the user presses Enter; cleared by the main loop once consumed.
    pub pending_prompt: Option<String>,
}

impl PromptState {
    fn new() -> Self {
        PromptState {
            input_buffer: Vec::new(),
            cursor_pos: 0,
            prompt_preferred_col: None,
            prompt_history: Vec::new(),
            history_index: None,
            pending_prompt: None,
        }
    }

    #[allow(dead_code)]
    pub fn input_str(&self) -> String {
        self.input_buffer.iter().collect()
    }

    pub fn input_insert(&mut self, c: char) {
        self.prompt_preferred_col = None;
        self.input_buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn input_backspace(&mut self) {
        self.prompt_preferred_col = None;
        if self.cursor_pos > 0 {
            self.input_buffer.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }

    pub fn input_delete(&mut self) {
        self.prompt_preferred_col = None;
        if self.cursor_pos < self.input_buffer.len() {
            self.input_buffer.remove(self.cursor_pos);
        }
    }

    pub fn cursor_left(&mut self) {
        self.prompt_preferred_col = None;
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn cursor_right(&mut self) {
        self.prompt_preferred_col = None;
        if self.cursor_pos < self.input_buffer.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn cursor_home(&mut self) {
        self.prompt_preferred_col = None;
        self.cursor_pos = 0;
    }

    pub fn cursor_end(&mut self) {
        self.prompt_preferred_col = None;
        self.cursor_pos = self.input_buffer.len();
    }

    pub fn cursor_word_left(&mut self) {
        self.prompt_preferred_col = None;
        if self.cursor_pos == 0 {
            return;
        }
        let mut pos = self.cursor_pos - 1;
        while pos > 0 && self.input_buffer[pos] == ' ' {
            pos -= 1;
        }
        while pos > 0 && self.input_buffer[pos - 1] != ' ' {
            pos -= 1;
        }
        self.cursor_pos = pos;
    }

    pub fn cursor_word_right(&mut self) {
        self.prompt_preferred_col = None;
        let len = self.input_buffer.len();
        if self.cursor_pos >= len {
            return;
        }
        let mut pos = self.cursor_pos;
        while pos < len && self.input_buffer[pos] != ' ' {
            pos += 1;
        }
        while pos < len && self.input_buffer[pos] == ' ' {
            pos += 1;
        }
        self.cursor_pos = pos;
    }

    /// Move the cursor up one logical line within the buffer, preserving preferred column.
    /// Returns `true` if the cursor moved, `false` if already on the first line
    /// (caller should fall through to history navigation).
    pub fn cursor_up_line(&mut self) -> bool {
        let pos = self.cursor_pos;
        let line_start = self.input_buffer[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        if line_start == 0 {
            return false;
        }
        let col = pos - line_start;
        let preferred = *self.prompt_preferred_col.get_or_insert(col);
        let prev_line_end = line_start - 1;
        let prev_line_start = self.input_buffer[..prev_line_end]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prev_line_len = prev_line_end - prev_line_start;
        self.cursor_pos = prev_line_start + preferred.min(prev_line_len);
        true
    }

    /// Move the cursor down one logical line within the buffer, preserving preferred column.
    /// Returns `true` if the cursor moved, `false` if already on the last line
    /// (caller should fall through to history navigation).
    pub fn cursor_down_line(&mut self) -> bool {
        let pos = self.cursor_pos;
        let line_start = self.input_buffer[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let col = pos - line_start;
        let preferred = *self.prompt_preferred_col.get_or_insert(col);
        let buf_len = self.input_buffer.len();
        let next_newline = self.input_buffer[pos..]
            .iter()
            .position(|&c| c == '\n')
            .map(|i| pos + i);
        match next_newline {
            None => false,
            Some(nl) => {
                let next_line_start = nl + 1;
                let next_line_end = self.input_buffer[next_line_start..]
                    .iter()
                    .position(|&c| c == '\n')
                    .map(|i| next_line_start + i)
                    .unwrap_or(buf_len);
                let next_line_len = next_line_end - next_line_start;
                self.cursor_pos = next_line_start + preferred.min(next_line_len);
                true
            }
        }
    }

    pub fn submit_input(&mut self) {
        let text: String = self.input_buffer.iter().collect();
        if text.trim().is_empty() {
            return;
        }
        self.prompt_history.push(text.clone());
        self.input_buffer.clear();
        self.cursor_pos = 0;
        self.prompt_preferred_col = None;
        self.history_index = None;
        self.pending_prompt = Some(text);
    }

    pub fn history_up(&mut self) {
        if self.prompt_history.is_empty() {
            return;
        }
        let new_index = match self.history_index {
            None => self.prompt_history.len() - 1,
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
        };
        self.history_index = Some(new_index);
        let entry: Vec<char> = self.prompt_history[new_index].chars().collect();
        self.cursor_pos = entry.len();
        self.prompt_preferred_col = None;
        self.input_buffer = entry;
    }

    pub fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(i) if i + 1 < self.prompt_history.len() => {
                let new_index = i + 1;
                self.history_index = Some(new_index);
                let entry: Vec<char> = self.prompt_history[new_index].chars().collect();
                self.cursor_pos = entry.len();
                self.prompt_preferred_col = None;
                self.input_buffer = entry;
            }
            Some(_) => {
                self.history_index = None;
                self.input_buffer.clear();
                self.cursor_pos = 0;
                self.prompt_preferred_col = None;
            }
        }
    }
}

/// Pipeline picker state (i-1).
pub struct PickerState {
    pub open: bool,
    /// Full list from `discover_all()`, populated at TUI startup.
    pub entries: Vec<PipelineEntry>,
    /// Characters typed after `:` for prefix filtering.
    pub filter: String,
    /// Indices into `entries` that match the current filter.
    pub filtered: Vec<usize>,
    /// Index into `filtered` for the highlighted row.
    pub cursor: usize,
    /// Set by `select()`; consumed by the main loop to hot-reload.
    pub pending_pipeline_switch: Option<PathBuf>,
}

impl PickerState {
    fn new() -> Self {
        PickerState {
            open: false,
            entries: Vec::new(),
            filter: String::new(),
            filtered: Vec::new(),
            cursor: 0,
            pending_pipeline_switch: None,
        }
    }

    pub fn open_picker(&mut self) {
        self.open = true;
        self.filter.clear();
        self.cursor = 0;
        self.update_filter();
    }

    pub fn close_picker(&mut self) {
        self.open = false;
        self.filter.clear();
        self.filtered.clear();
        self.cursor = 0;
    }

    /// Recompute `filtered` from the current `filter`.
    /// Case-insensitive prefix match on entry name.
    fn update_filter(&mut self) {
        let filter = self.filter.to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name.to_lowercase().starts_with(&filter))
            .map(|(i, _)| i)
            .collect();
        self.cursor = 0;
    }

    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.update_filter();
    }

    pub fn backspace(&mut self) {
        if self.filter.is_empty() {
            self.close_picker();
        } else {
            self.filter.pop();
            self.update_filter();
        }
    }

    pub fn nav_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn nav_down(&mut self) {
        if !self.filtered.is_empty() && self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    /// Return the path of the currently selected entry and close the picker.
    /// Returns `None` if the filtered list is empty.
    pub fn select(&mut self) -> Option<PathBuf> {
        let entry_idx = *self.filtered.get(self.cursor)?;
        let path = self.entries[entry_idx].path.clone();
        self.close_picker();
        self.pending_pipeline_switch = Some(path.clone());
        Some(path)
    }
}

/// Tool permission HITL state (SPEC §13.3).
pub struct PermissionState {
    /// Channel to send permission decisions back to the listener thread.
    pub tx: Option<mpsc::Sender<PermissionResponse>>,
    /// The pending permission request being shown in the modal.
    pub request: Option<PermissionRequest>,
    /// Whether the permission modal is visible.
    pub modal_open: bool,
    /// Cursor position in the permission modal (0=approve once, 1=allow session, 2=deny).
    pub cursor: usize,
    /// Tools approved for the rest of this session (auto-approved without prompting).
    pub session_allowlist: HashSet<String>,
}

impl PermissionState {
    fn new() -> Self {
        PermissionState {
            tx: None,
            request: None,
            modal_open: false,
            cursor: 0,
            session_allowlist: HashSet::new(),
        }
    }

    pub fn nav_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn nav_down(&mut self) {
        self.cursor = (self.cursor + 1).min(2);
    }

    fn reset(&mut self) {
        self.modal_open = false;
        self.request = None;
        // session_allowlist persists across runs within the same TUI session.
        // tx is set fresh each run via BackendEvent::PermReady.
    }
}

/// Interrupt system state (M11).
pub struct InterruptState {
    /// Pause flag shared with the executor; TUI sets it to request pause between steps.
    pub pause_flag: Option<Arc<AtomicBool>>,
    /// Kill flag shared with the executor; TUI sets it to abort after current step.
    pub kill_flag: Option<Arc<AtomicBool>>,
    /// Whether the 3-option interrupt modal is currently showing.
    pub modal_open: bool,
    /// Path B: guidance text typed during a pause, to be echoed in the viewport.
    pub pending_injection: Option<String>,
}

impl InterruptState {
    fn new() -> Self {
        InterruptState {
            pause_flag: None,
            kill_flag: None,
            modal_open: false,
            pending_injection: None,
        }
    }

    fn reset(&mut self) {
        self.modal_open = false;
        self.pending_injection = None;
        self.pause_flag = None;
        self.kill_flag = None;
    }
}

/// Viewport display state (M5, M9).
pub struct ViewportState {
    /// Accumulated display lines. Each entry may be a separator, prompt echo, or output line.
    pub lines: Vec<String>,
    /// Lines scrolled up from the bottom (0 = auto-scroll to latest output).
    pub scroll: u16,
    pub active_step_id: Option<String>,
    /// True once at least one StreamDelta has arrived for the current step.
    pub step_streamed: bool,
    /// Output lines keyed by step_id. Populated as steps run.
    pub step_outputs: HashMap<String, Vec<String>>,
    /// Ordered list of step IDs as they were started, for Ctrl+P/N navigation.
    pub step_order: Vec<String>,
    /// None = live view (auto-scroll to latest). Some(i) = viewing step_order[i].
    pub viewing_step: Option<usize>,
}

impl ViewportState {
    fn new() -> Self {
        ViewportState {
            lines: Vec::new(),
            scroll: 0,
            active_step_id: None,
            step_streamed: false,
            step_outputs: HashMap::new(),
            step_order: Vec::new(),
            viewing_step: None,
        }
    }

    /// Append `text` (which may contain `\n`) to `lines` and the active step buffer.
    /// When the user has scrolled up (scroll > 0), compensates so the viewed
    /// position stays fixed rather than drifting toward the new bottom.
    pub fn append_text(&mut self, text: &str) {
        let before = self.lines.len();
        Self::append_to(text, &mut self.lines);
        let added = self.lines.len().saturating_sub(before) as u16;
        if self.scroll > 0 {
            self.scroll = self.scroll.saturating_add(added);
        }
        if let Some(ref id) = self.active_step_id.clone() {
            if let Some(buf) = self.step_outputs.get_mut(id) {
                Self::append_to(text, buf);
            }
        }
    }

    /// Returns the number of new lines pushed.
    fn append_to(text: &str, target: &mut Vec<String>) {
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            if !first.is_empty() {
                if let Some(last) = target.last_mut() {
                    last.push_str(first);
                } else {
                    target.push(first.to_string());
                }
            }
        }
        for part in parts {
            target.push(part.to_string());
        }
    }

    /// Echo the user's prompt in the viewport before the pipeline runs.
    pub fn echo_prompt(&mut self, prompt: &str) {
        if !self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.lines.push(format!("> {prompt}"));
        self.scroll = 0;
    }

    /// Navigate to the previous completed step's output (Ctrl+P).
    pub fn session_prev(&mut self) {
        if self.step_order.is_empty() {
            return;
        }
        let new_idx = match self.viewing_step {
            None => self.step_order.len().saturating_sub(1),
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
        };
        self.viewing_step = Some(new_idx);
        self.scroll = 0;
    }

    /// Navigate to the next step (or back to live view) with Ctrl+N.
    pub fn session_next(&mut self) {
        match self.viewing_step {
            None => {}
            Some(i) if i + 1 < self.step_order.len() => {
                self.viewing_step = Some(i + 1);
                self.scroll = 0;
            }
            Some(_) => {
                self.viewing_step = None;
                self.scroll = 0;
            }
        }
    }

    fn reset(&mut self) {
        self.active_step_id = None;
        self.step_streamed = false;
        self.step_outputs.clear();
        self.step_order.clear();
        self.viewing_step = None;
    }
}

/// Run statistics (M6).
pub struct RunStats {
    pub run_start: Option<std::time::Instant>,
    pub run_end: Option<std::time::Instant>,
    pub cumulative_cost_usd: f64,
    pub cumulative_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    pub current_step_index: usize,
    pub total_steps: usize,
    pub last_session_id: Option<String>,
}

impl RunStats {
    fn new() -> Self {
        RunStats {
            run_start: None,
            run_end: None,
            cumulative_cost_usd: 0.0,
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
            current_step_index: 0,
            total_steps: 0,
            last_session_id: None,
        }
    }

    fn reset(&mut self) {
        self.run_start = None;
        self.run_end = None;
        self.cumulative_cost_usd = 0.0;
        self.cumulative_input_tokens = 0;
        self.cumulative_output_tokens = 0;
        self.current_step_index = 0;
        self.total_steps = 0;
    }
}

/// Sidebar display state (display-only, no interactive focus).
pub struct SidebarState {
    pub cursor: usize,
    pub disabled_steps: HashSet<String>,
}

impl SidebarState {
    fn new() -> Self {
        SidebarState {
            cursor: 0,
            disabled_steps: HashSet::new(),
        }
    }
}

// ── AppState ───────────────────────────────────────────────────────────────────

/// Application state for the TUI.
pub struct AppState {
    pub running: bool,
    #[allow(dead_code)]
    pub pipeline: Option<Pipeline>,
    pub steps: Vec<StepDisplay>,
    pub phase: ExecutionPhase,
    pub prompt: PromptState,
    pub viewport: ViewportState,
    pub picker: PickerState,
    pub permissions: PermissionState,
    pub interrupt: InterruptState,
    pub stats: RunStats,
    pub sidebar: SidebarState,
}

impl AppState {
    /// Build the sidebar step list for a pipeline.
    ///
    /// If the pipeline's first step is not named "invocation", a virtual display entry is
    /// prepended so the sidebar shows the full execution chain (SPEC §4.1).
    pub fn steps_for_pipeline(pipeline: &Pipeline) -> Vec<StepDisplay> {
        let has_invocation = pipeline
            .steps
            .first()
            .map(|s| s.id.as_str() == "invocation")
            .unwrap_or(false);

        let mut steps: Vec<StepDisplay> = pipeline
            .steps
            .iter()
            .map(|s| StepDisplay {
                id: s.id.as_str().to_string(),
                glyph: StepGlyph::NotReached,
            })
            .collect();

        if !has_invocation {
            steps.insert(
                0,
                StepDisplay {
                    id: "invocation".to_string(),
                    glyph: StepGlyph::NotReached,
                },
            );
        }
        steps
    }

    pub fn new(pipeline: Option<Pipeline>) -> Self {
        let steps = pipeline
            .as_ref()
            .map(Self::steps_for_pipeline)
            .unwrap_or_default();

        AppState {
            running: true,
            pipeline,
            steps,
            phase: ExecutionPhase::Idle,
            prompt: PromptState::new(),
            viewport: ViewportState::new(),
            picker: PickerState::new(),
            permissions: PermissionState::new(),
            interrupt: InterruptState::new(),
            stats: RunStats::new(),
            sidebar: SidebarState::new(),
        }
    }

    // ── executor event handling ───────────────────────────────────────────────

    /// Apply an `ExecutorEvent` from the backend thread to the UI state.
    pub fn apply_executor_event(&mut self, ev: ExecutorEvent) {
        match ev {
            ExecutorEvent::StepStarted {
                ref step_id,
                step_index,
                total_steps,
                ..
            } => {
                self.phase = ExecutionPhase::Running;
                self.viewport.active_step_id = Some(step_id.clone());
                self.viewport.step_streamed = false;
                // Register step in per-step output buffers (M9).
                if !self.viewport.step_outputs.contains_key(step_id.as_str()) {
                    self.viewport.step_order.push(step_id.clone());
                    self.viewport
                        .step_outputs
                        .insert(step_id.clone(), Vec::new());
                }
                self.stats.current_step_index = step_index;
                self.stats.total_steps = total_steps;
                if self.stats.run_start.is_none() {
                    self.stats.run_start = Some(std::time::Instant::now());
                }
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Running;
                    }
                }
                // Step separator in the viewport.
                if !self.viewport.lines.is_empty() {
                    self.viewport.lines.push(String::new());
                }
                self.viewport.lines.push(format!("── {} ──", step_id));
                self.viewport.scroll = 0;
            }
            ExecutorEvent::StepCompleted {
                ref step_id,
                cost_usd,
                ..
            } => {
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Completed;
                    }
                }
                if let Some(c) = cost_usd {
                    self.stats.cumulative_cost_usd += c;
                }
            }
            ExecutorEvent::StepFailed {
                ref step_id,
                ref error,
            } => {
                self.phase = ExecutionPhase::Failed;
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Failed;
                    }
                }
                self.viewport.append_text(&format!("\n[error: {error}]"));
                self.stats.run_end = Some(std::time::Instant::now());
            }
            ExecutorEvent::StepSkipped { ref step_id } => {
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Skipped;
                    }
                }
            }
            ExecutorEvent::RunnerEvent {
                event: ail_core::runner::RunnerEvent::StreamDelta { ref text },
            } => {
                self.viewport.step_streamed = true;
                self.viewport.append_text(text);
                // Do NOT reset viewport scroll here — preserves user's scroll position.
            }
            ExecutorEvent::RunnerEvent {
                event: ail_core::runner::RunnerEvent::Thinking { ref text },
            } => {
                // Prefix thinking blocks so the viewport can render them in a distinct style.
                for line in text.lines() {
                    self.viewport.append_text(&format!("\n[thinking] {line}"));
                }
            }
            ExecutorEvent::RunnerEvent {
                event: ail_core::runner::RunnerEvent::Completed(ref result),
            } => {
                if !self.viewport.step_streamed && !result.response.is_empty() {
                    // Stub runner or no streaming — show full response text now.
                    // Prepend a newline so the response starts on its own line,
                    // not concatenated with the last [thinking] line.
                    self.viewport.append_text(&format!("\n{}", result.response));
                }
                if let Some(ref sid) = result.session_id {
                    self.stats.last_session_id = Some(sid.clone());
                }
            }
            ExecutorEvent::RunnerEvent {
                event:
                    ail_core::runner::RunnerEvent::CostUpdate {
                        cost_usd,
                        input_tokens,
                        output_tokens,
                    },
            } => {
                self.stats.cumulative_cost_usd += cost_usd;
                self.stats.cumulative_input_tokens += input_tokens;
                self.stats.cumulative_output_tokens += output_tokens;
            }
            ExecutorEvent::RunnerEvent {
                event: ail_core::runner::RunnerEvent::ToolUse { ref tool_name },
            } => {
                self.viewport.lines.push(format!("  [tool: {}]", tool_name));
                // Do NOT reset viewport scroll — preserves user's scroll position.
            }
            ExecutorEvent::HitlGateReached { ref step_id } => {
                self.phase = ExecutionPhase::HitlGate;
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::HitlPaused;
                    }
                }
                self.viewport.lines.push(String::new());
                self.viewport
                    .lines
                    .push(format!("◉ pause_for_human — step: {step_id}"));
                self.viewport
                    .lines
                    .push("  press Enter to continue, or type feedback first".to_string());
                self.viewport.scroll = 0;
            }
            ExecutorEvent::RunnerEvent { .. } => {
                // ToolResult, Error — no viewport update needed in M5.
            }
            ExecutorEvent::PipelineCompleted(_) => {
                self.phase = ExecutionPhase::Completed;
                self.stats.run_end = Some(std::time::Instant::now());
                self.viewport.lines.push(String::new());
                self.viewport.lines.push("── done ──".to_string());
                self.viewport.scroll = 0;
            }
            ExecutorEvent::PipelineError { ref error, .. } => {
                self.phase = ExecutionPhase::Failed;
                self.stats.run_end = Some(std::time::Instant::now());
                self.viewport
                    .append_text(&format!("\n[pipeline error: {error}]"));
            }
        }
    }

    /// Reset glyphs and run statistics for a new pipeline run.
    pub fn reset_for_run(&mut self) {
        for step in &mut self.steps {
            if step.glyph != StepGlyph::Disabled {
                step.glyph = StepGlyph::NotReached;
            }
        }
        self.stats.reset();
        self.viewport.reset();
        self.interrupt.reset();
        self.permissions.reset();
    }

    // ── pending-prompt dispatch (loop logic) ──────────────────────────────────

    /// Consume `pending_prompt` and return what the event loop should do with it.
    pub fn resolve_pending_prompt(&mut self) -> PromptAction {
        match self.prompt.pending_prompt.take() {
            None => PromptAction::None,
            Some(prompt) => {
                if self.phase == ExecutionPhase::HitlGate {
                    self.phase = ExecutionPhase::Running;
                    PromptAction::SendHitl(prompt)
                } else {
                    self.viewport.echo_prompt(&prompt);
                    self.reset_for_run();
                    self.phase = ExecutionPhase::Running;
                    let disabled_steps = self.sidebar.disabled_steps.clone();
                    PromptAction::SubmitToBackend {
                        prompt,
                        disabled_steps,
                    }
                }
            }
        }
    }

    /// Apply a successful pipeline hot-reload. Returns the pipeline for the backend command.
    pub fn apply_pipeline_switch(&mut self, new_pipeline: Pipeline, name: String) -> Pipeline {
        self.steps = AppState::steps_for_pipeline(&new_pipeline);
        self.pipeline = Some(new_pipeline.clone());
        self.sidebar.cursor = 0;
        self.sidebar.disabled_steps.clear();
        self.viewport
            .lines
            .push(format!("── switched to: {name} ──"));
        new_pipeline
    }

    /// Append a pipeline load error to the viewport.
    pub fn apply_pipeline_switch_error(&mut self, detail: &str) {
        self.viewport
            .lines
            .push(format!("[pipeline load error: {detail}]"));
    }

    // ── tool permission HITL (SPEC §13.3) ────────────────────────────────────

    /// Handle an incoming permission request from the runner.
    ///
    /// If the tool is in the session allowlist, auto-approves and returns
    /// `[SendPermissionResponse(Allow)]`. Otherwise opens the modal and returns `[]`.
    pub fn handle_permission_request(&mut self, req: PermissionRequest) -> Vec<SideEffect> {
        if self
            .permissions
            .session_allowlist
            .contains(&req.display_name)
        {
            self.viewport.append_text(&format!(
                "\n  [permission: {} — auto-allowed (session)]",
                req.display_name
            ));
            return vec![SideEffect::SendPermissionResponse(
                PermissionResponse::Allow,
            )];
        }
        let detail = format!(
            "\n  [permission: {} — waiting for approval]{}",
            req.display_name, req.display_detail
        );
        self.viewport.append_text(&detail);
        self.permissions.request = Some(req);
        self.permissions.cursor = 0;
        self.permissions.modal_open = true;
        vec![]
    }

    /// Confirm the currently highlighted permission option.
    pub fn perm_confirm(&mut self) -> Vec<SideEffect> {
        match self.permissions.cursor {
            0 => self.perm_approve_once(),
            1 => self.perm_approve_session(),
            _ => self.perm_deny(),
        }
    }

    /// Approve the pending permission request for this tool call only.
    pub fn perm_approve_once(&mut self) -> Vec<SideEffect> {
        let tool = self
            .permissions
            .request
            .as_ref()
            .map(|r| r.display_name.as_str())
            .unwrap_or("?")
            .to_owned();
        self.viewport
            .append_text(&format!("\n  [permission: {tool} — approved once]"));
        self.permissions.modal_open = false;
        self.permissions.request = None;
        vec![SideEffect::SendPermissionResponse(
            PermissionResponse::Allow,
        )]
    }

    /// Approve the pending permission request and add the tool to the session allowlist.
    pub fn perm_approve_session(&mut self) -> Vec<SideEffect> {
        if let Some(ref req) = self.permissions.request {
            self.permissions
                .session_allowlist
                .insert(req.display_name.clone());
        }
        let tool = self
            .permissions
            .request
            .as_ref()
            .map(|r| r.display_name.as_str())
            .unwrap_or("?")
            .to_owned();
        self.viewport
            .append_text(&format!("\n  [permission: {tool} — approved for session]"));
        self.permissions.modal_open = false;
        self.permissions.request = None;
        vec![SideEffect::SendPermissionResponse(
            PermissionResponse::Allow,
        )]
    }

    /// Deny the pending permission request.
    pub fn perm_deny(&mut self) -> Vec<SideEffect> {
        let tool = self
            .permissions
            .request
            .as_ref()
            .map(|r| r.display_name.as_str())
            .unwrap_or("?")
            .to_owned();
        self.viewport
            .append_text(&format!("\n  [permission: {tool} — denied]"));
        self.permissions.modal_open = false;
        self.permissions.request = None;
        vec![SideEffect::SendPermissionResponse(
            PermissionResponse::Deny("User denied".to_string()),
        )]
    }

    // ── interrupt system (M11) ────────────────────────────────────────────────

    /// Path A: resume — dismiss modal, change phase. Returns `[SetPauseFlag(false)]`.
    pub fn request_resume(&mut self) -> Vec<SideEffect> {
        self.phase = ExecutionPhase::Running;
        self.interrupt.modal_open = false;
        vec![SideEffect::SetPauseFlag(false)]
    }

    /// Path B: inject guidance — echo marker in viewport, resume.
    pub fn request_inject_guidance(&mut self, text: String) -> Vec<SideEffect> {
        self.viewport.lines.push(String::new());
        self.viewport
            .lines
            .push(format!("── ✎ guidance: {} ──", text.trim()));
        self.interrupt.pending_injection = Some(text);
        self.request_resume()
    }

    /// Path C: kill step — set kill flag, clear pause flag.
    /// Returns `[SetKillFlag, SetPauseFlag(false)]`.
    pub fn request_kill(&mut self) -> Vec<SideEffect> {
        self.interrupt.modal_open = false;
        self.phase = ExecutionPhase::Running;
        vec![SideEffect::SetKillFlag, SideEffect::SetPauseFlag(false)]
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ail_core::executor::ExecutorEvent;
    use ail_core::runner::{RunResult, RunnerEvent};

    fn app() -> AppState {
        AppState::new(None)
    }

    // ── apply_executor_event ──────────────────────────────────────────────────

    #[test]
    fn step_started_registers_step_and_sets_phase() {
        let mut a = app();
        a.apply_executor_event(ExecutorEvent::StepStarted {
            step_id: "review".to_string(),
            step_index: 0,
            total_steps: 2,
            resolved_prompt: None,
        });
        assert_eq!(a.phase, ExecutionPhase::Running);
        assert_eq!(a.viewport.active_step_id.as_deref(), Some("review"));
        assert!(!a.viewport.step_streamed);
        assert!(a.viewport.step_outputs.contains_key("review"));
        assert_eq!(a.stats.current_step_index, 0);
        assert_eq!(a.stats.total_steps, 2);
        assert!(a.stats.run_start.is_some());
    }

    #[test]
    fn step_started_pushes_separator_line() {
        let mut a = app();
        a.viewport.lines.push("existing".to_string());
        a.apply_executor_event(ExecutorEvent::StepStarted {
            step_id: "s1".to_string(),
            step_index: 0,
            total_steps: 1,
            resolved_prompt: None,
        });
        assert!(a.viewport.lines.iter().any(|l| l.contains("s1")));
        // A blank separator line was inserted before the step header.
        assert!(a.viewport.lines.iter().any(|l| l.is_empty()));
    }

    #[test]
    fn step_completed_updates_glyph_and_accumulates_cost() {
        let mut a = app();
        a.steps.push(StepDisplay {
            id: "s1".to_string(),
            glyph: StepGlyph::Running,
        });
        a.apply_executor_event(ExecutorEvent::StepCompleted {
            step_id: "s1".to_string(),
            cost_usd: Some(0.005),
            input_tokens: 0,
            output_tokens: 0,
            response: None,
        });
        assert_eq!(a.steps[0].glyph, StepGlyph::Completed);
        assert!((a.stats.cumulative_cost_usd - 0.005).abs() < 1e-9);
    }

    #[test]
    fn step_failed_sets_phase_and_glyph() {
        let mut a = app();
        a.steps.push(StepDisplay {
            id: "s1".to_string(),
            glyph: StepGlyph::Running,
        });
        a.apply_executor_event(ExecutorEvent::StepFailed {
            step_id: "s1".to_string(),
            error: "oops".to_string(),
        });
        assert_eq!(a.phase, ExecutionPhase::Failed);
        assert_eq!(a.steps[0].glyph, StepGlyph::Failed);
        assert!(a.stats.run_end.is_some());
        assert!(a.viewport.lines.iter().any(|l| l.contains("oops")));
    }

    #[test]
    fn step_skipped_updates_glyph() {
        let mut a = app();
        a.steps.push(StepDisplay {
            id: "s1".to_string(),
            glyph: StepGlyph::NotReached,
        });
        a.apply_executor_event(ExecutorEvent::StepSkipped {
            step_id: "s1".to_string(),
        });
        assert_eq!(a.steps[0].glyph, StepGlyph::Skipped);
    }

    #[test]
    fn stream_delta_appends_text_and_sets_streamed() {
        let mut a = app();
        a.apply_executor_event(ExecutorEvent::RunnerEvent {
            event: RunnerEvent::StreamDelta {
                text: "hello".to_string(),
            },
        });
        assert!(a.viewport.step_streamed);
        assert!(a.viewport.lines.iter().any(|l| l.contains("hello")));
    }

    #[test]
    fn cost_update_accumulates_tokens() {
        let mut a = app();
        a.apply_executor_event(ExecutorEvent::RunnerEvent {
            event: RunnerEvent::CostUpdate {
                cost_usd: 0.01,
                input_tokens: 100,
                output_tokens: 50,
            },
        });
        assert!((a.stats.cumulative_cost_usd - 0.01).abs() < 1e-9);
        assert_eq!(a.stats.cumulative_input_tokens, 100);
        assert_eq!(a.stats.cumulative_output_tokens, 50);
    }

    #[test]
    fn pipeline_completed_sets_phase_and_run_end() {
        let mut a = app();
        a.apply_executor_event(ExecutorEvent::PipelineCompleted(
            ail_core::executor::ExecuteOutcome::Completed,
        ));
        assert_eq!(a.phase, ExecutionPhase::Completed);
        assert!(a.stats.run_end.is_some());
        assert!(a.viewport.lines.iter().any(|l| l.contains("done")));
    }

    #[test]
    fn hitl_gate_sets_phase_and_glyph() {
        let mut a = app();
        a.steps.push(StepDisplay {
            id: "gate".to_string(),
            glyph: StepGlyph::Running,
        });
        a.apply_executor_event(ExecutorEvent::HitlGateReached {
            step_id: "gate".to_string(),
        });
        assert_eq!(a.phase, ExecutionPhase::HitlGate);
        assert_eq!(a.steps[0].glyph, StepGlyph::HitlPaused);
    }

    #[test]
    fn completed_runner_event_stores_session_id() {
        let mut a = app();
        a.apply_executor_event(ExecutorEvent::RunnerEvent {
            event: RunnerEvent::Completed(RunResult {
                response: "done".to_string(),
                cost_usd: None,
                session_id: Some("abc123".to_string()),
                input_tokens: 0,
                output_tokens: 0,
                thinking: None,
            }),
        });
        assert_eq!(a.stats.last_session_id.as_deref(), Some("abc123"));
    }

    // ── prompt input ──────────────────────────────────────────────────────────

    #[test]
    fn input_insert_adds_char_and_advances_cursor() {
        let mut a = app();
        a.prompt.input_insert('h');
        a.prompt.input_insert('i');
        assert_eq!(a.prompt.input_str(), "hi");
        assert_eq!(a.prompt.cursor_pos, 2);
    }

    #[test]
    fn input_backspace_removes_char() {
        let mut a = app();
        a.prompt.input_insert('h');
        a.prompt.input_insert('i');
        a.prompt.input_backspace();
        assert_eq!(a.prompt.input_str(), "h");
        assert_eq!(a.prompt.cursor_pos, 1);
    }

    #[test]
    fn submit_input_clears_buffer_and_sets_pending_prompt() {
        let mut a = app();
        for c in "hello".chars() {
            a.prompt.input_insert(c);
        }
        a.prompt.submit_input();
        assert!(a.prompt.input_buffer.is_empty());
        assert_eq!(a.prompt.pending_prompt.as_deref(), Some("hello"));
        assert_eq!(
            a.prompt.prompt_history.last().map(|s| s.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn submit_input_empty_is_no_op() {
        let mut a = app();
        a.prompt.submit_input();
        assert!(a.prompt.pending_prompt.is_none());
    }

    #[test]
    fn history_up_loads_previous_entry() {
        let mut a = app();
        a.prompt.prompt_history.push("first".to_string());
        a.prompt.history_up();
        assert_eq!(a.prompt.input_str(), "first");
        assert_eq!(a.prompt.history_index, Some(0));
    }

    #[test]
    fn history_down_clears_on_past_end() {
        let mut a = app();
        a.prompt.prompt_history.push("first".to_string());
        a.prompt.history_up();
        a.prompt.history_down();
        assert!(a.prompt.input_buffer.is_empty());
        assert_eq!(a.prompt.history_index, None);
    }

    #[test]
    fn cursor_up_line_returns_false_on_single_line() {
        let mut a = app();
        for c in "hello".chars() {
            a.prompt.input_insert(c);
        }
        assert!(!a.prompt.cursor_up_line());
    }

    #[test]
    fn cursor_down_line_returns_false_on_last_line() {
        let mut a = app();
        for c in "hello".chars() {
            a.prompt.input_insert(c);
        }
        assert!(!a.prompt.cursor_down_line());
    }

    #[test]
    fn cursor_up_line_moves_to_previous_line() {
        let mut a = app();
        a.prompt.input_buffer = "ab\ncd".chars().collect();
        a.prompt.cursor_pos = 4; // on 'cd', col 1
        let moved = a.prompt.cursor_up_line();
        assert!(moved);
        assert_eq!(a.prompt.cursor_pos, 1); // col 1 of "ab"
    }

    // ── picker ────────────────────────────────────────────────────────────────

    #[test]
    fn open_picker_populates_filtered_with_all_entries() {
        let mut a = app();
        a.picker.entries = vec![
            PipelineEntry {
                name: "alpha".to_string(),
                path: PathBuf::from("alpha.yaml"),
            },
            PipelineEntry {
                name: "beta".to_string(),
                path: PathBuf::from("beta.yaml"),
            },
        ];
        a.picker.open_picker();
        assert!(a.picker.open);
        assert_eq!(a.picker.filtered.len(), 2);
    }

    #[test]
    fn picker_type_char_filters_entries() {
        let mut a = app();
        a.picker.entries = vec![
            PipelineEntry {
                name: "alpha".to_string(),
                path: PathBuf::from("alpha.yaml"),
            },
            PipelineEntry {
                name: "beta".to_string(),
                path: PathBuf::from("beta.yaml"),
            },
        ];
        a.picker.open_picker();
        a.picker.type_char('b');
        assert_eq!(a.picker.filtered.len(), 1);
        assert_eq!(a.picker.entries[a.picker.filtered[0]].name, "beta");
    }

    #[test]
    fn picker_backspace_on_empty_filter_closes_picker() {
        let mut a = app();
        a.picker.open = true;
        a.picker.backspace();
        assert!(!a.picker.open);
    }

    #[test]
    fn picker_select_sets_pending_switch_and_closes() {
        let mut a = app();
        a.picker.entries = vec![PipelineEntry {
            name: "alpha".to_string(),
            path: PathBuf::from("alpha.yaml"),
        }];
        a.picker.open_picker();
        let result = a.picker.select();
        assert_eq!(result, Some(PathBuf::from("alpha.yaml")));
        assert!(!a.picker.open);
        assert_eq!(
            a.picker.pending_pipeline_switch,
            Some(PathBuf::from("alpha.yaml"))
        );
    }

    // ── session navigation ────────────────────────────────────────────────────

    #[test]
    fn session_prev_on_empty_is_no_op() {
        let mut a = app();
        a.viewport.session_prev();
        assert_eq!(a.viewport.viewing_step, None);
    }

    #[test]
    fn session_prev_from_live_goes_to_last() {
        let mut a = app();
        a.viewport.step_order = vec!["s1".to_string(), "s2".to_string()];
        a.viewport.session_prev();
        assert_eq!(a.viewport.viewing_step, Some(1));
    }

    #[test]
    fn session_next_from_last_returns_to_live() {
        let mut a = app();
        a.viewport.step_order = vec!["s1".to_string(), "s2".to_string()];
        a.viewport.viewing_step = Some(1);
        a.viewport.session_next();
        assert_eq!(a.viewport.viewing_step, None);
    }

    #[test]
    fn session_next_advances_index() {
        let mut a = app();
        a.viewport.step_order = vec!["s1".to_string(), "s2".to_string()];
        a.viewport.viewing_step = Some(0);
        a.viewport.session_next();
        assert_eq!(a.viewport.viewing_step, Some(1));
    }

    // ── side effects: permission ──────────────────────────────────────────────

    #[test]
    fn handle_permission_request_auto_allows_allowlisted_tool() {
        let mut a = app();
        a.permissions.session_allowlist.insert("Bash".to_string());
        let effects = a.handle_permission_request(PermissionRequest {
            display_name: "Bash".to_string(),
            display_detail: String::new(),
        });
        assert_eq!(
            effects,
            vec![SideEffect::SendPermissionResponse(
                PermissionResponse::Allow
            )]
        );
        assert!(!a.permissions.modal_open);
    }

    #[test]
    fn handle_permission_request_unknown_tool_opens_modal() {
        let mut a = app();
        let effects = a.handle_permission_request(PermissionRequest {
            display_name: "Write".to_string(),
            display_detail: " path=/foo".to_string(),
        });
        assert!(effects.is_empty());
        assert!(a.permissions.modal_open);
        assert!(a.permissions.request.is_some());
    }

    #[test]
    fn perm_approve_once_returns_allow_effect() {
        let mut a = app();
        a.permissions.request = Some(PermissionRequest {
            display_name: "Bash".to_string(),
            display_detail: String::new(),
        });
        a.permissions.modal_open = true;
        let effects = a.perm_approve_once();
        assert_eq!(
            effects,
            vec![SideEffect::SendPermissionResponse(
                PermissionResponse::Allow
            )]
        );
        assert!(!a.permissions.modal_open);
        assert!(a.permissions.request.is_none());
    }

    #[test]
    fn perm_approve_session_adds_to_allowlist() {
        let mut a = app();
        a.permissions.request = Some(PermissionRequest {
            display_name: "Bash".to_string(),
            display_detail: String::new(),
        });
        a.permissions.modal_open = true;
        let effects = a.perm_approve_session();
        assert_eq!(
            effects,
            vec![SideEffect::SendPermissionResponse(
                PermissionResponse::Allow
            )]
        );
        assert!(a.permissions.session_allowlist.contains("Bash"));
    }

    #[test]
    fn perm_deny_returns_deny_effect() {
        let mut a = app();
        a.permissions.request = Some(PermissionRequest {
            display_name: "Bash".to_string(),
            display_detail: String::new(),
        });
        a.permissions.modal_open = true;
        let effects = a.perm_deny();
        assert!(matches!(
            effects.as_slice(),
            [SideEffect::SendPermissionResponse(
                PermissionResponse::Deny(_)
            )]
        ));
        assert!(!a.permissions.modal_open);
    }

    // ── side effects: interrupt ───────────────────────────────────────────────

    #[test]
    fn request_resume_returns_set_pause_false_and_sets_phase() {
        let mut a = app();
        a.phase = ExecutionPhase::Paused;
        a.interrupt.modal_open = true;
        let effects = a.request_resume();
        assert_eq!(effects, vec![SideEffect::SetPauseFlag(false)]);
        assert_eq!(a.phase, ExecutionPhase::Running);
        assert!(!a.interrupt.modal_open);
    }

    #[test]
    fn request_kill_returns_both_effects() {
        let mut a = app();
        let effects = a.request_kill();
        assert_eq!(
            effects,
            vec![SideEffect::SetKillFlag, SideEffect::SetPauseFlag(false)]
        );
        assert!(!a.interrupt.modal_open);
        assert_eq!(a.phase, ExecutionPhase::Running);
    }

    // ── loop logic ────────────────────────────────────────────────────────────

    #[test]
    fn resolve_pending_prompt_none_returns_none() {
        let mut a = app();
        assert!(matches!(a.resolve_pending_prompt(), PromptAction::None));
    }

    #[test]
    fn resolve_pending_prompt_hitl_gate_returns_send_hitl() {
        let mut a = app();
        a.phase = ExecutionPhase::HitlGate;
        a.prompt.pending_prompt = Some("continue".to_string());
        let action = a.resolve_pending_prompt();
        assert!(matches!(action, PromptAction::SendHitl(_)));
        assert_eq!(a.phase, ExecutionPhase::Running);
        assert!(a.prompt.pending_prompt.is_none());
    }

    #[test]
    fn resolve_pending_prompt_normal_returns_submit_to_backend() {
        let mut a = app();
        a.prompt.pending_prompt = Some("hello".to_string());
        let action = a.resolve_pending_prompt();
        assert!(matches!(
            action,
            PromptAction::SubmitToBackend { prompt, .. } if prompt == "hello"
        ));
        assert_eq!(a.phase, ExecutionPhase::Running);
    }

    #[test]
    fn apply_pipeline_switch_rebuilds_steps_and_resets_sidebar() {
        let mut a = app();
        a.sidebar.cursor = 3;
        a.sidebar.disabled_steps.insert("s1".to_string());
        let pipeline = Pipeline::passthrough();
        a.apply_pipeline_switch(pipeline, "test-pipe".to_string());
        assert_eq!(a.sidebar.cursor, 0);
        assert!(a.sidebar.disabled_steps.is_empty());
        assert!(a.viewport.lines.iter().any(|l| l.contains("test-pipe")));
    }

    #[test]
    fn apply_pipeline_switch_error_appends_to_viewport() {
        let mut a = app();
        a.apply_pipeline_switch_error("file not found");
        assert!(a
            .viewport
            .lines
            .iter()
            .any(|l| l.contains("file not found")));
    }
}
