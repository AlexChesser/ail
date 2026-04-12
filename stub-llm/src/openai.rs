//! OpenAI-compatible response format builder.
//!
//! Generates JSON response bodies matching the `/v1/chat/completions` format
//! used by OpenAI, Ollama, and other compatible providers.

use serde_json::{json, Value};

/// Build an OpenAI-compatible chat completion response body.
pub(crate) fn build_success_response(
    content: &str,
    model: Option<&str>,
    usage: Option<(u64, u64)>,
) -> Value {
    let model = model.unwrap_or("stub-model");
    let (prompt_tokens, completion_tokens) = usage.unwrap_or((0, 0));

    json!({
        "id": "chatcmpl-stub",
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_response_has_required_fields() {
        let resp = build_success_response("Hello!", None, None);
        assert_eq!(
            resp["choices"][0]["message"]["content"].as_str().unwrap(),
            "Hello!"
        );
        assert_eq!(resp["model"].as_str().unwrap(), "stub-model");
        assert_eq!(
            resp["choices"][0]["message"]["role"].as_str().unwrap(),
            "assistant"
        );
    }

    #[test]
    fn success_response_uses_custom_model() {
        let resp = build_success_response("Hi", Some("gpt-4o"), None);
        assert_eq!(resp["model"].as_str().unwrap(), "gpt-4o");
    }

    #[test]
    fn success_response_includes_usage() {
        let resp = build_success_response("Hi", None, Some((100, 50)));
        assert_eq!(resp["usage"]["prompt_tokens"].as_u64().unwrap(), 100);
        assert_eq!(resp["usage"]["completion_tokens"].as_u64().unwrap(), 50);
        assert_eq!(resp["usage"]["total_tokens"].as_u64().unwrap(), 150);
    }
}
