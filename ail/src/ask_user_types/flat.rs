//! Flat `AskUserQuestion` format — no `questions` array wrapper.
//!
//! Some models omit the outer `questions:[...]` array and place the question
//! fields directly at the top level:
//!
//! ```json
//! {
//!   "question": "Which framework should I use?",
//!   "header": "Framework choice",
//!   "multiSelect": false,
//!   "options": [
//!     { "label": "React", "description": "Component-based" },
//!     { "label": "Vue" }
//!   ]
//! }
//! ```
//!
//! This type wraps the input into a single-element `questions` array and delegates
//! option normalisation to [`super::canonical`].

use serde_json::Value;

use super::{NormalizedOption, NormalizedQuestion};

/// Returns `Some` when `input["question"]` is a non-empty string and `input["questions"]`
/// is absent (avoiding overlap with the canonical / preview types).
pub fn try_parse(input: &Value) -> Option<Vec<NormalizedQuestion>> {
    let question = input.get("question")?.as_str()?.to_string();
    if question.is_empty() {
        return None;
    }
    // Defer to canonical / preview types if a questions array is also present.
    if input.get("questions").is_some() {
        return None;
    }

    let header = input
        .get("header")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let multi_select = match input.get("multiSelect") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.to_lowercase() == "true",
        _ => false,
    };
    let options = parse_options(input.get("options").unwrap_or(&Value::Null));

    Some(vec![NormalizedQuestion {
        header,
        question,
        multi_select,
        options,
    }])
}

fn parse_options(raw: &Value) -> Vec<NormalizedOption> {
    let arr = match raw.as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter().filter_map(parse_option).collect()
}

fn parse_option(opt: &Value) -> Option<NormalizedOption> {
    match opt {
        Value::String(s) => Some(NormalizedOption {
            label: s.clone(),
            description: None,
        }),
        Value::Object(m) => {
            let label = m.get("label")?.as_str()?.to_string();
            let description = m
                .get("description")
                .or_else(|| m.get("preview"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(NormalizedOption { label, description })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn flat_form_wraps_into_single_question() {
        let input = json!({
            "header": "Colours",
            "question": "Pick one",
            "multiSelect": false,
            "options": [{ "label": "Red" }, { "label": "Blue" }]
        });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question, "Pick one");
        assert_eq!(qs[0].header, "Colours");
        assert_eq!(qs[0].options[0].label, "Red");
    }

    #[test]
    fn returns_none_when_questions_array_present() {
        let input = json!({
            "question": "q",
            "questions": [{ "question": "q2", "options": [] }]
        });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn returns_none_when_question_absent() {
        let input = json!({ "options": [{ "label": "A" }] });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn string_options_normalised() {
        let input = json!({ "question": "q", "options": ["Alpha", "Beta"] });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs[0].options[0].label, "Alpha");
        assert_eq!(qs[0].options[1].label, "Beta");
    }

    #[test]
    fn preview_option_mapped_to_description() {
        let input = json!({
            "question": "q",
            "options": [{ "label": "A", "preview": "ay" }]
        });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs[0].options[0].description.as_deref(), Some("ay"));
    }
}
