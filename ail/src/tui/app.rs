/// Application state for the TUI.
pub struct AppState {
    pub running: bool,
}

impl AppState {
    pub fn new() -> Self {
        AppState { running: true }
    }
}
