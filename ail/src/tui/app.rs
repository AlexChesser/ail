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
}
