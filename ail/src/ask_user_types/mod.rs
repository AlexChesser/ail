//! Type-based parsing for `ail_ask_user` / `AskUserQuestion` tool inputs.
//!
//! Different models (and different versions of the same model) produce the `AskUserQuestion`
//! payload in subtly different shapes. Rather than one monolithic normaliser, each known
//! format lives in its own file and implements a `try_parse` function that returns `Some` when
//! the input matches and `None` otherwise.
//!
//! [`parse`] tries each type in order; the first `Some` wins. All types produce the same
//! canonical `{ questions: [...] }` JSON output so the rest of the pipeline is unchanged.
//!
//! ## Adding a new format
//!
//! 1. Create `ail/src/ask_user_types/<name>.rs`.
//! 2. Implement `pub fn try_parse(input: &serde_json::Value) -> Option<Vec<super::NormalizedQuestion>>`.
//! 3. Add `mod <name>;` below and insert the call into the chain in `parse()`.
//! 4. Add unit tests inside the new file.

mod canonical;
mod claude_preview;
mod flat;
mod stringified;

use serde_json::Value;

// ── Shared output types ──────────────────────────────────────────────────────

/// A single option in a normalised `AskUserQuestion`.
pub struct NormalizedOption {
    pub label: String,
    pub description: Option<String>,
}

/// A single question in a normalised `AskUserQuestion` payload.
pub struct NormalizedQuestion {
    pub header: String,
    pub question: String,
    pub multi_select: bool,
    pub options: Vec<NormalizedOption>,
}

impl NormalizedQuestion {
    /// Serialise to the canonical JSON shape expected by the permission socket and VS Code frontend.
    pub fn to_value(&self) -> Value {
        let options: Vec<Value> = self
            .options
            .iter()
            .map(|o| {
                let mut map = serde_json::Map::new();
                map.insert("label".into(), Value::String(o.label.clone()));
                if let Some(ref d) = o.description {
                    map.insert("description".into(), Value::String(d.clone()));
                }
                Value::Object(map)
            })
            .collect();

        serde_json::json!({
            "header": self.header,
            "question": self.question,
            "multiSelect": self.multi_select,
            "options": options,
        })
    }
}

// ── Parse chain ──────────────────────────────────────────────────────────────

/// Parse a raw `ail_ask_user` / `AskUserQuestion` tool input into the canonical
/// `{ questions: [...] }` JSON shape.
///
/// Tries each known type in order; the first successful parse wins.
/// If no type matches, falls back to an empty questions list (fail-open).
pub fn parse(input: &Value) -> Value {
    let questions = claude_preview::try_parse(input)
        .or_else(|| canonical::try_parse(input))
        .or_else(|| flat::try_parse(input))
        .or_else(|| stringified::try_parse(input))
        .unwrap_or_default();

    let values: Vec<Value> = questions.iter().map(NormalizedQuestion::to_value).collect();
    serde_json::json!({ "questions": values })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn normalized_question_to_value_includes_all_fields() {
        let q = NormalizedQuestion {
            header: "My Header".to_string(),
            question: "Which one?".to_string(),
            multi_select: true,
            options: vec![
                NormalizedOption {
                    label: "Alpha".to_string(),
                    description: Some("First".to_string()),
                },
                NormalizedOption {
                    label: "Beta".to_string(),
                    description: None,
                },
            ],
        };
        let v = q.to_value();
        assert_eq!(v["header"], "My Header");
        assert_eq!(v["question"], "Which one?");
        assert_eq!(v["multiSelect"], true);
        assert_eq!(v["options"][0]["label"], "Alpha");
        assert_eq!(v["options"][0]["description"], "First");
        assert_eq!(v["options"][1]["label"], "Beta");
        // No description key when description is None.
        assert!(v["options"][1].get("description").is_none());
    }

    #[test]
    fn normalized_question_to_value_multi_select_false() {
        let q = NormalizedQuestion {
            header: String::new(),
            question: "Pick one".to_string(),
            multi_select: false,
            options: vec![],
        };
        let v = q.to_value();
        assert_eq!(v["multiSelect"], false);
        assert_eq!(v["options"], json!([]));
    }

    #[test]
    fn parse_returns_empty_questions_on_unrecognised_input() {
        let v = parse(&json!({ "something": "else" }));
        assert!(v["questions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn parse_wraps_questions_in_canonical_envelope() {
        let input = json!({
            "questions": [{ "question": "Pick?", "options": [{ "label": "A" }] }]
        });
        let v = parse(&input);
        assert!(v.get("questions").is_some());
        assert_eq!(v["questions"].as_array().unwrap().len(), 1);
        assert_eq!(v["questions"][0]["question"], "Pick?");
    }
}
