use uuid::Uuid;

use crate::config::domain::Pipeline;

use super::turn_log::TurnLog;

pub struct Session {
    pub run_id: String,
    pub pipeline: Pipeline,
    pub invocation_prompt: String,
    pub turn_log: TurnLog,
    pub tool_allowlist: Vec<String>,
}

impl Session {
    pub fn new(pipeline: Pipeline, invocation_prompt: String) -> Self {
        let run_id = Uuid::new_v4().to_string();
        let turn_log = TurnLog::new(run_id.clone());
        Session {
            run_id,
            pipeline,
            invocation_prompt,
            turn_log,
            tool_allowlist: Vec::new(),
        }
    }
}
