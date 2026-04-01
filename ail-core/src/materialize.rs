use crate::config::domain::{
    ActionKind, ContextSource, ExitCodeMatch, Pipeline, ResultAction, ResultMatcher, StepBody,
};

/// Serialize a pipeline back to annotated YAML with origin comments per step.
///
/// Output round-trips through `config::load()` (YAML parsers ignore comments).
pub fn materialize(pipeline: &Pipeline) -> String {
    let source_label = pipeline
        .source
        .as_deref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<passthrough>".to_string());

    let mut out = String::from("version: \"0.0.1\"\npipeline:\n");

    for (idx, step) in pipeline.steps.iter().enumerate() {
        out.push_str(&format!("  # origin: [{}] {}\n", idx + 1, source_label));
        out.push_str(&format!("  - id: {}\n", step.id.as_str()));

        match &step.body {
            StepBody::Prompt(text) => {
                // Inline prompts use double-quote scalar; escape backslashes and quotes.
                let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
                out.push_str(&format!("    prompt: \"{escaped}\"\n"));
            }
            StepBody::Skill(path) => {
                out.push_str(&format!("    skill: {}\n", path.display()));
            }
            StepBody::SubPipeline(path) => {
                out.push_str(&format!("    pipeline: {path}\n"));
            }
            StepBody::Action(ActionKind::PauseForHuman) => {
                out.push_str("    action: pause_for_human\n");
            }
            StepBody::Context(ContextSource::Shell(cmd)) => {
                let escaped = cmd.replace('\\', "\\\\").replace('"', "\\\"");
                out.push_str(&format!("    context:\n      shell: \"{escaped}\"\n"));
            }
        }

        if let Some(branches) = &step.on_result {
            out.push_str("    on_result:\n");
            for branch in branches {
                let matcher = match &branch.matcher {
                    ResultMatcher::Contains(text) => {
                        let escaped = text.replace('"', "\\\"");
                        format!("contains: \"{escaped}\"")
                    }
                    ResultMatcher::ExitCode(ExitCodeMatch::Exact(n)) => {
                        format!("exit_code: {n}")
                    }
                    ResultMatcher::ExitCode(ExitCodeMatch::Any) => "exit_code: any".to_string(),
                    ResultMatcher::Always => "always: true".to_string(),
                };
                let action = match &branch.action {
                    ResultAction::Continue => "continue".to_string(),
                    ResultAction::Break => "break".to_string(),
                    ResultAction::AbortPipeline => "abort_pipeline".to_string(),
                    ResultAction::PauseForHuman => "pause_for_human".to_string(),
                    ResultAction::Pipeline(path) => format!("pipeline: {path}"),
                };
                out.push_str(&format!("      - {matcher}\n        action: {action}\n"));
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{Step, StepId};

    fn make_pipeline(prompt: &str) -> Pipeline {
        Pipeline {
            steps: vec![Step {
                id: StepId("test_step".to_string()),
                body: StepBody::Prompt(prompt.to_string()),
                tools: None,
                on_result: None,
                model: None,
                runner: None,
            }],
            source: Some(std::path::PathBuf::from("test.ail.yaml")),
            defaults: Default::default(),
        }
    }

    #[test]
    fn output_contains_origin_comment() {
        let pipeline = make_pipeline("do something");
        let output = materialize(&pipeline);
        assert!(output.contains("# origin: [1] test.ail.yaml"));
    }

    #[test]
    fn output_is_valid_yaml() {
        let pipeline = make_pipeline("do something");
        let output = materialize(&pipeline);
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
        assert!(result.is_ok(), "Output was not valid YAML: {output}");
    }
}
