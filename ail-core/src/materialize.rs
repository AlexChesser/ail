use crate::config::domain::{ActionKind, Pipeline, StepBody};

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
                // Inline prompts use block scalar (|) to handle multi-line safely
                let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
                out.push_str(&format!("    prompt: \"{escaped}\"\n"));
            }
            StepBody::Skill(path) => {
                out.push_str(&format!("    skill: {}\n", path.display()));
            }
            StepBody::SubPipeline(path) => {
                out.push_str(&format!("    pipeline: {}\n", path.display()));
            }
            StepBody::Action(ActionKind::PauseForHuman) => {
                out.push_str("    action: pause_for_human\n");
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
            }],
            source: Some(std::path::PathBuf::from("test.ail.yaml")),
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
