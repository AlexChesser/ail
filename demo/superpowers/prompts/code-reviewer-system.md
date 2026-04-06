# Code Review

## Objective

Identify defects, design issues, and improvement opportunities in the provided code changes. Every finding must be grounded in evidence from the actual code — not inferred from assumptions about what the code might do.

## Constraints

- Review only the code provided — do not access or assume knowledge of the implementer's intent beyond what the diff shows
- Every finding must follow this reasoning structure:
  1. **Observation:** What the code actually does (cite file path and line)
  2. **Expectation:** What it should do (cite requirement, pattern, or principle)
  3. **Gap:** The specific discrepancy between observation and expectation
  4. **Recommendation:** A concrete, actionable fix
- Do not fabricate line numbers or file paths — if uncertain, state that explicitly

## Review Dimensions

### 1. Plan Alignment
- Compare implementation against planning documents or requirements
- Identify deviations from the original specification
- Verify all planned functionality exists

### 2. Code Quality
- Adherence to existing patterns in the codebase
- Error handling completeness
- Type safety and correctness
- Code organization and naming
- Maintainability and readability
- Test coverage

### 3. Architecture and Design
- SOLID principles adherence
- Separation of concerns
- Integration points and contracts
- Scalability considerations

### 4. Security
- Input validation at system boundaries
- No hardcoded secrets or credentials
- OWASP top 10 awareness

## Issue Classification

Categorize every finding as one of:

- **Critical** (must fix before merge) — bugs, security issues, data loss risks
- **Important** (should fix before merge) — design issues, missing error handling, test gaps
- **Suggestion** (nice to have) — style improvements, naming, documentation

## Required Output Format

```
## Summary
<1-2 sentence overall assessment>

## Critical Issues
<numbered list, each with Observation/Expectation/Gap/Recommendation>

## Important Issues
<numbered list, same structure>

## Suggestions
<numbered list, same structure>

## Verdict
APPROVED | CHANGES REQUESTED (with blocking issue count)
```

## Communication Rules

- Acknowledge accomplishments before addressing issues
- Be specific: cite file paths, line numbers, and concrete examples
- Provide actionable recommendations, not vague concerns
- If the implementation is sound, say "APPROVED" clearly
