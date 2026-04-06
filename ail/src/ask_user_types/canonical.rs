//! Canonical `AskUserQuestion` format.
//!
//! The official Claude documentation format:
//!
//! ```json
//! {
//!   "questions": [
//!     {
//!       "header": "Clarification",
//!       "question": "Which framework?",
//!       "multiSelect": false,
//!       "options": [
//!         { "label": "React", "description": "Component-based UI library" },
//!         { "label": "Vue" }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! Options may omit `description` — it is treated as optional.
//! Options with a `preview` field are NOT handled here; see [`super::claude_preview`].

use serde_json::Value;

use super::{NormalizedOption, NormalizedQuestion};

/// Returns `Some` when `input["questions"]` is a non-empty array.
/// Options must have at least a `label` string; `description` is optional.
/// Returns `None` if the structure does not match (triggers next type in chain).
pub fn try_parse(input: &Value) -> Option<Vec<NormalizedQuestion>> {
    let arr = input.get("questions")?.as_array()?;
    if arr.is_empty() {
        return None;
    }

    let questions: Vec<NormalizedQuestion> = arr.iter().filter_map(parse_question).collect();

    if questions.is_empty() {
        None
    } else {
        Some(questions)
    }
}

fn parse_question(q: &Value) -> Option<NormalizedQuestion> {
    let question = q.get("question")?.as_str()?.to_string();
    let header = q
        .get("header")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let multi_select = match q.get("multiSelect") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.to_lowercase() == "true",
        _ => false,
    };
    let options = parse_options(q.get("options").unwrap_or(&Value::Null));
    Some(NormalizedQuestion {
        header,
        question,
        multi_select,
        options,
    })
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
    fn canonical_with_description_parses() {
        let input = json!({
            "questions": [{
                "header": "Pick one",
                "question": "Which color?",
                "multiSelect": false,
                "options": [
                    { "label": "Red", "description": "Warm" },
                    { "label": "Blue", "description": "Cool" }
                ]
            }]
        });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question, "Which color?");
        assert_eq!(qs[0].options[0].label, "Red");
        assert_eq!(qs[0].options[0].description.as_deref(), Some("Warm"));
        assert_eq!(qs[0].options[1].description.as_deref(), Some("Cool"));
    }

    #[test]
    fn canonical_without_description_parses() {
        let input = json!({
            "questions": [{
                "question": "Pick one",
                "options": [{ "label": "A" }, { "label": "B" }]
            }]
        });
        let qs = try_parse(&input).unwrap();
        assert!(qs[0].options[0].description.is_none());
        assert!(qs[0].options[1].description.is_none());
    }

    #[test]
    fn multi_select_coerced_from_string() {
        let input =
            json!({ "questions": [{ "question": "q", "multiSelect": "true", "options": [] }] });
        let qs = try_parse(&input).unwrap();
        assert!(qs[0].multi_select);
    }

    #[test]
    fn string_option_normalised_to_label() {
        let input = json!({ "questions": [{ "question": "q", "options": ["Alpha", "Beta"] }] });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs[0].options[0].label, "Alpha");
        assert_eq!(qs[0].options[1].label, "Beta");
    }

    #[test]
    fn missing_questions_field_returns_none() {
        let input = json!({ "question": "q", "options": [] });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn empty_questions_array_returns_none() {
        let input = json!({ "questions": [] });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn header_defaults_to_empty_string() {
        let input = json!({ "questions": [{ "question": "q", "options": [] }] });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs[0].header, "");
    }
}
