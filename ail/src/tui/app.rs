use std::collections::HashSet;

use ail_core::config::domain::Pipeline;
use ail_core::executor::ExecutorEvent;

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

/// Which panel has keyboard focus (M7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Prompt,
    Sidebar,
}

/// Whether a step-detail HUD overlay is open (M8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Normal,
    /// HUD showing config for step at this sidebar index.
    StepHud(usize),
}

/// Application state for the TUI.
pub struct AppState {
    pub running: bool,
    #[allow(dead_code)]
    pub pipeline: Option<Pipeline>,
    pub steps: Vec<StepDisplay>,
    pub phase: ExecutionPhase,

    // Focus and sidebar navigation (M7)
    pub focus: Focus,
    pub sidebar_cursor: usize,
    pub disabled_steps: HashSet<String>,

    // Step detail HUD (M8)
    pub view_mode: ViewMode,
    pub hud_scroll: u16,

    // Prompt input state (M3)
    pub input_buffer: Vec<char>,
    pub cursor_pos: usize,
    pub prompt_history: Vec<String>,
    pub history_index: Option<usize>,
    /// Set when the user presses Enter; cleared by the main loop once consumed.
    pub pending_prompt: Option<String>,

    // Viewport state (M5)
    /// Accumulated display lines. Each entry may be a separator, prompt echo, or output line.
    pub viewport_lines: Vec<String>,
    /// Lines scrolled up from the bottom (0 = auto-scroll to latest output).
    pub viewport_scroll: u16,
    /// Updated each frame so scroll methods can compute page size.
    pub viewport_height: u16,

    // Streaming tracking (M5)
    pub active_step_id: Option<String>,
    /// True once at least one StreamDelta has arrived for the current step.
    pub step_streamed: bool,

    // Run statistics (M6)
    pub run_start: Option<std::time::Instant>,
    pub run_end: Option<std::time::Instant>,
    pub cumulative_cost_usd: f64,
    pub cumulative_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    pub current_step_index: usize,
    pub total_steps: usize,
    pub last_session_id: Option<String>,
}

impl AppState {
    pub fn new(pipeline: Option<Pipeline>) -> Self {
        let steps = pipeline
            .as_ref()
            .map(|p| {
                p.steps
                    .iter()
                    .map(|s| StepDisplay {
                        id: s.id.as_str().to_string(),
                        glyph: StepGlyph::NotReached,
                    })
                    .collect()
            })
            .unwrap_or_default();

        AppState {
            running: true,
            pipeline,
            steps,
            phase: ExecutionPhase::Idle,
            focus: Focus::Prompt,
            sidebar_cursor: 0,
            disabled_steps: HashSet::new(),
            view_mode: ViewMode::Normal,
            hud_scroll: 0,
            input_buffer: Vec::new(),
            cursor_pos: 0,
            prompt_history: Vec::new(),
            history_index: None,
            pending_prompt: None,
            viewport_lines: Vec::new(),
            viewport_scroll: 0,
            viewport_height: 24,
            active_step_id: None,
            step_streamed: false,
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

    // ── executor event handling ───────────────────────────────────────────────

    /// Apply an `ExecutorEvent` from the backend thread to the UI state.
    pub fn apply_executor_event(&mut self, ev: ExecutorEvent) {
        match ev {
            ExecutorEvent::StepStarted {
                ref step_id,
                step_index,
                total_steps,
            } => {
                self.phase = ExecutionPhase::Running;
                self.active_step_id = Some(step_id.clone());
                self.step_streamed = false;
                self.current_step_index = step_index;
                self.total_steps = total_steps;
                if self.run_start.is_none() {
                    self.run_start = Some(std::time::Instant::now());
                }
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Running;
                    }
                }
                // Step separator in the viewport.
                if !self.viewport_lines.is_empty() {
                    self.viewport_lines.push(String::new());
                }
                self.viewport_lines.push(format!("── {} ──", step_id));
                self.viewport_scroll = 0;
            }
            ExecutorEvent::StepCompleted {
                ref step_id,
                cost_usd,
            } => {
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Completed;
                    }
                }
                if let Some(c) = cost_usd {
                    self.cumulative_cost_usd += c;
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
                self.append_text(&format!("\n[error: {error}]"));
                self.run_end = Some(std::time::Instant::now());
            }
            ExecutorEvent::StepSkipped { ref step_id } => {
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::Skipped;
                    }
                }
            }
            ExecutorEvent::RunnerEvent(ail_core::runner::RunnerEvent::StreamDelta { ref text }) => {
                self.step_streamed = true;
                self.append_text(text);
                self.viewport_scroll = 0;
            }
            ExecutorEvent::RunnerEvent(ail_core::runner::RunnerEvent::Completed(ref result)) => {
                if !self.step_streamed {
                    // Stub runner or no streaming — show full response text now.
                    self.append_text(&result.response);
                    self.viewport_scroll = 0;
                }
                if let Some(ref sid) = result.session_id {
                    self.last_session_id = Some(sid.clone());
                }
            }
            ExecutorEvent::RunnerEvent(ail_core::runner::RunnerEvent::CostUpdate {
                cost_usd,
                input_tokens,
                output_tokens,
            }) => {
                self.cumulative_cost_usd += cost_usd;
                self.cumulative_input_tokens += input_tokens;
                self.cumulative_output_tokens += output_tokens;
            }
            ExecutorEvent::RunnerEvent(ail_core::runner::RunnerEvent::ToolUse {
                ref tool_name,
            }) => {
                self.viewport_lines.push(format!("  [tool: {}]", tool_name));
                self.viewport_scroll = 0;
            }
            ExecutorEvent::HitlGateReached { ref step_id } => {
                self.phase = ExecutionPhase::HitlGate;
                for s in &mut self.steps {
                    if s.id == *step_id {
                        s.glyph = StepGlyph::HitlPaused;
                    }
                }
                self.viewport_lines.push(String::new());
                self.viewport_lines
                    .push(format!("◉ pause_for_human — step: {step_id}"));
                self.viewport_lines
                    .push("  press Enter to continue, or type feedback first".to_string());
                self.viewport_scroll = 0;
            }
            ExecutorEvent::RunnerEvent(_) => {
                // ToolResult, Error — no viewport update needed in M5.
            }
            ExecutorEvent::PipelineCompleted(_) => {
                self.phase = ExecutionPhase::Completed;
                self.run_end = Some(std::time::Instant::now());
                self.viewport_lines.push(String::new());
                self.viewport_lines.push("── done ──".to_string());
                self.viewport_scroll = 0;
            }
            ExecutorEvent::PipelineError(ref msg) => {
                self.phase = ExecutionPhase::Failed;
                self.run_end = Some(std::time::Instant::now());
                self.append_text(&format!("\n[pipeline error: {msg}]"));
            }
        }
    }

    /// Append `text` (which may contain `\n`) to `viewport_lines`.
    fn append_text(&mut self, text: &str) {
        let mut parts = text.split('\n');
        if let Some(first) = parts.next() {
            if !first.is_empty() {
                if let Some(last) = self.viewport_lines.last_mut() {
                    last.push_str(first);
                } else {
                    self.viewport_lines.push(first.to_string());
                }
            }
        }
        for part in parts {
            self.viewport_lines.push(part.to_string());
        }
    }

    /// Echo the user's prompt in the viewport before the pipeline runs.
    pub fn echo_prompt(&mut self, prompt: &str) {
        if !self.viewport_lines.is_empty() {
            self.viewport_lines.push(String::new());
        }
        self.viewport_lines.push(format!("> {prompt}"));
        self.viewport_scroll = 0;
    }

    /// Reset glyphs and run statistics for a new pipeline run.
    pub fn reset_for_run(&mut self) {
        for step in &mut self.steps {
            if step.glyph != StepGlyph::Disabled {
                step.glyph = StepGlyph::NotReached;
            }
        }
        self.run_start = None;
        self.run_end = None;
        self.cumulative_cost_usd = 0.0;
        self.cumulative_input_tokens = 0;
        self.cumulative_output_tokens = 0;
        self.current_step_index = 0;
        self.total_steps = 0;
        self.active_step_id = None;
        self.step_streamed = false;
    }

    // ── sidebar navigation (M7) ──────────────────────────────────────────────

    pub fn sidebar_nav_up(&mut self) {
        if self.sidebar_cursor > 0 {
            self.sidebar_cursor -= 1;
        }
    }

    pub fn sidebar_nav_down(&mut self) {
        if !self.steps.is_empty() && self.sidebar_cursor + 1 < self.steps.len() {
            self.sidebar_cursor += 1;
        }
    }

    pub fn sidebar_toggle_disabled(&mut self) {
        if let Some(step) = self.steps.get_mut(self.sidebar_cursor) {
            match step.glyph {
                StepGlyph::Disabled => {
                    step.glyph = StepGlyph::NotReached;
                    self.disabled_steps.remove(&step.id);
                }
                StepGlyph::NotReached => {
                    step.glyph = StepGlyph::Disabled;
                    self.disabled_steps.insert(step.id.clone());
                }
                // Cannot disable running/completed/failed/skipped/hitl steps
                _ => {}
            }
        }
    }

    /// Open the step-detail HUD for the currently focused sidebar step.
    pub fn hud_open(&mut self) {
        if !self.steps.is_empty() {
            self.view_mode = ViewMode::StepHud(self.sidebar_cursor);
            self.hud_scroll = 0;
        }
    }

    pub fn hud_close(&mut self) {
        self.view_mode = ViewMode::Normal;
    }

    pub fn hud_scroll_up(&mut self) {
        self.hud_scroll = self.hud_scroll.saturating_add(1);
    }

    pub fn hud_scroll_down(&mut self) {
        self.hud_scroll = self.hud_scroll.saturating_sub(1);
    }

    pub fn sidebar_enter_focus(&mut self) {
        self.focus = Focus::Sidebar;
        // Clamp cursor to valid range.
        if !self.steps.is_empty() && self.sidebar_cursor >= self.steps.len() {
            self.sidebar_cursor = self.steps.len() - 1;
        }
    }

    pub fn sidebar_exit_focus(&mut self) {
        self.focus = Focus::Prompt;
    }

    // ── viewport scrolling ────────────────────────────────────────────────────

    pub fn viewport_page_up(&mut self) {
        let page = self.viewport_height.max(1);
        let max_scroll = (self.viewport_lines.len() as u16).saturating_sub(self.viewport_height);
        self.viewport_scroll = (self.viewport_scroll + page).min(max_scroll);
    }

    pub fn viewport_page_down(&mut self) {
        let page = self.viewport_height.max(1);
        self.viewport_scroll = self.viewport_scroll.saturating_sub(page);
    }

    // ── prompt input ──────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub fn input_str(&self) -> String {
        self.input_buffer.iter().collect()
    }

    pub fn input_insert(&mut self, c: char) {
        self.input_buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.input_buffer.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }

    pub fn input_delete(&mut self) {
        if self.cursor_pos < self.input_buffer.len() {
            self.input_buffer.remove(self.cursor_pos);
        }
    }

    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.input_buffer.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor_pos = self.input_buffer.len();
    }

    pub fn cursor_word_left(&mut self) {
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

    pub fn submit_input(&mut self) {
        let text: String = self.input_buffer.iter().collect();
        if text.trim().is_empty() {
            return;
        }
        self.prompt_history.push(text.clone());
        self.input_buffer.clear();
        self.cursor_pos = 0;
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
                self.input_buffer = entry;
            }
            Some(_) => {
                self.history_index = None;
                self.input_buffer.clear();
                self.cursor_pos = 0;
            }
        }
    }
}
