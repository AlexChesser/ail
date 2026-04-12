use crate::config::domain::{
    ActionKind, ContextSource, ExitCodeMatch, Pipeline, ResultAction, ResultMatcher, StepBody,
};

/// Escape a string for use inside a YAML double-quoted scalar.
fn yaml_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

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

        if let Some(ref sp) = step.system_prompt {
            out.push_str(&format!("    system_prompt: \"{}\"\n", yaml_quote(sp)));
        }
        if step.resume {
            out.push_str("    resume: true\n");
        }

        match &step.body {
            StepBody::Prompt(text) => {
                // Inline prompts use double-quote scalar; escape backslashes and quotes.
                out.push_str(&format!("    prompt: \"{}\"\n", yaml_quote(text)));
            }
            StepBody::Skill(path) => {
                out.push_str(&format!("    skill: {}\n", path.display()));
            }
            StepBody::SubPipeline { path, prompt } => {
                out.push_str(&format!("    pipeline: {path}\n"));
                if let Some(p) = prompt {
                    out.push_str(&format!("    prompt: \"{}\"\n", yaml_quote(p)));
                }
            }
            StepBody::Action(ActionKind::PauseForHuman) => {
                out.push_str("    action: pause_for_human\n");
            }
            StepBody::Action(ActionKind::ModifyOutput {
                ref headless_behavior,
                ref default_value,
            }) => {
                out.push_str("    action: modify_output\n");
                match headless_behavior {
                    crate::config::domain::HitlHeadlessBehavior::Skip => {}
                    crate::config::domain::HitlHeadlessBehavior::Abort => {
                        out.push_str("    on_headless: abort\n");
                    }
                    crate::config::domain::HitlHeadlessBehavior::UseDefault => {
                        out.push_str("    on_headless: use_default\n");
                        if let Some(dv) = default_value {
                            out.push_str(&format!("    default_value: \"{}\"\n", yaml_quote(dv)));
                        }
                    }
                }
            }
            StepBody::Context(ContextSource::Shell(cmd)) => {
                out.push_str(&format!(
                    "    context:\n      shell: \"{}\"\n",
                    yaml_quote(cmd)
                ));
            }
        }

        if let Some(branches) = &step.on_result {
            out.push_str("    on_result:\n");
            for branch in branches {
                let matcher = match &branch.matcher {
                    ResultMatcher::Contains(text) => {
                        format!("contains: \"{}\"", yaml_quote(text))
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
                    ResultAction::Pipeline { path, prompt } => {
                        let action_str = format!("pipeline: {path}");
                        if let Some(p) = prompt {
                            out.push_str(&format!(
                                "      - {matcher}\n        action: {action_str}\n        prompt: \"{}\"\n",
                                yaml_quote(p)
                            ));
                            continue;
                        }
                        action_str
                    }
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
    use crate::config::domain::{
        ProviderConfig, ResultBranch, ResultMatcher, Step, StepId, ToolPolicy,
    };
    use std::path::PathBuf;

    fn make_step(id: &str, body: StepBody) -> Step {
        Step {
            id: StepId(id.to_string()),
            body,
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    fn make_pipeline(prompt: &str) -> Pipeline {
        Pipeline {
            steps: vec![make_step("test_step", StepBody::Prompt(prompt.to_string()))],
            source: Some(PathBuf::from("test.ail.yaml")),
            defaults: Default::default(),
            timeout_seconds: None,
            default_tools: None,
        }
    }

    // -------------------------------------------------------------------------
    // Existing tests (kept intact)
    // -------------------------------------------------------------------------

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

    // -------------------------------------------------------------------------
    // New tests
    // -------------------------------------------------------------------------

    /// Passthrough pipeline (source = None) materialises without panicking and
    /// produces valid YAML with a `<passthrough>` annotation.
    #[test]
    fn passthrough_source_uses_passthrough_label() {
        let pipeline = Pipeline::passthrough();
        let output = materialize(&pipeline);
        assert!(
            output.contains("<passthrough>"),
            "expected <passthrough> label in origin comment, got: {output}"
        );
    }

    /// The step id must appear in the serialised output.
    #[test]
    fn single_step_id_appears_in_output() {
        let pipeline = make_pipeline("do something");
        let output = materialize(&pipeline);
        assert!(
            output.contains("test_step"),
            "step id not found in output: {output}"
        );
    }

    /// All step ids must appear when multiple steps are present.
    #[test]
    fn multiple_step_ids_all_appear_in_output() {
        let pipeline = Pipeline {
            steps: vec![
                make_step("alpha", StepBody::Prompt("first".to_string())),
                make_step("beta", StepBody::Prompt("second".to_string())),
                make_step("gamma", StepBody::Prompt("third".to_string())),
            ],
            source: Some(PathBuf::from("multi.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(output.contains("alpha"), "alpha not in output");
        assert!(output.contains("beta"), "beta not in output");
        assert!(output.contains("gamma"), "gamma not in output");
    }

    /// Origin comments are numbered 1-based per step position.
    #[test]
    fn multiple_steps_have_sequential_origin_numbers() {
        let pipeline = Pipeline {
            steps: vec![
                make_step("s1", StepBody::Prompt("p1".to_string())),
                make_step("s2", StepBody::Prompt("p2".to_string())),
            ],
            source: Some(PathBuf::from("numbered.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(output.contains("# origin: [1]"), "missing origin [1]");
        assert!(output.contains("# origin: [2]"), "missing origin [2]");
    }

    /// The prompt text must appear verbatim in the output.
    #[test]
    fn prompt_text_appears_in_output() {
        let prompt = "Review everything carefully before committing";
        let pipeline = make_pipeline(prompt);
        let output = materialize(&pipeline);
        assert!(
            output.contains(prompt),
            "prompt text not found in output: {output}"
        );
    }

    /// Prompt text with special characters (backslash, double-quote) must be
    /// escaped so the output remains valid YAML.
    #[test]
    fn prompt_with_special_chars_produces_valid_yaml() {
        let pipeline = make_pipeline(r#"Use "quotes" and back\slashes"#);
        let output = materialize(&pipeline);
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
        assert!(result.is_ok(), "Output was not valid YAML: {output}");
    }

    /// A context:shell step must appear with the `context:` / `shell:` keys.
    #[test]
    fn context_shell_step_appears_in_output() {
        let pipeline = Pipeline {
            steps: vec![make_step(
                "lint",
                StepBody::Context(ContextSource::Shell(
                    "cargo clippy -- -D warnings".to_string(),
                )),
            )],
            source: Some(PathBuf::from("ctx.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(output.contains("context:"), "context: key missing");
        assert!(output.contains("shell:"), "shell: key missing");
        assert!(
            output.contains("cargo clippy"),
            "shell command not in output"
        );
    }

    /// An empty steps list must produce syntactically valid YAML.
    #[test]
    fn empty_steps_list_produces_valid_yaml() {
        let pipeline = Pipeline {
            steps: vec![],
            source: Some(PathBuf::from("empty.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
        assert!(result.is_ok(), "Output was not valid YAML: {output}");
    }

    /// A step with `resume: true` must include `resume: true` in the output.
    #[test]
    fn resume_true_step_appears_in_output() {
        let mut step = make_step("resume_step", StepBody::Prompt("hello".to_string()));
        step.resume = true;
        let pipeline = Pipeline {
            steps: vec![step],
            source: Some(PathBuf::from("resume.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(
            output.contains("resume: true"),
            "resume: true not in output: {output}"
        );
    }

    /// A step with `system_prompt` set must include it in the output.
    #[test]
    fn system_prompt_appears_in_output() {
        let mut step = make_step("sp_step", StepBody::Prompt("hello".to_string()));
        step.system_prompt = Some("You are a helpful assistant.".to_string());
        let pipeline = Pipeline {
            steps: vec![step],
            source: Some(PathBuf::from("sp.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(
            output.contains("system_prompt:"),
            "system_prompt key missing"
        );
        assert!(
            output.contains("You are a helpful assistant."),
            "system_prompt value missing"
        );
    }

    /// An `action: pause_for_human` step must appear in the output.
    #[test]
    fn action_pause_for_human_appears_in_output() {
        let pipeline = Pipeline {
            steps: vec![make_step(
                "gate",
                StepBody::Action(ActionKind::PauseForHuman),
            )],
            source: Some(PathBuf::from("action.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(
            output.contains("pause_for_human"),
            "pause_for_human not in output: {output}"
        );
    }

    /// A step with `on_result` branches must produce an `on_result:` section.
    #[test]
    fn on_result_section_appears_in_output() {
        let mut step = make_step("checked", StepBody::Prompt("do work".to_string()));
        step.on_result = Some(vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: crate::config::domain::ResultAction::Continue,
        }]);
        let pipeline = Pipeline {
            steps: vec![step],
            source: Some(PathBuf::from("on_result.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(output.contains("on_result:"), "on_result: key missing");
        assert!(output.contains("always: true"), "always matcher missing");
        assert!(output.contains("continue"), "continue action missing");
    }

    /// A sub-pipeline step must appear with the `pipeline:` key.
    #[test]
    fn sub_pipeline_step_appears_in_output() {
        let pipeline = Pipeline {
            steps: vec![make_step(
                "child",
                StepBody::SubPipeline {
                    path: "child.ail.yaml".to_string(),
                    prompt: None,
                },
            )],
            source: Some(PathBuf::from("parent.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(
            output.contains("pipeline: child.ail.yaml"),
            "pipeline key missing"
        );
    }

    /// A `skill:` step must appear with the `skill:` key.
    #[test]
    fn skill_step_appears_in_output() {
        let pipeline = Pipeline {
            steps: vec![make_step(
                "sk",
                StepBody::Skill(PathBuf::from("skills/my-skill")),
            )],
            source: Some(PathBuf::from("skill.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        let output = materialize(&pipeline);
        assert!(output.contains("skill:"), "skill: key missing");
        assert!(output.contains("my-skill"), "skill path missing");
    }

    /// Output must always start with the version header line.
    #[test]
    fn output_starts_with_version_header() {
        let pipeline = make_pipeline("anything");
        let output = materialize(&pipeline);
        assert!(
            output.starts_with("version:"),
            "output does not start with version header: {output}"
        );
    }

    /// Default tools field on the pipeline does not affect step-level output
    /// (materialize only serialises steps, not defaults).
    #[test]
    fn pipeline_with_default_tools_produces_valid_yaml() {
        let pipeline = Pipeline {
            steps: vec![make_step("s", StepBody::Prompt("x".to_string()))],
            source: Some(PathBuf::from("dt.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: Some(ToolPolicy {
                disabled: false,
                allow: vec!["Bash".to_string()],
                deny: vec![],
            }),
        };
        let output = materialize(&pipeline);
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
        assert!(result.is_ok(), "Output was not valid YAML: {output}");
    }
}
