use ail_core::config::domain::Pipeline;

/// State of a single pipeline step as displayed in the sidebar.
#[derive(Debug, Clone)]
pub struct StepDisplay {
    pub id: String,
    pub glyph: StepGlyph,
}

/// The visual state of a step glyph.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepGlyph {
    NotReached,
    Running,
    Completed,
    Failed,
    Skipped,    // condition-skipped (⊘)
    Disabled,   // user-disabled (⊖)
    HitlPaused, // ◉
}

/// Application state for the TUI.
pub struct AppState {
    pub running: bool,
    #[allow(dead_code)]
    pub pipeline: Option<Pipeline>,
    pub steps: Vec<StepDisplay>,

    // Prompt input state (M3)
    pub input_buffer: Vec<char>,
    pub cursor_pos: usize,
    pub prompt_history: Vec<String>,
    pub history_index: Option<usize>,
    /// Set when the user presses Enter; cleared by the backend once consumed.
    pub pending_prompt: Option<String>,
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
            input_buffer: Vec::new(),
            cursor_pos: 0,
            prompt_history: Vec::new(),
            history_index: None,
            pending_prompt: None,
        }
    }

    /// Reset all step glyphs to NotReached before a new pipeline run.
    #[allow(dead_code)]
    pub fn reset_step_glyphs(&mut self) {
        for step in &mut self.steps {
            if step.glyph != StepGlyph::Disabled {
                step.glyph = StepGlyph::NotReached;
            }
        }
    }

    /// The current input as a String.
    #[allow(dead_code)]
    pub fn input_str(&self) -> String {
        self.input_buffer.iter().collect()
    }

    /// Insert a character at the cursor position.
    pub fn input_insert(&mut self, c: char) {
        self.input_buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    /// Delete the character before the cursor (backspace).
    pub fn input_backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.input_buffer.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }

    /// Delete the character at the cursor (delete key).
    pub fn input_delete(&mut self) {
        if self.cursor_pos < self.input_buffer.len() {
            self.input_buffer.remove(self.cursor_pos);
        }
    }

    /// Move cursor left one character.
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right one character.
    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.input_buffer.len() {
            self.cursor_pos += 1;
        }
    }

    /// Jump cursor to start of line.
    pub fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Jump cursor to end of line.
    pub fn cursor_end(&mut self) {
        self.cursor_pos = self.input_buffer.len();
    }

    /// Jump cursor left one word (Ctrl+Left / Alt+b).
    pub fn cursor_word_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let mut pos = self.cursor_pos - 1;
        // skip whitespace
        while pos > 0 && self.input_buffer[pos] == ' ' {
            pos -= 1;
        }
        // skip non-whitespace
        while pos > 0 && self.input_buffer[pos - 1] != ' ' {
            pos -= 1;
        }
        self.cursor_pos = pos;
    }

    /// Jump cursor right one word (Ctrl+Right / Alt+f).
    pub fn cursor_word_right(&mut self) {
        let len = self.input_buffer.len();
        if self.cursor_pos >= len {
            return;
        }
        let mut pos = self.cursor_pos;
        // skip non-whitespace
        while pos < len && self.input_buffer[pos] != ' ' {
            pos += 1;
        }
        // skip whitespace
        while pos < len && self.input_buffer[pos] == ' ' {
            pos += 1;
        }
        self.cursor_pos = pos;
    }

    /// Submit the current input: push to history and set pending_prompt.
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

    /// Navigate history up (older).
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

    /// Navigate history down (newer); returns to empty buffer at the bottom.
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
