use ail_core::config::domain::{
    ContextSource, ExitCodeMatch, ResultAction, ResultMatcher, StepBody,
};
use ail_core::config::load;
use ail_core::executor::execute;
use ail_core::runner::stub::StubRunner;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// SPEC §5.5 — context:shell: step parses into ContextSource::Shell
#[test]
fn context_shell_step_parses_correctly() {
    let pipeline = load(&fixtures_dir().join("context_shell.ail.yaml")).unwrap();
    assert_eq!(pipeline.steps.len(), 1);
    let step = &pipeline.steps[0];
    assert_eq!(step.id.as_str(), "lint");
    match &step.body {
        StepBody::Context(ContextSource::Shell(cmd)) => {
            assert_eq!(cmd, "cargo clippy -- -D warnings");
        }
        other => panic!("expected Context::Shell, got {other:?}"),
    }
}

/// SPEC §5.5 — context: with no source (no shell, no mcp) fails validation
#[test]
fn context_step_without_source_fails_validation() {
    let result = load(&fixtures_dir().join("invalid_context_no_source.ail.yaml"));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type,
        ail_core::error::error_types::CONFIG_VALIDATION_FAILED
    );
}

/// SPEC §5.5 — step cannot have both context: and prompt:
#[test]
fn context_and_prompt_on_same_step_fails() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: bad
    prompt: "hello"
    context:
      shell: "echo hi"
"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bad.ail.yaml");
    std::fs::write(&path, yaml).unwrap();
    let result = load(&path);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type,
        ail_core::error::error_types::CONFIG_VALIDATION_FAILED
    );
}

/// SPEC §5.5 — on_result multi-branch array parses correctly (benchmarking pipeline)
#[test]
fn on_result_multi_branch_parses() {
    let pipeline = load(&fixtures_dir().join("on_result_multi_branch.ail.yaml")).unwrap();
    let lint_step = &pipeline.steps[0];
    let branches = lint_step
        .on_result
        .as_ref()
        .expect("lint step has on_result");
    assert_eq!(branches.len(), 2);

    // Branch 0: exit_code: 0 → continue
    match &branches[0].matcher {
        ResultMatcher::ExitCode(ExitCodeMatch::Exact(0)) => {}
        other => panic!("expected ExitCode(Exact(0)), got {other:?}"),
    }
    assert!(matches!(&branches[0].action, ResultAction::Continue));

    // Branch 1: exit_code: any → continue
    match &branches[1].matcher {
        ResultMatcher::ExitCode(ExitCodeMatch::Any) => {}
        other => panic!("expected ExitCode(Any), got {other:?}"),
    }
    assert!(matches!(&branches[1].action, ResultAction::Continue));
}

/// SPEC §5.5 — exit_code: 0 (integer) parses as ExitCode(Exact(0))
#[test]
fn exit_code_integer_parses() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: lint
    context:
      shell: "true"
    on_result:
      - exit_code: 0
        action: continue
"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.ail.yaml");
    std::fs::write(&path, yaml).unwrap();
    let pipeline = load(&path).unwrap();
    let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
    assert!(matches!(
        &branch.matcher,
        ResultMatcher::ExitCode(ExitCodeMatch::Exact(0))
    ));
}

/// SPEC §5.5 — exit_code: any (string) parses as ExitCode(Any)
#[test]
fn exit_code_any_parses() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: lint
    context:
      shell: "true"
    on_result:
      - exit_code: any
        action: continue
"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.ail.yaml");
    std::fs::write(&path, yaml).unwrap();
    let pipeline = load(&path).unwrap();
    let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
    assert!(matches!(
        &branch.matcher,
        ResultMatcher::ExitCode(ExitCodeMatch::Any)
    ));
}

/// SPEC §5.5 — on_result branch with unknown action fails validation
#[test]
fn on_result_unknown_action_fails() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: lint
    context:
      shell: "true"
    on_result:
      - exit_code: 0
        action: destroy_everything
"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.ail.yaml");
    std::fs::write(&path, yaml).unwrap();
    let result = load(&path);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type,
        ail_core::error::error_types::CONFIG_VALIDATION_FAILED
    );
}

/// SPEC §5.5 — context shell step captures stdout
#[test]
fn context_shell_captures_stdout() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("greet".to_string()),
            body: StepBody::Context(ContextSource::Shell("printf 'hello'".to_string())),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("x")).unwrap();

    let entry = &session.turn_log.entries()[0];
    assert_eq!(entry.stdout.as_deref(), Some("hello"));
    assert!(entry.response.is_none());

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — context shell step captures stderr on a separate stream
#[test]
fn context_shell_captures_stderr() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("warn".to_string()),
            body: StepBody::Context(ContextSource::Shell("printf 'error msg' >&2".to_string())),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("x")).unwrap();

    let entry = &session.turn_log.entries()[0];
    let stderr = entry.stderr.as_deref().unwrap_or("");
    assert!(
        stderr.contains("error msg"),
        "Expected stderr to contain 'error msg', got: {stderr:?}"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — context shell step captures exit code 0
#[test]
fn context_shell_captures_exit_code_zero() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("ok".to_string()),
            body: StepBody::Context(ContextSource::Shell("true".to_string())),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("x")).unwrap();

    assert_eq!(session.turn_log.entries()[0].exit_code, Some(0));

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — context shell step captures non-zero exit code (not an error)
#[test]
fn context_shell_captures_nonzero_exit_code() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("fail".to_string()),
            body: StepBody::Context(ContextSource::Shell("exit 42".to_string())),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    let result = execute(&mut session, &StubRunner::new("x"));
    // Non-zero exit is a RESULT, not an error — pipeline continues
    assert!(result.is_ok());
    assert_eq!(session.turn_log.entries()[0].exit_code, Some(42));

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — context step does not call the runner
#[test]
fn context_shell_does_not_call_runner() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("lint".to_string()),
            body: StepBody::Context(ContextSource::Shell("true".to_string())),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    let counting_runner = ail_core::runner::stub::CountingStubRunner::new("x");
    execute(&mut session, &counting_runner).unwrap();
    assert_eq!(
        counting_runner.invocation_count(),
        0,
        "runner must not be called for context steps"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — context step result feeds into prompt template variable
#[test]
fn context_then_prompt_pipeline() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![
            Step {
                id: StepId("lint".to_string()),
                body: StepBody::Context(ContextSource::Shell(
                    "printf 'no issues found'".to_string(),
                )),
                tools: None,
                model: None,
                on_result: None,
            },
            Step {
                id: StepId("review".to_string()),
                body: StepBody::Prompt("Lint output: {{ step.lint.result }}".to_string()),
                tools: None,
                model: None,
                on_result: None,
            },
        ],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("ok")).unwrap();

    let review_entry = &session.turn_log.entries()[1];
    assert_eq!(review_entry.step_id, "review");
    assert!(
        review_entry.prompt.contains("no issues found"),
        "Expected lint output in resolved prompt, got: {}",
        review_entry.prompt
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — step.lint.exit_code template variable resolves to string
#[test]
fn template_step_exit_code_resolves() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![
            Step {
                id: StepId("lint".to_string()),
                body: StepBody::Context(ContextSource::Shell("exit 3".to_string())),
                tools: None,
                model: None,
                on_result: None,
            },
            Step {
                id: StepId("report".to_string()),
                body: StepBody::Prompt("Exit was: {{ step.lint.exit_code }}".to_string()),
                tools: None,
                model: None,
                on_result: None,
            },
        ],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("ok")).unwrap();

    let report_entry = &session.turn_log.entries()[1];
    assert!(
        report_entry.prompt.contains("3"),
        "Expected exit code '3' in resolved prompt, got: {}",
        report_entry.prompt
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 + §5.8 — prompt: can reference a file path
#[test]
fn prompt_file_path_loads_contents() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let prompt_file = tmp.path().join("my_prompt.md");
    std::fs::write(&prompt_file, "Please review the code carefully.").unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("review".to_string()),
            body: StepBody::Prompt(prompt_file.to_str().unwrap().to_string()),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("ok")).unwrap();

    let entry = &session.turn_log.entries()[0];
    assert!(
        entry.prompt.contains("Please review the code carefully"),
        "Expected file contents in resolved prompt, got: {}",
        entry.prompt
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — prompt: file not found returns CONFIG_FILE_NOT_FOUND error
#[test]
fn prompt_file_not_found_returns_error() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("review".to_string()),
            body: StepBody::Prompt("./nonexistent_prompt_file.md".to_string()),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    let result = execute(&mut session, &StubRunner::new("x"));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type,
        ail_core::error::error_types::CONFIG_FILE_NOT_FOUND
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.5 — inline prompt string is unchanged by file path resolution
#[test]
fn prompt_inline_string_unchanged() {
    use ail_core::config::domain::{Pipeline, Step, StepId};

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![Step {
            id: StepId("review".to_string()),
            body: StepBody::Prompt("Please review this code.".to_string()),
            tools: None,
            model: None,
            on_result: None,
        }],
        defaults: Default::default(),
        source: None,
    };
    let mut session = ail_core::session::Session::new(pipeline, "p".to_string());
    execute(&mut session, &StubRunner::new("ok")).unwrap();

    let entry = &session.turn_log.entries()[0];
    assert_eq!(entry.prompt, "Please review this code.");

    std::env::set_current_dir(orig).unwrap();
}
