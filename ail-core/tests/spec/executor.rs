#[test]
fn two_step_pipeline_runs_both_steps_in_order() {
    let mut turn_log = Vec::new();
    let mut session = Session::new();
    let mut pipeline = Pipeline::new();

    // Step 1: Initial prompt
    pipeline.insert_step("prompt:".to_string(), PromptStep::new("Hello, world!".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");

    // Step 2: Second prompt (the invocation should be after step 1)
    pipeline.insert_step("prompt:".to_string(), PromptStep::new("World".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");
}

#[test]
fn template_variable_in_prompt_is_resolved_before_runner() {
    let turn_log = Vec::new();
    let mut session = Session::new();
    let mut pipeline = Pipeline::new();

    pipeline.insert_step("prompt:".to_string(), PromptStep::new("World".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");
}

#[test]
fn template_variable_in_prompt_is_resolved_before_runner() {
    let mut turn_log = Vec::new();
    let mut session = Session::new();
    let mut pipeline = Pipeline::new();

    // Step 1: Initial prompt
    pipeline.insert_step("prompt:".to_string(), PromptStep::new("Hello, world!".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");

    // Step 2: Second prompt (the invocation should be after step 1)
    pipeline.insert_step("prompt:".to_string(), PromptStep::new("World".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");
}

#[test]
fn two_step_pipeline_runs_both_steps_in_order() {
    let mut turn_log = Vec::new();
    let mut session = Session::new();
    let mut pipeline = Pipeline::new();

    // Step 1: Initial prompt
    pipeline.insert_step("prompt:".to_string(), PromptStep::new("Hello, world!".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");

    // Step 2: Second prompt (the invocation should be after step 1)
    pipeline.insert_step("prompt:".to_string(), PromptStep::new("World".to_string()));
    turn_log.push(turn_log.join("\n") + "\n\ninvocation_response\n\ninvocation");
}
