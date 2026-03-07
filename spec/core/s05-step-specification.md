## 5. Step Specification

Every item in the `pipeline` array is a step. Each step does exactly one of four things, declared by its primary field:

| Primary field | What it does |
|---|---|
| `prompt:` | Sends text to the LLM — inline string or path to a `.md` file. |
| `skill:` | Loads a `SKILL.md` package as model instructions, optionally combined with a `prompt:`. |
| `pipeline:` | Calls another pipeline as an isolated sub-routine. |
| `action:` | Performs a non-LLM operation (e.g. `pause_for_human`). |

Exactly one primary field is required per step. All other fields are optional.

### 5.1 Core Fields

| Field | Description |
|---|---|
| `id` | String. **Required.** Unique identifier for this step within the resolved pipeline. Snake_case recommended. Step IDs are the public API of a `FROM`-able pipeline — treat them as stable identifiers. |
| `prompt` | String or file path. Inline text or path to a `.md` file. Path detected by prefix: `./` `../` `~/` `/`. |
| `skill` | Path. Loads an Agent Skills-compliant `SKILL.md` package. See §6. |
| `pipeline` | File path. Calls another `.ail.yaml` as an isolated sub-pipeline. See §9. |
| `action` | String. A non-LLM operation. Supported: `pause_for_human`. |
| `provider` | String. Overrides the default provider for this step. |
| `timeout_seconds` | Integer. Maximum seconds to wait. Default: `120`. |
| `condition` | Expression string. Step is skipped if false. See §12. |
| `on_error` | Enum: `continue` \| `pause_for_human` \| `abort_pipeline` \| `retry`. Default: `pause_for_human`. |
| `max_retries` | Integer. Retry attempts when `on_error: retry`. Default: `2`. |
| `on_result` | Block. Declarative branching after completion. See §5.3. |
| `disabled` | Boolean. Skips step unconditionally. Useful during development. |
| `before` | List. Private pre-processing steps that run before this step's prompt fires. See §5.7. |
| `then` | List. Private post-processing steps chained to this step. See §5.5. |
| `tools` | Block. Pre-approve or pre-deny tool calls for this step. See §5.6. |
| `resume` | Boolean. When `true`, resumes the most recent preceding session on the same provider. See §15.4. |

**`id` is always required.** Every step must declare an explicit `id`. Because any pipeline may be inherited from via `FROM`, `ail` cannot know at parse time which steps will be targeted by hook operations in inheriting pipelines. Step IDs must be stable identifiers — renaming a step ID in a `FROM`-able pipeline is a breaking change for all inheritors.

### 5.2 `prompt:` — Inline and File

```yaml
# Inline prompt
- id: simple_check
  prompt: "Review the above output and fix anything obviously wrong."

# Prompt loaded from a markdown file
- id: detailed_review
  prompt: ./prompts/architectural-review.md

# Prompt from parent directory
- id: org_style_check
  prompt: ../org-prompts/style-guide-check.md

# Prompt from home directory
- id: personal_conventions
  prompt: ~/prompts/my-conventions.md
```

Files are read at pipeline load time. Template variables within files are resolved at step execution time.

### 5.3 `on_result` — Declarative Branching

```yaml
- id: security_audit
  prompt: "Identify any security vulnerabilities. If none, respond CLEAN."
  on_result:
    contains: "CLEAN"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security issues detected. Review before proceeding."
```

**Supported actions:**

| Action | Effect |
|---|---|
| `continue` | Proceed to next step. Default if `on_result` omitted. |
| `pause_for_human` | Suspend pipeline. Wait for Approve / Reject / Modify. |
| `preview_for_human` | Show transformed prompt alongside original. Human chooses: use transformed, use original, or edit. See §5.7. |
| `use_original` | Discard `before:` transformation. Pass raw prompt to parent step unchanged. Only valid inside a `before:` chain. |
| `abort_pipeline` | Stop immediately, treating the pipeline as failed. Logged to audit trail. |
| `repeat_step` | Re-run this step. Respects `max_retries`. |
| `break` | Exit the current pipeline cleanly. Remaining steps are skipped. Not an error — the pipeline completed successfully with an intentional early exit. In a sub-pipeline, returns control to the caller. |
| `pipeline: <path>` | Conditionally call another pipeline. Equivalent to a `pipeline:` step but triggered by `on_result` match. Follows the same isolation model as §9. |

**Match operators:**

| Operator | Meaning |
|---|---|
| `contains: "TEXT"` | Response contains literal string (case-insensitive). |
| `matches: "REGEX"` | Response matches regular expression. |
| `starts_with: "TEXT"` | Response begins with literal string. |
| `is_empty` | Response is blank or whitespace only. |
| `always` | Unconditionally fires. |

> ⚠️ **Reliability warning — prose matching is best-effort.**
> The `contains`, `matches`, and `starts_with` operators match against free-form LLM text output. LLMs are not deterministic. A step instructed to respond `CLEAN` may respond `CLEAN.`, `Yes, CLEAN`, or `The code is clean` — all of which fail a `contains: "CLEAN"` check. **Prose-based `on_result` branching is not a reliable control flow mechanism.**
>
> **Improving reliability with constrained prompts.** You can significantly reduce variance by instructing the model to respond with a single, exact token: `"Answer only with CLEAN or VULNERABILITIES_FOUND, nothing else."` This does not make prose matching a hard contract — the model may still deviate — but it narrows the output space substantially and makes `contains` checks much more reliable in practice.
>
> For a genuine contract, use `input_schema` with the `field:` + `equals:` operators. See §22 (Planned Extensions — Structured Step I/O Schemas). The `contains` and `matches` operators are best suited to advisory checks, logging triggers, and `always`-fired actions where a missed match is acceptable.

**`break` vs `abort_pipeline`:**

| Action | Intent | Exit state | Caller behaviour |
|---|---|---|---|
| `break` | Intentional early exit | Success | Sub-pipeline returns cleanly; caller continues |
| `abort_pipeline` | Something went wrong | Failure | Caller's `on_error` fires |

```yaml
- id: early_exit_check
  prompt: "Does this response contain any code changes? Answer CODE_CHANGED or NO_CODE."
  on_result:
    contains: "NO_CODE"
    if_true:
      action: break
      message: "No code changes — quality gates skipped."
    if_false:
      action: continue
```

### 5.4 Step Output Model

Each step captures its output as `step.<id>.response` — the final text produced, available to subsequent steps via template variables resolved from the pipeline run log.

**Full step lifecycle:**

```
before: chain          ← private pre-processing; may transform the input prompt
  ↓
  (use_original bypasses transformation; raw prompt proceeds unchanged)
  ↓
parent step fires      ← LLM receives the (possibly transformed) prompt
  ↓
parent step completes  ← response captured; log entry written to disk
  ↓
on_result evaluated    ← declarative branching
  ↓
then: chain            ← private post-processing
  ↓
next step
```

For steps where `ail` calls an LLM provider directly, structured output (thinking traces, tool call sequences) is additionally captured in the pipeline run log. The full structured model is under active research — see §22 (Planned Extensions — Structured Step I/O Schemas).

For steps that wrap third-party CLI runners, `ail` captures stdout as the response. See §23 Open Questions for details on runner-specific behaviour.

### 5.5 `then:` — Private Post-Processing Chains

`then:` attaches a private chain of post-processing steps directly to a parent step. Steps in a `then:` chain are:

- **Not visible to the hook system** — they cannot be targeted by `run_before`, `run_after`, `override`, or `disable` from any inheriting pipeline.
- **Not independently referenceable** — their output is not accessible via `{{ step.<id>.response }}` from the wider pipeline.
- **Unconditionally run** — they execute after the parent step completes, regardless of `on_result`. If the parent step is skipped by its `condition`, the `then:` chain is also skipped.
- **Tightly coupled** — they are considered part of the parent step's execution, not peers.

This makes `then:` the right tool for housekeeping that belongs to a step — context distillation, internal scoring, cleanup — where forcing a full top-level step would create visual noise and false hookability.

#### Short-form entries

A `then:` entry may be a bare string — a skill reference or prompt file path — when no additional configuration is needed:

```yaml
- id: security_audit
  prompt: ./prompts/security-audit.md
  then:
    - ail/janitor              # bare skill reference
    - ./prompts/cleanup.md     # bare prompt file
```

#### Full-form entries

When configuration is needed, a `then:` entry may be a full step block. All standard step fields are supported except `id` (auto-generated as `<parent_id>::then::<index>`), `condition` (inherited from parent), and `on_result` (use top-level steps if branching is needed):

```yaml
- id: security_audit
  prompt: ./prompts/security-audit.md
  then:
    - skill: ail/janitor
    - prompt: "Summarise the findings in one sentence for the audit log."
      provider: fast
```

#### Mixed short and full form

```yaml
- id: my_step
  prompt: "Generate the feature implementation."
  then:
    - ail/janitor                    # short-form
    - skill: ail/dry-refactor        # full-form with skill
    - prompt: ./prompts/score.md     # full-form with prompt file
      provider: fast
```

#### `materialize-chain` representation

`then:` steps appear in `materialize-chain` output subordinated under their parent, annotated as private and non-hookable:

```yaml
# origin: [2] .ail.yaml
- id: security_audit
  prompt: "..."
  # then: (private — not hookable)
  #   - id: security_audit::then::0  skill: ail/janitor
  #   - id: security_audit::then::1  prompt: ./prompts/cleanup.md
```

#### When not to use `then:`

If a post-processing step needs to:
- Be visible or hookable by inheriting pipelines
- Be referenceable by later steps via `{{ step.<id>.response }}`
- Branch via `on_result`

...it should be a top-level step, not a `then:` entry.

### 5.6 `tools:` — Pre-Approved and Pre-Denied Tool Calls

`tools:` on a step declares which Claude CLI tools are unconditionally allowed or denied before the permission callback is consulted. This eliminates HITL prompts for tools the pipeline author has already deemed safe or unsafe for a given step.

```yaml
# Simple allow/deny lists
- id: security_audit
  prompt: ./prompts/security-audit.md
  tools:
    allow: [Read, Glob, LS]
    deny: [Bash, Git, WebFetch]

# Pattern syntax — passed verbatim to --allowedTools / --disallowedTools
- id: constrained_refactor
  prompt: ./prompts/refactor.md
  tools:
    allow:
      - Read
      - Edit(./src/*)          # only edit files under src/
      - Bash(git log*)         # only git log commands
    deny:
      - Bash(rm *)             # deny destructive bash
      - WebFetch
```

#### How it works

`ail` passes `tools.allow` as `--allowedTools` and `tools.deny` as `--disallowedTools` when invoking the Claude CLI for this step. Claude enforces these before reaching the permission callback — pre-approved tools execute silently, pre-denied tools are rejected silently.

Tools not listed in either fall through to `ail`'s HITL permission UI.

#### Three-tier tool behaviour

| Tier | Mechanism | User sees |
|---|---|---|
| Pre-approved | `tools.allow` → `--allowedTools` | Nothing — executes silently |
| Pre-denied | `tools.deny` → `--disallowedTools` | Nothing — rejected silently |
| Unspecified | Falls through to HITL | Permission prompt in TUI |

#### Inheritance

`tools:` may be declared in the `defaults:` block to apply a pipeline-wide policy. Per-step declarations override the default for that step. Via `FROM` inheritance, an org base pipeline can establish a safe default tool policy that all child pipelines inherit.

```yaml
defaults:
  tools:
    allow: [Read, Glob, LS]   # safe read-only tools — pipeline-wide default
    deny: [WebFetch]           # no network access anywhere in this pipeline

pipeline:
  - id: refactor
    tools:
      allow: [Read, Glob, LS, Edit, Bash(git diff*)]  # extend for this step
      deny: [WebFetch]
```

#### Interaction with `then:`

`then:` chain steps inherit their parent step's `tools:` policy unless explicitly overridden within the full-form `then:` entry.

#### Pattern syntax

`ail` does not parse or validate tool patterns — they are passed verbatim to the Claude CLI. Pattern syntax follows Claude CLI conventions (e.g. `Bash(git log*)`, `Edit(./src/*)`). Refer to the Claude CLI reference for supported pattern forms.

### 5.7 `before:` — Private Pre-Processing Chains

`before:` attaches a private chain of pre-processing steps that run after a step is triggered but before its prompt is sent to the LLM. This is the symmetric counterpart to `then:` — where `then:` operates on output, `before:` operates on input.

Steps in a `before:` chain share the same privacy properties as `then:` steps:

- **Not visible to the hook system** — they cannot be targeted by `run_before`, `run_after`, `override`, or `disable` from any inheriting pipeline.
- **Not independently referenceable** — their output is not accessible via `{{ step.<id>.response }}` from the wider pipeline.
- **Tightly coupled** — they are considered part of the parent step's execution.

The key difference from `then:`: a `before:` step's output becomes the transformed input for the parent step's LLM call. The original prompt is still accessible — and can be restored — via the `use_original` action.

#### Use Cases

**Prompt optimisation.** Transform a casual user prompt into a structured, research-backed LLM request before the agent sees it:

```yaml
pipeline:
  - run_before: invocation
    id: prompt_optimizer
    skill: ail/prompt-optimizer
    before:
      - skill: ail/prompt-optimizer
        on_result:
          always:
            action: preview_for_human
            message: "Your prompt was transformed. Use the optimised version?"
            show_original: true
            if_rejected:
              action: use_original
```

**Context compaction.** Compress accumulated conversation context before an expensive step on a long pipeline:

```yaml
- id: architecture_review
  prompt: ./prompts/arch-review.md
  before:
    - ail/janitor
```

**Context gathering.** Retrieve relevant information and inject it as context before the parent step's LLM call — useful for sub-agent or critic steps:

```yaml
- id: code_critic
  skill: ./skills/critic/
  before:
    - prompt: "Summarise the files changed in the last 3 steps in one paragraph."
      provider: fast
```

#### Short and Full Form

`before:` entries support the same short-form and full-form syntax as `then:`:

```yaml
before:
  - ail/janitor                    # short-form: bare skill reference
  - ./prompts/gather-context.md    # short-form: bare prompt file
  - skill: ail/prompt-optimizer    # full-form
    on_result:
      always:
        action: preview_for_human
        show_original: true
        if_rejected:
          action: use_original
```

#### The `preview_for_human` Circuit Breaker

When a `before:` step transforms a prompt, the human may not know it happened. For prompt transformation use cases — especially on `invocation` — the `preview_for_human` action provides a transparent opt-in circuit breaker.

```yaml
on_result:
  always:
    action: preview_for_human
    message: "Your prompt was optimised. Use the transformed version?"
    show_original: true    # display original alongside transformed in TUI
    if_rejected:
      action: use_original
```

The TUI renders the original and transformed prompts side by side. The human chooses one of three outcomes:

| Choice | Effect |
|---|---|
| **Use transformed** | Transformation proceeds. Parent step receives optimised prompt. |
| **Use original** | `use_original` fires. Transformation discarded silently. Parent step receives raw prompt. Transformation still appears in audit trail. |
| **Edit** | Human edits the transformed version inline. Edited version proceeds. |

The circuit breaker is recommended whenever prompt transformation is used in a context where the human might not expect it — particularly on `invocation` and in `FROM` base pipelines.

#### `use_original` Semantics

`use_original` is only valid inside a `before:` chain. It instructs the pipeline executor to discard the `before:` chain's output and pass the parent step's original prompt unchanged. The `before:` steps still execute and their outputs are recorded in the pipeline run log — transparency is preserved — but they do not affect what the LLM receives.

`use_original` used outside a `before:` chain raises a parse error.

#### `materialize-chain` Representation

`before:` steps appear in `materialize-chain` output subordinated under their parent, annotated as private and non-hookable, above the parent step prompt:

```yaml
# origin: [2] .ail.yaml
- id: security_audit
  # before: (private — not hookable)
  #   - id: security_audit::before::0  skill: ail/janitor
  prompt: "..."
  # then: (private — not hookable)
  #   - id: security_audit::then::0  prompt: ./prompts/cleanup.md
```

#### ⚠️ Governance Warning — `before:` on `invocation` in `FROM` Pipelines

`before:` on the `invocation` step in a `FROM` base pipeline silently transforms every user prompt in every session for every team that inherits from that pipeline. This is the most powerful and most consequential configuration in the entire spec.

**Risks:**
- Users may not know their prompts are being transformed.
- Transformations that improve prompts on average may degrade specific ones.
- A flawed transformation in a base pipeline affects all inheritors simultaneously.

**Detection:** When `ail materialize-chain` resolves a pipeline that contains a `before:` chain on `invocation` — whether declared directly or inherited via `FROM` — it emits a prominent warning identifying the origin pipeline and noting that prompt transformation is active on every invocation. This warning is rendered in the interactive TUI at session start.

This warning is a **UI-layer concern only**. It is not a parse error, not a lint failure, and is not emitted in headless or agent-driven modes. A pipeline with `before:` on `invocation` and no `preview_for_human` is fully valid — the warning exists to surface the configuration to humans who may not have inspected their full inheritance chain. Requiring `preview_for_human` would make such pipelines incompatible with unattended runs, in direct conflict with the Agent-First Design principle.

**Mitigations the spec provides:**
- The `preview_for_human` circuit breaker makes transformation visible and opt-out-able in interactive sessions.
- `materialize-chain` always shows `before:` chains — pipeline authors can inspect what they've inherited.
- An inheriting pipeline can `override:` the invocation hook to replace it with a version that has no `before:` chain.

**The recommended pattern for `FROM` base pipelines:**

If you use `before:` on `invocation` in a base pipeline, always include `preview_for_human` with `show_original: true` for interactive sessions. Give inheritors the ability to see and reject the transformation. Do not silently transform prompts in shared infrastructure.

#### When Not to Use `before:`

If a pre-processing step needs to:
- Be visible or hookable by inheriting pipelines
- Produce output referenceable by later steps via `{{ step.<id>.response }}`
- Branch via `on_result` in a way that affects the wider pipeline

...it should be a top-level step preceding the parent, not a `before:` entry.

---
