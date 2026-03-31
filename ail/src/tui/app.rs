use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Application state for the TUI.
pub struct AppState {
    pub running: bool,
    #[allow(dead_code)]
    pub pipeline: Option<Pipeline>,
    pub steps: Vec<StepDisplay>,
    pub phase: ExecutionPhase,

    // Sidebar display (display-only, no interactive focus)
    pub sidebar_cursor: usize,
    pub disabled_steps: HashSet<String>,

    // Pipeline picker (i-1)
    pub picker_open: bool,
    /// Full list from `discover_all()`, populated at TUI startup.
    pub picker_entries: Vec<PipelineEntry>,
    /// Characters typed after `:` for prefix filtering.
    pub picker_filter: String,
    /// Indices into `picker_entries` that match the current filter.
    pub picker_filtered: Vec<usize>,
    /// Index into `picker_filtered` for the highlighted row.
    pub picker_cursor: usize,
    /// Set by `picker_select()`; consumed by the main loop to hot-reload.
    pub pending_pipeline_switch: Option<PathBuf>,

    // Interrupt system (M11)
    /// Pause flag shared with the executor; TUI sets it to request pause between steps.
    pub pause_flag: Option<Arc<AtomicBool>>,
    /// Kill flag shared with the executor; TUI sets it to abort after current step.
    pub kill_flag: Option<Arc<AtomicBool>>,
    /// Whether the 3-option interrupt modal is currently showing.
    pub interrupt_modal_open: bool,
    /// Path B: guidance text typed during a pause, to be echoed in the viewport.
    pub pending_injection: Option<String>,

    // Prompt input state (M3)
    pub input_buffer: Vec<char>,
    pub cursor_pos: usize,
    /// Sticky column for Up/Down navigation; cleared by any horizontal or edit action.
    pub prompt_preferred_col: Option<usize>,
    pub prompt_history: Vec<String>,
    pub history_index: Option<usize>,
    /// Set when the user presses Enter; cleared by the main loop once consumed.
    pub pending_prompt: Option<String>,

    // Viewport state (M5)
    /// Accumulated display lines. Each entry may be a separator, prompt echo, or output line.
    pub viewport_lines: Vec<String>,
    /// Lines scrolled up from the bottom (0 = auto-scroll to latest output).
    pub viewport_scroll: u16,

    // Streaming tracking (M5)
    pub active_step_id: Option<String>,
    /// True once at least one StreamDelta has arrived for the current step.
    pub step_streamed: bool,

    // Per-step output buffers and session navigation (M9)
    /// Output lines keyed by step_id. Populated as steps run.
    pub step_outputs: HashMap<String, Vec<String>>,
    /// Ordered list of step IDs as they were started, for Ctrl+P/N navigation.
    pub step_order: Vec<String>,
    /// None = live view (auto-scroll to latest). Some(i) = viewing step_order[i].
    pub viewing_step: Option<usize>,

    // Run statistics (M6)
    pub run_start: Option<std::time::Instant>,
    pub run_end: Option<std::time::Instant>,
    pub cumulative_cost_usd: f64,
    pub cumulative_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    pub current_step_index: usize,
    pub total_steps: usize,
    pub last_session_id: Option<String>,

    // Tool permission HITL (SPEC §13.3)
    /// Channel to send permission decisions back to the listener thread.
    pub perm_tx: Option<mpsc::Sender<PermissionResponse>>,
    /// The pending permission request being shown in the modal.
    pub perm_request: Option<PermissionRequest>,
    /// Whether the permission modal is visible.
    pub perm_modal_open: bool,
    /// Cursor position in the permission modal (0=approve once, 1=allow session, 2=deny).
    pub perm_cursor: usize,
    /// Tools approved for the rest of this session (auto-approved without prompting).
    pub perm_session_allowlist: HashSet<String>,
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
            sidebar_cursor: 0,
            disabled_steps: HashSet::new(),
            picker_open: false,
            picker_entries: Vec::new(),
            picker_filter: String::new(),
            picker_filtered: Vec::new(),
            picker_cursor: 0,
            pending_pipeline_switch: None,
            pause_flag: None,
            kill_flag: None,
            interrupt_modal_open: false,
            pending_injection: None,
            input_buffer: Vec::new(),
            cursor_pos: 0,
            prompt_preferred_col: None,
            prompt_history: Vec::new(),
            history_index: None,
            pending_prompt: None,
            viewport_lines: Vec::new(),
            viewport_scroll: 0,

            active_step_id: None,
            step_streamed: false,
            step_outputs: HashMap::new(),
            step_order: Vec::new(),
            viewing_step: None,
            run_start: None,
            run_end: None,
            cumulative_cost_usd: 0.0,
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
            current_step_index: 0,
            total_steps: 0,
            last_session_id: None,
            perm_tx: None,
            perm_request: None,
            perm_modal_open: false,
            perm_cursor: 0,
            perm_session_allowlist: HashSet::new(),
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
                // Register step in per-step output buffers (M9).
                if !self.step_outputs.contains_key(step_id.as_str()) {
                    self.step_order.push(step_id.clone());
                    self.step_outputs.insert(step_id.clone(), Vec::new());
                }
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
                // Do NOT reset viewport_scroll here — preserves user's scroll position.
            }
            ExecutorEvent::RunnerEvent(ail_core::runner::RunnerEvent::Thinking { ref text }) => {
                // Prefix thinking blocks so the viewport can render them in a distinct style.
                for line in text.lines() {
                    self.append_text(&format!("\n[thinking] {line}"));
                }
            }
            ExecutorEvent::RunnerEvent(ail_core::runner::RunnerEvent::Completed(ref result)) => {
                if !self.step_streamed {
                    // Stub runner or no streaming — show full response text now.
                    self.append_text(&result.response);
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
                // Do NOT reset viewport_scroll — preserves user's scroll position.
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

    /// Append `text` (which may contain `\n`) to `viewport_lines` and the active step buffer.
    /// When the user has scrolled up (viewport_scroll > 0), compensates so the viewed
    /// position stays fixed rather than drifting toward the new bottom.
    fn append_text(&mut self, text: &str) {
        let before = self.viewport_lines.len();
        Self::append_to(text, &mut self.viewport_lines);
        let added = self.viewport_lines.len().saturating_sub(before) as u16;
        // Keep absolute scroll position stable when the user has scrolled up.
        if self.viewport_scroll > 0 {
            self.viewport_scroll = self.viewport_scroll.saturating_add(added);
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
        self.step_outputs.clear();
        self.step_order.clear();
        self.viewing_step = None;
        self.interrupt_modal_open = false;
        self.pending_injection = None;
        self.pause_flag = None;
        self.kill_flag = None;
        self.perm_modal_open = false;
        self.perm_request = None;
        // Note: perm_session_allowlist persists across runs within the same TUI session.
        // perm_tx is set fresh each run via BackendEvent::PermReady.
    }

    // ── tool permission HITL (SPEC §13.3) ────────────────────────────────────

    /// Handle an incoming permission request from the MCP bridge.
    ///
    /// If the tool is in the session allowlist, auto-approve without showing the modal.
    pub fn handle_permission_request(&mut self, req: PermissionRequest) {
        if self.perm_session_allowlist.contains(&req.tool_name) {
            self.append_text(&format!(
                "\n  [permission: {} — auto-allowed (session)]",
                req.tool_name
            ));
            if let Some(ref tx) = self.perm_tx {
                let _ = tx.send(PermissionResponse::Allow);
            }
            return;
        }
        // Log the request to scrollback with human-readable field breakdown.
        let mut detail = format!("\n  [permission: {} — waiting for approval]", req.tool_name);
        if let Some(obj) = req.tool_input.as_object() {
            for (k, v) in obj {
                let val_str = match v {
                    serde_json::Value::String(s) => {
                        // For multi-line strings (file content, scripts) show first line + indicator
                        let lines: Vec<&str> = s.lines().collect();
                        if lines.len() > 1 {
                            format!("{} … ({} lines)", lines[0], lines.len())
                        } else if s.len() > 100 {
                            format!("{}…", &s[..100])
                        } else {
                            s.clone()
                        }
                    }
                    other => {
                        let s = other.to_string();
                        if s.len() > 100 {
                            format!("{}…", &s[..100])
                        } else {
                            s
                        }
                    }
                };
                detail.push_str(&format!("\n    {k}: {val_str}"));
            }
        } else if !req.tool_input.is_null() {
            detail.push_str(&format!("\n    {}", req.tool_input));
        }
        self.append_text(&detail);
        self.perm_request = Some(req);
        self.perm_cursor = 0;
        self.perm_modal_open = true;
    }

    /// Move the permission modal cursor up (wraps).
    pub fn perm_nav_up(&mut self) {
        self.perm_cursor = self.perm_cursor.saturating_sub(1);
    }

    /// Move the permission modal cursor down (wraps).
    pub fn perm_nav_down(&mut self) {
        self.perm_cursor = (self.perm_cursor + 1).min(2);
    }

    /// Confirm the currently highlighted permission option.
    pub fn perm_confirm(&mut self) {
        match self.perm_cursor {
            0 => self.perm_approve_once(),
            1 => self.perm_approve_session(),
            _ => self.perm_deny(),
        }
    }

    /// Approve the pending permission request for this tool call only.
    pub fn perm_approve_once(&mut self) {
        let tool = self
            .perm_request
            .as_ref()
            .map(|r| r.tool_name.as_str())
            .unwrap_or("?");
        self.append_text(&format!("\n  [permission: {tool} — approved once]"));
        if let Some(ref tx) = self.perm_tx {
            let _ = tx.send(PermissionResponse::Allow);
        }
        self.perm_modal_open = false;
        self.perm_request = None;
    }

    /// Approve the pending permission request and add the tool to the session allowlist.
    pub fn perm_approve_session(&mut self) {
        if let Some(ref req) = self.perm_request {
            self.perm_session_allowlist.insert(req.tool_name.clone());
        }
        let tool = self
            .perm_request
            .as_ref()
            .map(|r| r.tool_name.as_str())
            .unwrap_or("?");
        self.append_text(&format!("\n  [permission: {tool} — approved for session]"));
        if let Some(ref tx) = self.perm_tx {
            let _ = tx.send(PermissionResponse::Allow);
        }
        self.perm_modal_open = false;
        self.perm_request = None;
    }

    /// Deny the pending permission request.
    pub fn perm_deny(&mut self) {
        let tool = self
            .perm_request
            .as_ref()
            .map(|r| r.tool_name.as_str())
            .unwrap_or("?");
        self.append_text(&format!("\n  [permission: {tool} — denied]"));
        if let Some(ref tx) = self.perm_tx {
            let _ = tx.send(PermissionResponse::Deny("User denied".to_string()));
        }
        self.perm_modal_open = false;
        self.perm_request = None;
    }

    // ── pipeline picker (i-1) ─────────────────────────────────────────────────

    /// Open the picker: populate filtered list and show all entries.
    pub fn open_picker(&mut self) {
        self.picker_open = true;
        self.picker_filter.clear();
        self.picker_cursor = 0;
        self.picker_update_filter();
    }

    /// Close the picker without selecting.
    pub fn close_picker(&mut self) {
        self.picker_open = false;
        self.picker_filter.clear();
        self.picker_filtered.clear();
        self.picker_cursor = 0;
    }

    /// Recompute `picker_filtered` from the current `picker_filter`.
    /// Case-insensitive prefix match on entry name.
    pub fn picker_update_filter(&mut self) {
        let filter = self.picker_filter.to_lowercase();
        self.picker_filtered = self
            .picker_entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name.to_lowercase().starts_with(&filter))
            .map(|(i, _)| i)
            .collect();
        self.picker_cursor = 0;
    }

    pub fn picker_type_char(&mut self, c: char) {
        self.picker_filter.push(c);
        self.picker_update_filter();
    }

    pub fn picker_backspace(&mut self) {
        if self.picker_filter.is_empty() {
            self.close_picker();
        } else {
            self.picker_filter.pop();
            self.picker_update_filter();
        }
    }

    pub fn picker_nav_up(&mut self) {
        if self.picker_cursor > 0 {
            self.picker_cursor -= 1;
        }
    }

    pub fn picker_nav_down(&mut self) {
        if !self.picker_filtered.is_empty() && self.picker_cursor + 1 < self.picker_filtered.len() {
            self.picker_cursor += 1;
        }
    }

    /// Return the path of the currently selected entry and close the picker.
    /// Returns `None` if the filtered list is empty.
    pub fn picker_select(&mut self) -> Option<PathBuf> {
        let entry_idx = *self.picker_filtered.get(self.picker_cursor)?;
        let path = self.picker_entries[entry_idx].path.clone();
        self.close_picker();
        self.pending_pipeline_switch = Some(path.clone());
        Some(path)
    }

    // ── interrupt system (M11) ────────────────────────────────────────────────

    /// Path A: resume — clear pause flag, dismiss modal.
    pub fn request_resume(&mut self) {
        if let Some(ref flag) = self.pause_flag {
            flag.store(false, Ordering::SeqCst);
        }
        self.phase = ExecutionPhase::Running;
        self.interrupt_modal_open = false;
    }

    /// Path B: inject guidance — echo marker in viewport, resume.
    pub fn request_inject_guidance(&mut self, text: String) {
        self.viewport_lines.push(String::new());
        self.viewport_lines
            .push(format!("── ✎ guidance: {} ──", text.trim()));
        self.pending_injection = Some(text);
        self.request_resume();
    }

    /// Path C: kill step — set kill flag, clear pause flag.
    pub fn request_kill(&mut self) {
        if let Some(ref flag) = self.kill_flag {
            flag.store(true, Ordering::SeqCst);
        }
        // Clear pause so executor can proceed to check kill flag.
        if let Some(ref flag) = self.pause_flag {
            flag.store(false, Ordering::SeqCst);
        }
        self.interrupt_modal_open = false;
        self.phase = ExecutionPhase::Running;
    }

    // ── session navigation (M9) ───────────────────────────────────────────────

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
        self.viewport_scroll = 0;
    }

    /// Navigate to the next step (or back to live view) with Ctrl+N.
    pub fn session_next(&mut self) {
        match self.viewing_step {
            None => {}
            Some(i) if i + 1 < self.step_order.len() => {
                self.viewing_step = Some(i + 1);
                self.viewport_scroll = 0;
            }
            Some(_) => {
                self.viewing_step = None;
                self.viewport_scroll = 0;
            }
        }
    }

    // ── viewport scrolling ────────────────────────────────────────────────────

    // ── prompt input ──────────────────────────────────────────────────────────

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
        // Find start of current logical line.
        let line_start = self.input_buffer[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        if line_start == 0 {
            return false; // already on first line
        }
        let col = pos - line_start;
        // Latch preferred column on first vertical move; reuse on subsequent moves.
        let preferred = *self.prompt_preferred_col.get_or_insert(col);
        // Find start of previous logical line.
        let prev_line_end = line_start - 1; // the '\n' itself
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
        // Find start of current logical line.
        let line_start = self.input_buffer[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let col = pos - line_start;
        // Latch preferred column on first vertical move; reuse on subsequent moves.
        let preferred = *self.prompt_preferred_col.get_or_insert(col);
        // Find the next '\n' (end of current line).
        let buf_len = self.input_buffer.len();
        let next_newline = self.input_buffer[pos..]
            .iter()
            .position(|&c| c == '\n')
            .map(|i| pos + i);
        match next_newline {
            None => false, // already on last line
            Some(nl) => {
                let next_line_start = nl + 1;
                // Find end of next line.
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
