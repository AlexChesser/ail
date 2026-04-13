//! Skill registry — built-in and user-defined skill resolution (SPEC §6, §14).
//!
//! Skills are reusable, named prompt templates that can be invoked via `skill:` step
//! bodies. Built-in modules live under the `ail/` namespace (e.g. `ail/code_review`).
//! The registry resolves a skill name to a prompt string that is then sent to the runner.

#![allow(clippy::result_large_err)]

use std::collections::HashMap;

use crate::error::AilError;

/// A resolved skill definition ready for execution.
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    /// Human-readable name.
    pub name: String,
    /// Description of what this skill does.
    pub description: String,
    /// The prompt template sent to the runner. May reference `{{ last_response }}`
    /// and other template variables — resolved at execution time by the template engine.
    pub prompt_template: String,
}

/// Registry of available skills. Built-in skills are registered at construction time.
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    /// Create a new registry pre-populated with all built-in `ail/*` modules.
    pub fn new() -> Self {
        let mut skills = HashMap::new();

        // ── ail/code_review ─────────────────────────────────────────────────
        skills.insert(
            "ail/code_review".to_string(),
            SkillDefinition {
                name: "ail/code_review".to_string(),
                description: "Reviews code for correctness, style, and potential issues.".to_string(),
                prompt_template: concat!(
                    "You are a senior code reviewer. Review the following for:\n",
                    "- Correctness: logic errors, edge cases, off-by-one errors\n",
                    "- Style: naming, formatting, idiomatic usage\n",
                    "- Security: injection, unsafe operations, credential exposure\n",
                    "- Performance: unnecessary allocations, O(n^2) where O(n) suffices\n\n",
                    "Provide specific, actionable feedback. If the code is clean, say so briefly.\n\n",
                    "Code to review:\n{{ last_response }}"
                ).to_string(),
            },
        );

        // ── ail/test_writer ─────────────────────────────────────────────────
        skills.insert(
            "ail/test_writer".to_string(),
            SkillDefinition {
                name: "ail/test_writer".to_string(),
                description: "Generates unit tests for functions in the preceding response.".to_string(),
                prompt_template: concat!(
                    "You are a test engineer. Given the code below, write comprehensive unit tests.\n",
                    "Cover:\n",
                    "- Happy path\n",
                    "- Edge cases (empty input, boundary values, nulls)\n",
                    "- Error cases\n\n",
                    "Use the project's existing test framework and conventions.\n\n",
                    "Code to test:\n{{ last_response }}"
                ).to_string(),
            },
        );

        // ── ail/security_audit ──────────────────────────────────────────────
        skills.insert(
            "ail/security_audit".to_string(),
            SkillDefinition {
                name: "ail/security_audit".to_string(),
                description: "Security-focused review. Flags vulnerabilities.".to_string(),
                prompt_template: concat!(
                    "You are a security auditor. Analyse the following code for:\n",
                    "- Injection vulnerabilities (SQL, command, path traversal)\n",
                    "- Authentication and authorisation flaws\n",
                    "- Sensitive data exposure (secrets, PII)\n",
                    "- Unsafe deserialization\n",
                    "- Dependency vulnerabilities\n\n",
                    "For each finding, state the severity (CRITICAL/HIGH/MEDIUM/LOW) and a remediation.\n",
                    "If the code contains a vulnerability, include the word VULNERABILITY in your response.\n",
                    "If no issues are found, state: No security issues identified.\n\n",
                    "Code to audit:\n{{ last_response }}"
                ).to_string(),
            },
        );

        // ── ail/janitor ─────────────────────────────────────────────────────
        skills.insert(
            "ail/janitor".to_string(),
            SkillDefinition {
                name: "ail/janitor".to_string(),
                description: "Context distillation. Compresses working context to reduce token usage.".to_string(),
                prompt_template: concat!(
                    "You are a context distiller. Summarise the following content into the most compact ",
                    "form that preserves all actionable information. Remove redundancy, boilerplate, ",
                    "and verbose explanations. Keep code snippets, error messages, file paths, and ",
                    "specific values intact.\n\n",
                    "Content to distill:\n{{ last_response }}"
                ).to_string(),
            },
        );

        SkillRegistry { skills }
    }

    /// Look up a skill by name. Returns `Err(SkillUnknown)` if the name is not registered.
    pub fn resolve(&self, name: &str) -> Result<&SkillDefinition, AilError> {
        self.skills.get(name).ok_or_else(|| {
            let available: Vec<&str> = {
                let mut names: Vec<&str> = self.skills.keys().map(|s| s.as_str()).collect();
                names.sort();
                names
            };
            AilError::skill_unknown(format!(
                "Unknown skill '{name}'. Available built-in skills: {}",
                available.join(", ")
            ))
        })
    }

    /// List all registered skill names (sorted).
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.skills.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::error_types;

    #[test]
    fn registry_contains_code_review() {
        let registry = SkillRegistry::new();
        let skill = registry.resolve("ail/code_review").expect("should exist");
        assert_eq!(skill.name, "ail/code_review");
        assert!(!skill.prompt_template.is_empty());
    }

    #[test]
    fn registry_contains_test_writer() {
        let registry = SkillRegistry::new();
        let skill = registry.resolve("ail/test_writer").expect("should exist");
        assert_eq!(skill.name, "ail/test_writer");
    }

    #[test]
    fn registry_contains_security_audit() {
        let registry = SkillRegistry::new();
        let skill = registry
            .resolve("ail/security_audit")
            .expect("should exist");
        assert_eq!(skill.name, "ail/security_audit");
    }

    #[test]
    fn registry_contains_janitor() {
        let registry = SkillRegistry::new();
        let skill = registry.resolve("ail/janitor").expect("should exist");
        assert_eq!(skill.name, "ail/janitor");
    }

    #[test]
    fn unknown_skill_returns_typed_error() {
        let registry = SkillRegistry::new();
        let err = registry
            .resolve("ail/nonexistent")
            .expect_err("should fail");
        assert_eq!(err.error_type(), error_types::SKILL_UNKNOWN);
        assert!(err.detail().contains("nonexistent"));
        assert!(err.detail().contains("ail/code_review"));
    }

    #[test]
    fn list_returns_sorted_names() {
        let registry = SkillRegistry::new();
        let names = registry.list();
        assert!(names.len() >= 4);
        // Verify sorted
        for window in names.windows(2) {
            assert!(window[0] <= window[1], "names should be sorted");
        }
    }
}
