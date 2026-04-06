//! Claude model format — `preview` field on options instead of `description`.
//!
//! Observed in the wild when models produce `AskUserQuestion` inputs with `preview`
//! on each option rather than `description`. Claude CLI's strict schema rejects these
//! because `description` is listed as required; the bridge intercepts and normalises.
//!
//! Sample (from runlog, 2026-04):
//!
//! ```json
//! {
//!   "questions": [
//!     {
//!       "header": "Color preferences",
//!       "question": "What color do you prefer?",
//!       "multiSelect": false,
//!       "options": [
//!         { "label": "Red",   "preview": "Red"   },
//!         { "label": "Blue",  "preview": "Blue"  },
//!         { "label": "Green", "preview": "Green" }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! `preview` is mapped to `description` so downstream code (the VS Code frontend) receives
//! a consistent shape. This type is tried **before** [`super::canonical`] so that `preview`
//! values are preserved rather than silently dropped.

use serde_json::Value;

use super::{NormalizedOption, NormalizedQuestion};

/// Returns `Some` when `input["questions"]` is a non-empty array and at least one option
/// carries a `preview` field without a `description` field.
/// Returns `None` to fall through to the next type in the chain.
pub fn try_parse(input: &Value) -> Option<Vec<NormalizedQuestion>> {
    let arr = input.get("questions")?.as_array()?;
    if arr.is_empty() {
        return None;
    }

    // Only take over when we see the preview-without-description pattern.
    if !arr.iter().any(has_preview_options) {
        return None;
    }

    let questions: Vec<NormalizedQuestion> = arr.iter().filter_map(parse_question).collect();

    if questions.is_empty() {
        None
    } else {
        Some(questions)
    }
}

fn has_preview_options(q: &Value) -> bool {
    q.get("options")
        .and_then(|o| o.as_array())
        .map(|opts| {
            opts.iter()
                .any(|opt| opt.get("preview").is_some() && opt.get("description").is_none())
        })
        .unwrap_or(false)
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
            // Map preview → description when description is absent.
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
    fn preview_mapped_to_description() {
        let input = json!({
            "questions": [{
                "header": "Color preferences",
                "question": "What color do you prefer?",
                "multiSelect": false,
                "options": [
                    { "label": "Red",   "preview": "Red"   },
                    { "label": "Blue",  "preview": "Blue"  },
                    { "label": "Green", "preview": "Green" }
                ]
            }]
        });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs[0].options[0].label, "Red");
        assert_eq!(qs[0].options[0].description.as_deref(), Some("Red"));
        assert_eq!(qs[0].options[1].description.as_deref(), Some("Blue"));
        assert_eq!(qs[0].options[2].description.as_deref(), Some("Green"));
    }

    #[test]
    fn description_takes_precedence_over_preview() {
        let input = json!({
            "questions": [{
                "question": "q",
                "options": [{ "label": "A", "description": "desc", "preview": "prev" }]
            }]
        });
        // description present → this type won't even match (no preview-without-description)
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn no_preview_field_returns_none() {
        let input = json!({
            "questions": [{ "question": "q", "options": [{ "label": "A" }] }]
        });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn empty_questions_returns_none() {
        let input = json!({ "questions": [] });
        assert!(try_parse(&input).is_none());
    }

    #[test]
    fn multiple_questions_all_normalised() {
        let input = json!({
            "questions": [
                {
                    "question": "First?",
                    "options": [{ "label": "X", "preview": "ex" }]
                },
                {
                    "question": "Second?",
                    "options": [{ "label": "Y", "preview": "why" }]
                }
            ]
        });
        let qs = try_parse(&input).unwrap();
        assert_eq!(qs.len(), 2);
        assert_eq!(qs[0].options[0].description.as_deref(), Some("ex"));
        assert_eq!(qs[1].options[0].description.as_deref(), Some("why"));
    }
}
