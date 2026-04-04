#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output() {
        let result = ai_with_input("hello world");
        assert_eq!(result, Some("hello world"));
    }

    #[test]
    fn test_pipelines() {
        let result = ai_with_pipeline(
            &["step"],
            &["prompt"],
            &[
                &[
                    "What is hello world?",
                    "Hello there! What's your name?"
                ],
            ],
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_pipeline() {
        let result = ai_with_pipeline(
            &["step"],
            &["prompt"],
            &[
                &[
                    "Hello world!"
                ],
            ],
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_pipeline_invalid() {
        let result = ai_with_pipeline(
            &["step"],
            &["prompt"],
            &[
                &[
                    "Hello!"
                ],
            ],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_paste_input() {
        let result = ai_paste_input("/tmp/test.txt");
        assert!(result.is_some());
    }

    #[test]
    fn test_output_file() {
        let result = ai_with_output_file("/tmp/output.txt");
        assert!(result.is_some());
        assert!(std::fs::read("/tmp/output.txt").unwrap() == "hello world".to_string());
    }

    #[test]
    fn test_output_text() {
        let result = ai_with_text("What is hello?");
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_error_case() {
        let result = ai_error();
        assert!(result.is_none());
    }
}

fn ai_with_input(text: &str) -> Option<String> {
    let input = &text;
    let result = ai_with_output_file(text);
    assert!(result.is_none());
    Some(format!("{}: {}", result, input).to_string())
}

fn ai_with_pipeline(
    steps: &[Step],
    prompts: &[Prompt],
    responses: &[String],
) -> Option<Vec<String>> {
    let result = ai_with_output_file(&responses);
    assert!(result.is_none());
    if let Some(response) = result {
        let mut result = Vec::new();
        for step in steps {
            result.extend(step.prompt);
        }
        Some(response)
    } else {
        None
    }
}

fn ai_with_output_file(text: &str) -> Option<Vec<String>> {
    let result = ai_with_input(text);
    assert!(result.is_none());
    Some(result)
}

fn ai_with_text(text: &str) -> Option<String> {
    let result = ai_with_output_file(text);
    assert!(result.is_none());
    Some(result)
}

fn ai_error() -> Option<String> {
    let result = ai_with_input("This is an error: I don't know what you asked me to do.");
    assert!(result.is_none());
    Some("error: I don't know what you asked me to do".to_string())
}

fn ai_paste_input(input_path: &str) -> Option<Vec<String>> {
    let result = ai_with_input(std::fs::read_to_string(input_path).unwrap());
    assert!(result.is_none());
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_input() {
        let result = ai_with_input("hello");
        assert_eq!(result, Some("hello".to_string()));
    }
}

#[tokio::test]
async fn test_api_output_file() -> anyhow::Result<()> {
    let result = ai_with_output_file("/tmp/test.txt");
    assert!(result.is_some());
    let _ = std::fs::read("/tmp/test.txt").unwrap();
    Ok(())
}

#[tokio::test]
async fn test_api_output_text() -> anyhow::Result<()> {
    let result = ai_with_text("hello");
    assert_eq!(result.unwrap(), "hello".to_string());
    Ok(())
}

#[tokio::test]
async fn test_api_error() -> anyhow::Result<()> {
    let result = ai_error();
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_api_pipeline() -> anyhow::Result<()> {
    let result = ai_with_pipeline(
        &["step"],
        &["prompt"],
        &[
            &[
                "What is hello?",
                "Hello! How can I help you today?"
            ],
        ],
    );
    assert!(result.is_some());
    Ok(())
}

#[tokio::test]
async fn test_api_pipeline_invalid() -> anyhow::Result<()> {
    let result = ai_with_pipeline(
        &["step"],
        &["prompt"],
        &[
            &[
                "What is hello!"
            ],
        ],
    );
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_api_paste_input() -> anyhow::Result<()> {
    let result = ai_paste_input("/tmp/test.txt");
    assert!(result.is_some());
    Ok(())
}

#[tokio::test]
async fn test_api_output() -> anyhow::Result<()> {
    let result = ai_with_output_file("hello world");
    assert!(result.is_some());
    Ok(())
}
