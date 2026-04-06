//! Stringified `questions` format.
//!
//! Some models JSON-encode the `questions` array into a string rather than embedding
//! it directly:
//!
//! ```json
//! {
//!   "questions": "[{\"question\":\"Which framework?\",\"options\":[{\"label\":\"React\"}]}]"
//! }
//! ```
//!
//! This type parses the string and re-runs the canonical parser on the result.

use serde_json::Value;

use super::{canonical, NormalizedQuestion};

/// Returns `Some` when `input["questions"]` is a JSON-encoded string that, once parsed,
/// yields a non-empty array accepted by [`super::canonical`].
pub fn try_parse(input: &Value) -> Option<Vec<NormalizedQuestion>> {
    let s = input.get("questions")?.as_str()?;
    let parsed: Value = serde_json::from_str(s).ok()?;
    // Re-run canonical on { "questions": <parsed_array> }
    let rewrapped = serde_json::json!({ "questions": parsed });
    canonical::try_parse(&rewrapped)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn stringified_questions_parsed_and_normalised() {
        let questions_str = serde_json::to_string(&json!([{
            "header": "h",
            "question": "What?",
            "multiSelect": false,
            "options": [{ "label": "A" }, { "label": "B" }]
        }]))
        .unwrap();
        let input = json!({ "questions": questions_str });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question, "What?");
        assert_eq!(qs[0].options[0].label, "A");
    }

    #[test]
    fn non_string_questions_returns_none() {
        let input = json!({ "questions": [{ "question": "q", "options": [] }] });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn invalid_json_string_returns_none() {
        let input = json!({ "questions": "{not valid json" });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn empty_array_string_returns_none() {
        let input = json!({ "questions": "[]" });
        assert!(try_parse(&input).is_none());
    }
}
