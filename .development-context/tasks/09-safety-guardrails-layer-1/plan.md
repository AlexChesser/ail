# Safety Guardrails — Layer 1 (Deterministic Enforcement Only)

**Status:** Design draft — iterating before committing to spec
**Issue:** AlexChesser/ail#122
**Parent:** #105 (Tier 3, Priority #11)
**Branch:** `claude/add-pipeline-guardrails-nTTFP`
**Target spec file:** `spec/core/s31-safety-guardrails.md` (next free section after §30)

---

## The Key Framing (do not lose this)

Issue #122 as originally scoped conflated two very different things:

- **Layer 1 — Deterministic:** What `ail` can enforce at parse time or through hard runtime checks. `ail` either allows the pipeline to run or it does not. No LLM in the enforcement loop.
- **Layer 2 — Probabilistic:** Prompt directives ("never output credentials") and regex blocklists on LLM output. Depends entirely on model obedience. LangChain, LangGraph, and the wider ecosystem all punt this to external libraries (NeMo Guardrails, Guardrails AI, `llm-guard`) because model-based validation of model output is fundamentally not a guarantee — it's a probability.

**Decision: this ticket implements Layer 1 only.** Layer 2 is explicitly documented as a non-goal, with a recommended pattern (validation step + external library via `context: shell:`) for users who need it.

### Why this reframe matters

1. **Enforceability:** Every Layer 1 rule is a deterministic check. Users who read the spec can trust that "blocked" actually means blocked.
2. **Liability:** Declining Layer 2 explicitly removes a class of liability exposure. We are not in the business of claiming we can detect PII in model output. We are in the business of guaranteeing that a pipeline cannot run a shell command outside the approved list.
3. **Tractability:** The issue becomes shippable in four reviewable phases instead of an open-ended design exercise.
4. **Parity with ecosystem:** Users who want Layer 2 get the same deal they get in LangChain/LangGraph — wire in Guardrails AI or equivalent — but they do it via a first-class `context: shell:` step that is itself governed by Layer 1 rules. That composition is better than what LangChain offers.

### What the user already has today

This is already in the codebase — these are **not** new features, and the design must not regress them:

- **§5.8 `tools:` allow/deny/disabled** on any `prompt:` step — `spec/core/s05-step-specification.md:424`. Supports simple tool names and Claude CLI pattern syntax like `Edit(./src/*)`. Can also be set in `defaults:` block for pipeline-wide policy. Inherits via `FROM`.
- **§7 `FROM` inheritance** with `run_before` / `run_after` / `override` / `disable` hook operations — `spec/core/s07-pipeline-inheritance.md`. Hook ops already produce parse errors when they target nonexistent IDs.
- **§4.4 turn log** — every run writes NDJSON events; new `safety.policy_applied` event slots into this existing infrastructure.

What's missing: the **governance flag** (`required: true`) that prevents a child pipeline from weakening a parent's policy, plus the allowlists for non-tool resources (skills, sub-pipelines, shell patterns, MCP servers).

---

## Proposed §31 Spec Structure

### §31.1 Scope boundary (stated up front)

Table form — this is the contract, not documentation:

| # | Rule | Enforcement point | Built on |
|---|------|-------------------|----------|
| 1 | Tool allow/deny with `required: true` | Parse time (child YAML validated against parent policy) | Extends §5.8 `tools:` |
| 2 | Skill path allowlist | Parse time (every `skill:` target resolved and matched) | New |
| 3 | Sub-pipeline / `FROM` path allowlist | Parse time (every `FROM:` and `pipeline:` target resolved and matched) | New |
| 4 | Shell command allow/deny patterns | Parse time (every `context: shell:` command matched) | New |
| 5 | MCP server allowlist | Runtime, via `tools: deny` expansion (`mcp__*` except listed servers) | Extends §5.8 |
| 6 | HITL integrity invariants | Parse time (enumerated named checks — no arbitrary regex) | New |
| 7 | Policy audit event | Runtime (turn log `safety.policy_applied` event with resolved policy hash) | Extends §4.4 |

**Explicit non-goals (documented in the spec):**

- Prompt directive injection ("never output credentials, PII, etc."). Model obedience is not enforceable. Recommended pattern: dedicated validation step with `input_schema` + `on_result.field.equals` for deterministic branching (already specified at `s21-planned-extensions.md:328-361`).
- Regex blocklists on LLM response content.
- PII detection, content moderation, jailbreak detection.
- Any rule whose enforcement requires an LLM call.

For all the non-goals, the spec recommends: add a `context: shell:` step that shells out to Guardrails AI / NeMo Guardrails / `llm-guard` / regex / whatever the org's policy requires, and branch on its exit code via `on_result`. That step is itself governed by Layer 1 rules.

### §31.2 YAML grammar

```yaml
safety:
  required: true   # block-level: every rule below is un-weakenable by children

  tools:
    allow: [Read, Glob, LS, Edit]
    deny: [Bash(rm*), Bash(sudo*), WebFetch]
    # merges with defaults.tools per §5.8; required:true makes these un-overridable

  mcp_allowlist:
    - filesystem
    - github
    # implicit: deny mcp__* for any server not listed

  skill_allowlist:
    - ail/*
    - ./skills/*
    - /etc/ail/approved-skills/*

  pipeline_allowlist:
    - ./pipelines/*
    - /etc/ail/approved-pipelines/*
    # applies to FROM: targets AND pipeline: step targets

  shell:
    allow:
      - "git *"
      - "cargo *"
      - "npm test*"
    deny:
      - "rm -rf*"
      - "curl *"
      - "* | sh"

  invariants:
    - hitl_gates_enabled      # rejects any step that sets disabled:true on pause_for_human
    - no_skip_permissions     # rejects runner config that passes --dangerously-skip-permissions
    - output_schema_required  # rejects prompt: steps without output_schema (opt-in, strict mode)
```

**`invariants` is a closed, named set.** No arbitrary YAML regex. Each name corresponds to a hand-written check with a clear error message. This deliberately dodges the "blocklist with non-deterministic pattern matching" trap that §21 left open.

### §31.3 Inheritance semantics

- Safety block inherits through `FROM` like `defaults:` does (§7).
- `required: true` at block level OR individual rule level means the child **cannot** remove, relax, or disable that rule. Any attempt is a parse error with the offending rule and the pipeline file where it was declared as required.
- A child **can** add more rules — allowlists intersect (narrow), denylists union (broaden).
- A child trying to **widen** an allowlist that a `required: true` parent declared raises a parse error. Narrowing is always allowed.
- A child trying to **remove** a denylist entry that a `required: true` parent declared raises a parse error.

### §31.4 Error model

New stable constant family in `error::error_types`:

```
safety/tool-policy-violation
safety/skill-not-permitted
safety/pipeline-not-permitted
safety/shell-pattern-denied
safety/mcp-server-not-permitted
safety/invariant-violated
safety/required-policy-weakened
```

Error messages always include:
- Which rule fired
- Which pipeline file declared the rule
- Which step/location violated it

No silent failures. No best-effort enforcement. Every violation aborts load-time or run-time.

### §31.5 Audit trail

At `run_start`, emit a `safety.policy_applied` event to the turn log with:

```json
{
  "event": "safety.policy_applied",
  "resolved_safety": { "...": "fully merged safety block after FROM resolution" },
  "policy_hash": "sha256:...",
  "source_pipelines": ["/etc/ail/org-base.yaml", "./team-base.yaml", ".ail.yaml"]
}
```

This is the compliance artifact — machine-readable record of exactly which rules governed the run. The `policy_hash` lets auditors verify policy didn't drift between runs.

### §31.6 `materialize` interaction

`ail materialize` shows the fully resolved `safety:` block with origin comments per rule (which pipeline file it came from). `required: true` rules render with a `# required by <path>` comment. Maintains the "no surprises" guarantee from §17.

New optional flag: `ail materialize --show-safety` prints only the resolved safety block (useful for audit scripts that don't want the full pipeline).

### §31.7 CLI surface

- `ail validate --pipeline foo.yaml` — already exists in `ail/src/main.rs`; extended to run all §31 parse-time checks.
- `ail materialize --show-safety` — new flag.
- No new top-level commands.

---

## Implementation Phasing (for the issue)

Each phase independently shippable and gives a coherent subset of the governance story.

### Phase 1 — Governance on existing `tools:`

- Add `required: true` to §5.8 grammar.
- Wire FROM-inheritance rejection in `ail-core/src/config/inheritance.rs`.
- Smallest change; highest leverage; proves the governance pattern works before we build more.
- No new spec file needed yet — extends §5.8 in place.

### Phase 2 — Path allowlists

- New `safety:` top-level block with `skill_allowlist` and `pipeline_allowlist` (glob matching).
- Pure parse-time check against resolved paths.
- Creates `spec/core/s31-safety-guardrails.md` for real.
- No runtime changes.

### Phase 3 — Shell patterns + MCP allowlist

- Shell pattern matching at parse time (every `context: shell:` command checked).
- MCP allowlist expands into `--disallowedTools mcp__*` at runtime in `runner/claude/permission.rs`.
- Touches runner layer — keep this PR isolated from Phase 2.

### Phase 4 — Invariants + audit event

- Named invariant checks (`hitl_gates_enabled`, `no_skip_permissions`, `output_schema_required`).
- Emit `safety.policy_applied` to turn log at run_start.
- Each invariant is ~20 lines of explicit code with a matching test.

---

## Issue #122 Acceptance Criteria — Proposed Revision

**Drop:**
- "blocklists (forbidden patterns in prompts or outputs)"
- "output filtering"
- "pre-execution (block the prompt)"
- "sanitize and continue" outcome

**Keep (reworded):**
- Guardrail types = allowlists (tools, skills, pipelines, shell, MCP) + named invariants
- YAML syntax = top-level `safety:` block with FROM-inheritance via `required: true`
- Pattern format = globs for paths, regex for shell commands, literal names for tools/MCP servers
- Enforcement = parse-time except MCP allowlist (runtime via deny expansion)
- On violation = always abort with typed error; never sanitize-and-continue
- Violations recorded in turn log via `safety.policy_applied` event + run abort

**Add explicit non-goal statement to issue body:**

> Content filtering of LLM output is a Layer 2 concern and is out of scope. `ail` documents the `input_schema` + validation-step pattern (§21) and recommends external libraries (Guardrails AI, NeMo Guardrails, `llm-guard`) composed via `context: shell:` for workflows requiring this.

---

## Open Design Questions to Iterate On

Order of least-confident to most-confident:

1. **Does `invariants` belong in §31 at all, or is it feature-gating in disguise?**
   `hitl_gates_enabled` is clearly safety. `output_schema_required` is more like a strict-mode configuration option. If we include it in `safety:`, we risk the `invariants` list becoming a dumping ground for every "org policy" a buyer wants. Alternatives:
   - Keep only clearly safety-relevant invariants; move the rest to a separate `strict:` block.
   - Allow plugins to register invariants (ties into §21 plugin extensibility).
   - Drop invariants entirely from v1; address on demand as real use cases surface.

2. **Shell pattern matching: regex or glob-only?**
   Globs (`rm -rf*`) are easier to write and audit but less expressive. Regex (`^rm\s+-rf`) is powerful but easier to get wrong (anchors, escaping). Candidates:
   - Glob-only for v1 (keep it simple; Claude CLI tool patterns are glob-style already, so this matches).
   - Glob with `regex:` escape hatch (`regex:^rm\s+-rf`).
   - Full regex (rejected — too easy to footgun).

3. **Does `tools: required: true` exist today?**
   Need to verify in `config/inheritance.rs` whether `defaults.tools` is already `required`-style by default (i.e. children can't override) or merge-style (children can override). If merge-style, Phase 1 adds a flag; if already required-style, Phase 1 is a no-op and we go straight to Phase 2.

4. **MCP allowlist semantics — allowlist or denylist-by-default?**
   Proposal above is "allowlist → everything else denied." But Claude CLI's MCP integration auto-discovers servers from config. We might need to think about how `mcp_allowlist: []` behaves (deny-all) vs absent block (allow-all based on Claude config) vs `mcp_allowlist: [*]` (explicit allow-all). Needs runner-spec coordination.

5. **How does safety interact with the plugin runner system (§19, r10)?**
   A plugin runner could in principle bypass the Claude CLI tool permission mechanism. The safety block needs to be enforceable regardless of which runner handles a step. Enforcement should happen in `executor` dispatch, not inside individual runners.

6. **Do we want `safety:` visible in `materialize` by default, or opt-in?**
   §21 open questions asked this about directives: "should they be opaque to prevent circumvention?" For allowlists/denylists we probably want visibility (authors need to know what they can't do). But for `tools: deny: [WebFetch]` in a heavily-governed enterprise setup, the visibility might itself leak policy. Lean toward: visible by default, with a `visibility: private` flag per rule for sensitive cases.

---

## Files That Will Change (Implementation Preview)

Not authoritative — just to keep an eye on blast radius:

**Spec:**
- `spec/core/s31-safety-guardrails.md` (new)
- `spec/core/s05-step-specification.md` (§5.8 — add `required:` flag docs)
- `spec/core/s07-pipeline-inheritance.md` (inheritance rules for safety block)
- `spec/core/s17-materialize.md` (safety block rendering + `--show-safety` flag)
- `spec/core/s21-planned-extensions.md` (replace Safety Guardrails subsection with pointer to §31)
- `spec/core/s04-execution-model.md` (§4.4 — new `safety.policy_applied` event)
- `spec/README.md` (index)

**Code (ail-core):**
- `src/config/dto.rs` — `Safety` DTO
- `src/config/domain.rs` — `Safety` domain type
- `src/config/validation/` — new `safety.rs` module, integration into `mod.rs`
- `src/config/inheritance.rs` — `required:true` enforcement
- `src/error.rs` — new `safety/*` error_types constants
- `src/executor/` — parse-time checks wired in before step dispatch (skill, shell, pipeline steps)
- `src/runner/claude/permission.rs` — MCP allowlist expansion
- `src/session/` — `safety.policy_applied` event emission

**Tests:**
- `ail-core/tests/spec/s31_safety_guardrails.rs` (new, following per-section convention)
- Fixtures under `ail-core/tests/fixtures/safety_*.yaml`

**Docs:**
- `ail-core/CLAUDE.md` — mention safety module responsibilities if we add one

---

## Conversation Context (for future compacted re-entry)

User's instinct on this: rightly skeptical that LLM-output-based guardrails could ever be a guarantee. Wants `ail`'s value prop to be "deterministic things work; probabilistic things compose with external tools." That framing drove the Layer 1 / Layer 2 split and the explicit non-goal statement. Branch is `claude/add-pipeline-guardrails-nTTFP`; no code has been written yet, only this design. Next step is iterating on the six open questions above before producing the actual spec file.
