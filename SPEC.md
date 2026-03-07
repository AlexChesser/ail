# AIL Pipeline Language Specification

> **ail** — Alexander's Impressive Loops *(super-secret project real name for those in the know)*
> *Refer to this as: Artificial Intelligence Loops — in front of the business muggles.*
> *The control plane for how agents behave after the human stops typing.*

---

## Table of Contents

1. [Purpose & Philosophy](#1-purpose--philosophy)
2. [Concepts & Vocabulary](#2-concepts--vocabulary)
3. [File Format](#3-file-format)
4. [The Pipeline Execution Model](#4-the-pipeline-execution-model)
5. [Step Specification](#5-step-specification)
6. [Skills](#6-skills)
7. [Pipeline Inheritance](#7-pipeline-inheritance)
8. [Hook Ordering — The Onion Model](#8-hook-ordering--the-onion-model)
9. [Calling Pipelines as Steps](#9-calling-pipelines-as-steps)
10. [Named Pipelines & Composition](#10-named-pipelines--composition) *(deferred)*
11. [Template Variables](#11-template-variables)
12. [Conditions](#12-conditions)
13. [Human-in-the-Loop (HITL) Gates](#13-human-in-the-loop-hitl-gates)
14. [Built-in Modules](#14-built-in-modules)
15. [Providers](#15-providers)
16. [Triggers](#16-triggers)
17. [Error Handling & Resilience](#17-error-handling--resilience)
18. [The `materialize-chain` Command](#18-the-materialize-chain-command)
19. [Complete Examples](#19-complete-examples)
20. [Runners & Adapters](#20-runners--adapters)
21. [MVP — v0.0.1 Scope](#21-mvp--v001-scope)
22. [Planned Extensions](#22-planned-extensions)
23. [Open Questions](#23-open-questions)

---

## 1. Purpose & Philosophy

Current agentic coding tools treat a human prompt as a single transactional event. If a developer wants a refactor or a security audit after code is generated, they must manually type the follow-up prompt every single time. This creates inconsistent quality and *prompt fatigue*.

**ail** introduces the **Deterministic Post-Processor**: a YAML-orchestrated pipeline runtime that ensures a specific, pre-determined chain of automated prompts fires after every human prompt — consistently, without manual intervention.

> **The Core Guarantee**
> For every completion event produced by an underlying agent, `ail` will begin executing the pipeline defined in the active `.ail.yaml` file before control returns to the human. Steps execute in order. Individual steps may be skipped by declared conditions or disabled explicitly. Execution may terminate early via `break`, `abort_pipeline`, or an unhandled error. All of these are explicit, declared outcomes — not silent failures. The human never receives runner output without the pipeline having had the opportunity to run.

The AIL Pipeline Language (APL) is the product. The orchestration engine is its runtime. Everything else — context distillation, learning loops, multi-model routing — are optional pipeline steps, not architectural prerequisites.

### The Two Layers

`ail` operates across two distinct layers that must never be confused:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime engine | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable.

---

## 2. Concepts & Vocabulary

| Term | Definition |
|---|---|
| `pipeline` | A named, ordered sequence of steps defined in a `.ail.yaml` file. One pipeline is "active" per session. |
| `step` | A single unit of work within a pipeline. A step invokes a prompt, skill, sub-pipeline, or action, then optionally branches on the result. |
| `invocation` | The implicit first step of every pipeline. Represents the triggering event — a human prompt, an agent call, or a scheduler firing — and the runner's response to it. |
| `skill` | A directory containing a `SKILL.md` file — natural language instructions that tell the model how to perform a specialised task. Read by the LLM, not the runtime. |
| `trigger` | The event that causes the pipeline to begin executing. The default trigger is `invocation_prompt_complete`. |
| `session` | One running instance of an underlying agent (e.g. Aider, Claude Code) managed by `ail`. |
| `completion event` | The signal that the underlying runner has finished. For CLI tools, this is typically process exit with code 0. See §23 Open Questions. |
| `HITL gate` | A Human-in-the-Loop gate. The pipeline pauses and waits for explicit human input before continuing. |
| `pipeline run log` | The durable, persisted record of a pipeline execution. Written to disk before the next step runs. The authoritative source for template variable resolution. See §4.4. |
| `context` | The working memory passed between pipeline steps, accessed via the pipeline run log and template variables. |
| `provider` | The LLM backend a step routes its prompt to. May differ per step. |
| `condition` | A boolean expression evaluated before a step runs. If false, the step is skipped. |
| `on_result` | Declarative branching logic that fires after a step completes, based on the content of the response. |
| `FROM` | Keyword declaring that this pipeline inherits from another. Accepts a file path. Chainable. Must be acyclic. |
| `run_before` | Hook keyword. Inserts a step immediately before the named step ID in the inherited pipeline. |
| `run_after` | Hook keyword. Inserts a step immediately after the named step ID in the inherited pipeline. |
| `override` | Replaces a named step from an inherited pipeline entirely. |
| `disable` | Removes a named step from an inherited pipeline without replacing it. |
| `materialize-chain` | CLI command that traverses the full inheritance chain and writes the resolved pipeline to disk. |

---

## 3. File Format

### 3.1 Discovery

`ail` looks for a pipeline definition file using the following resolution order. The first match wins.

1. Explicit path passed via `--pipeline <path>` CLI flag.
2. `.ail.yaml` in the current working directory.
3. `.ail/default.yaml` in the current working directory.
4. `~/.config/ail/default.yaml` (user-level default).

If no file is found, `ail` runs in **passthrough mode**: the underlying agent behaves exactly as if `ail` were not present. This is the zero-configuration safe default.

The discovery order is significant beyond file resolution — it is the **authority order** that governs hook precedence in inherited pipelines. See §8.

### 3.2 Top-Level Structure

```yaml
# .ail.yaml
version: "0.1"              # required; must match supported spec version

FROM: ./base.yaml           # optional; inherit from another pipeline (see §7)
                            # accepts file paths only — see §22 for future URI support

meta:                       # optional block
  name: "My Quality Gates"
  description: "DRY refactor + security audit on every output"
  author: "alex@example.com"

providers:                  # optional; named provider aliases (see §15)
  fast:     groq/llama-3.1-70b-versatile
  frontier: anthropic/claude-opus-4-5

defaults:                   # optional; inherited by all steps
  provider: openai/gpt-4o
  timeout_seconds: 120
  on_error: pause_for_human
  tools:                    # pipeline-wide tool policy; overridable per step
    allow: [Read, Glob, LS]
    deny: [WebFetch]

pipeline:                   # required; ordered list of steps
  - id: dry_refactor
    prompt: "Refactor the code above to eliminate unnecessary repetition."

  - id: security_audit
    prompt: "Review the changes for common security vulnerabilities."
```

**Version field:** The `version` field declares the minimum `ail` runtime version required to execute this pipeline. Each file in a `FROM` chain makes its own independent version declaration. The active `ail` runtime must support all versions declared anywhere in the resolved chain — if any file declares a version higher than the runtime supports, `ail` raises a fatal parse error identifying the conflicting file and recommending a runtime upgrade. There is no constraint on relative versions between files in the chain.

---

## 4. The Pipeline Execution Model

### 4.1 `invocation` — Step Zero

Every pipeline has an implicit first step called `invocation`. It is always step zero, always exists, and can always be referenced by subsequent steps via template variables.

`invocation` represents the triggering event and the runner's response to it. The trigger may be:

- A human typing a prompt into the underlying agent
- Another pipeline calling this one as a step
- A scheduled or manual trigger

The pipeline's authored steps begin executing only after `invocation` completes. `ail` never intercepts or replaces the triggering interaction — it extends it.

```
invocation           ← step zero; always present
  ↓
step_1               ← first authored step in the pipeline
  ↓
step_2
  ↓
  ...
  ↓
[control returns to caller]
```

#### Declaring `invocation` in YAML

`invocation` may be declared explicitly as the first step in the pipeline YAML. When declared, it must be the first step; placing it anywhere else is a validation error.

```yaml
version: "0.0.1"
pipeline:
  - id: invocation
    prompt: "{{ session.invocation_prompt }}"
  - id: review
    prompt: "Review the above output."
```

**If `invocation` is declared**, the executor runs it as a first-class step with whatever configuration the user supplied — this includes a custom prompt template, a non-default model or provider, `before:`/`then:` hooks, or any other step-level configuration. The host does not run a separate default invocation.

**If `invocation` is not declared**, the host runs a default invocation (the `--once` prompt, plain settings) before handing off to the executor. This is the minimal case for pipelines that do not need to customise how the triggering interaction is handled.

Declaring `invocation` in YAML also makes it visible in `materialize-chain` output, which is the primary way readers understand what a pipeline does end-to-end.

#### `invocation` in `FROM` chains

When a pipeline inherits from a base pipeline via `FROM`, the `invocation` step belongs to the triggering pipeline — not the base. The inherited steps execute after `invocation` completes, in the order they appear in the resolved (materialised) pipeline. The `invocation` step is never inherited and never duplicated; it fires exactly once per pipeline run, always as step zero.

Because `invocation` names the event rather than the actor, the template variables are unambiguous regardless of what triggered the pipeline:

- `{{ step.invocation.prompt }}` — the input that triggered this pipeline run
- `{{ step.invocation.response }}` — the runner's response before any pipeline steps ran

### 4.2 Execution Guarantee

Once an `invocation` completion event fires, `ail` begins executing the pipeline before control returns to the caller. If a HITL gate fires mid-pipeline, control remains locked until the human responds. Individual steps may be skipped by declared conditions, and execution may terminate early via `break`, `abort_pipeline`, or an unhandled error — all of which are explicit, declared outcomes recorded in the pipeline run log.

### 4.3 Hooks on `invocation`

Hook operations may target `invocation` directly, enabling session setup before the first prompt is processed.

```yaml
- run_before: invocation
  id: session_banner
  action: pause_for_human
  message: "Reminder: all outputs in this session are subject to compliance review."
```

The `before:` chain on `invocation` is a more powerful variant: rather than inserting a new step adjacent to invocation, it attaches private pre-processing that can transform the user's prompt before it reaches the agent. See §5.7 for full documentation and the governance warnings that apply when using this in a `FROM` base pipeline.

### 4.4 Pipeline Run Log & Step Context

Every pipeline execution is backed by a **pipeline run log** — a durable, structured record written to disk before the next step begins. The log is the authoritative source for template variable resolution. An implementation that resolves template variables from an in-memory cache without a durable backing store does not conform to this spec.

#### Log Identity

Each pipeline run is identified by a `pipeline.run_id` — the same identifier used in the tracing and observability systems (see §22). There is no separate context identifier; run identity is unified across logging, tracing, and template variable access.

#### What Is Logged Per Step

Each step's log entry captures, at minimum:

| Field | Always present | Notes |
|---|---|---|
| `prompt` | Yes | The prompt sent to the LLM for this step |
| `response` | Yes | The final text output of the step |
| `tool_calls[]` | If any occurred | Name, input parameters, and result per call |
| `interim_calls[]` | If provider exposes them | Mid-step LLM calls where available |
| `provider` | Yes | Which provider handled this step |
| `session_id` | If provider reports it | Captured for session resumption. |
| `cost_usd` | If provider reports it | Token cost for this step |
| `duration_ms` | Yes | Wall clock time for the step |
| `condition_result` | If condition declared | Whether the step's condition evaluated true or false |
| `on_result_matched` | If `on_result` declared | Which branch fired |
| `error` | If step failed | Structured error detail: error_type, title, detail. Follows the RFC 9457-inspired AilError model defined in ARCHITECTURE.md. null on success. |

The storage format (SQLite, JSONL, binary, or other) is an implementation detail. What the spec owns is the schema above and the guarantee that it is persisted before the next step runs. Log location follows XDG conventions and is documented in `ARCHITECTURE.md`.

#### Accessing Prior Step Results

Any step may access the logged output of any previously completed step in the same pipeline run via template variables:

```
{{ step.invocation.prompt }}           — the original human prompt
{{ step.invocation.response }}         — the runner's response
{{ step.dry_refactor.response }}       — a named step's response
{{ step.dry_refactor.tool_calls }}     — a named step's tool calls (array)
{{ preceding_response }}               — the immediately preceding step's response
```

Variables resolve at step execution time from the persisted log, not from in-memory state. A reference to a step that has not yet run raises a fatal parse error. A reference to a step that was skipped by its condition raises a fatal parse error unless the referencing step has a matching condition guard, in which case it resolves to an empty string.

#### Provider Isolation

Steps running against different providers are isolated from each other by default. Each step calling a different provider receives only the context explicitly injected via template variables — there is no implicit cross-provider session sharing.

Steps running against the same provider also run in isolation by default — each step is a fresh invocation. Session continuity within the same provider must be explicitly requested via `resume: true` (see §15.4).

#### Sub-Pipeline Context Isolation

A called pipeline (via `pipeline:` step) owns its context in isolation. The caller has access only to the sub-pipeline's input, final response, and — where available — its top-level tool calls. The sub-pipeline's internal steps, intermediate responses, and local template variables are not visible to the caller.

---

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

## 6. Skills

A skill is a directory containing a `SKILL.md` file — natural language instructions that tell the model how to perform a specialised task. Skills follow the [Agent Skills open standard](https://agentskills.io), making any skill authored for Claude, Gemini CLI, GitHub Copilot, Cursor, or other compatible tools directly usable in `ail` without modification.

### 6.1 The Skill/Pipeline Distinction

| | Skill | Pipeline |
|---|---|---|
| **Format** | Markdown | YAML |
| **Read by** | The LLM | The `ail` runtime |
| **Contains** | Instructions, examples, guidelines | Control flow, sequencing, branching |
| **Scope** | How to think about a task | When to run it and what to do with the result |

### 6.2 Using a Skill in a Step

```yaml
# Local skill directory
- id: security_review
  skill: ./skills/security-reviewer/

# Parent directory skill
- id: org_review
  skill: ../org-skills/compliance-checker/

# Home directory skill
- id: personal_style
  skill: ~/skills/my-conventions/

# Built-in ail skill
- id: dry_check
  skill: ail/dry-refactor
```

### 6.3 Combining `skill:` and `prompt:`

A step may declare both. The skill provides standing instructions; the prompt provides the specific task for this invocation.

```yaml
- id: security_review
  skill: ./skills/security-reviewer/
  prompt: "{{ step.invocation.response }}"
  provider: frontier
  on_result:
    contains: "CLEAN"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security findings require human review."
```

When both are present, skill content is provided as system/instruction context and the prompt is the user-level task.

### 6.4 Agent Skills Compatibility

`ail`'s built-in modules (§14) are implemented as Agent Skills-compliant `SKILL.md` packages — inspectable, forkable, and overridable. Any skill from the broader Agent Skills ecosystem is usable in `ail` by path reference.

---

## 7. Pipeline Inheritance

### 7.1 `FROM`

A pipeline may inherit from another using `FROM`. The inheriting pipeline receives all parent steps and may modify them via hook operations.

```yaml
FROM: ./org-base.yaml
```

`FROM` accepts file paths only — relative, absolute, or home-relative. Remote URI support is a planned extension (see §22).

`FROM` chains are resolved at startup and must be **acyclic**. `ail` detects cycles at load time by tracking canonical resolved file paths — including symlink resolution. A cycle raises a fatal parse error with the full chain displayed at the point the cycle was detected:

```
Error: circular inheritance detected
  .ail.yaml → ./team-base.yaml → ./org-base.yaml → .ail.yaml
  ail cannot resolve a pipeline that inherits from itself.
```

The full resolved chain is inspectable via `ail materialize-chain` (§18).

### 7.2 Hook Operations

| Operation | Effect |
|---|---|
| `run_before: <id>` | Insert steps immediately before the named step. |
| `run_after: <id>` | Insert steps immediately after the named step. |
| `override: <id>` | Replace the named step. The override must not declare a different `id`. |
| `disable: <id>` | Remove the named step entirely. |

```yaml
FROM: ./org-base.yaml

pipeline:
  - run_before: security_audit
    id: license_header_check
    prompt: "Verify all modified files have the correct license header."

  - run_after: test_writer
    id: coverage_reminder
    prompt: "Does new test coverage meet the 80% threshold?"

  - override: dry_refactor
    prompt: "Refactor using conventions in CONTRIBUTING.md."

  - disable: commit_checkpoint
```

**Error conditions:**

All hook operations targeting a step ID that does not exist in the fully resolved inheritance chain raise a **fatal parse error**. This applies uniformly to all four operations — there is no best-effort hook insertion. The error message includes the list of valid step IDs in the resolved chain so the author can identify the correct target.

| Condition | Result |
|---|---|
| `disable:` targeting nonexistent ID | **parse error** |
| `override:` targeting nonexistent ID | **parse error** |
| `run_before:` targeting nonexistent ID | **parse error** |
| `run_after:` targeting nonexistent ID | **parse error** |
| `override:` declaring an `id` different from the step being overridden | **parse error** |
| Two hooks in the same file both targeting the same step ID with the same operation | **parse error** — use sequential steps instead |

Hook operations are validated against the **fully resolved chain**, not just the immediate parent. A hook targeting `security_audit` in a grandchild pipeline is valid as long as `security_audit` exists anywhere in the full `FROM` ancestry.

**Renaming a step ID in a `FROM`-able pipeline breaks all inheritors** — treat step IDs as a public API.

### 7.3 `FROM` and Pipeline Identity

Pipelines do not currently have a registry identity beyond their file path. When specifying `FROM`, use the file path directly. Pipeline registries, versioning, and remote URIs are planned extensions (§22).

---

## 8. Hook Ordering — The Onion Model

When multiple inheritance layers declare hooks targeting the same step ID, one rule governs execution order:

> **Hooks fire in discovery order, outermost first. The base pipeline's hooks are innermost — closest to the target step.**

### 8.1 Discovery Order (Most to Least Specific)

1. `--pipeline <path>` (CLI flag)
2. `.ail.yaml` (project root)
3. `.ail/default.yaml` (project fallback)
4. `~/.config/ail/default.yaml` (user default)
5. `FROM` base pipeline (and any `FROM` ancestors, outermost last)

### 8.2 Materialized Execution Order

```
[--pipeline  run_before: security_audit]    ← most specific, outermost
  [.ail.yaml run_before: security_audit]
    [~/.config run_before: security_audit]
      [FROM base run_before: security_audit] ← least specific, innermost
        security_audit                        ← the actual step
      [FROM base run_after: security_audit]
    [~/.config run_after: security_audit]
  [.ail.yaml run_after: security_audit]
[--pipeline  run_after: security_audit]     ← most specific, outermost
```

### 8.3 Governance Implication

An organisation's base pipeline guarantees its hooks fire immediately adjacent to the target step — regardless of what project layers add around the outside. The base pipeline governs what happens closest to the step itself.

---

## 9. Calling Pipelines as Steps

A pipeline may call another as a step using the `pipeline:` primary field.

### 9.1 Isolation Model

```
Caller pipeline context
  ↓ (full current context passed as input)
Called pipeline
  └─ invocation = caller's current context snapshot
  └─ runs its own steps in complete isolation
  └─ its own template variables are all local
  └─ returns its final step's output as a single response
  ↓
Caller receives {{ step.<call_id>.response }}
```

The caller sees only the called pipeline's final output. Internal steps, intermediate responses, and local context are not visible to the caller.

### 9.2 Syntax

```yaml
- id: run_security_suite
  pipeline: ./pipelines/security-suite.yaml
  on_result:
    contains: "ALL_CLEAR"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security suite found issues."
```

### 9.3 Failure Propagation

If the called pipeline aborts internally, `ail` surfaces a pipeline stack trace to the TUI — equivalent to a call stack — showing which pipeline failed, at which step, and why. The caller's `on_error` field governs what happens next. The full internal trace is written to the pipeline run log.

---

## 10. Named Pipelines & Composition

> **Status: Deferred — not in v0.1 scope.**

Multiple named pipelines within a single `.ail.yaml` file will be supported in a future version. The same composition is currently achievable by calling separate `.ail.yaml` files as `pipeline:` steps (§9), which is the recommended pattern until this feature is implemented.

The syntax described here is reserved and will be rejected by the current parser.

```yaml
# DEFERRED — not yet implemented
pipelines:
  default:
    - id: dry_check
      prompt: "Refactor for DRY principles."

  security_gates:
    - id: vuln_scan
      prompt: "Identify vulnerabilities."
```

---

## 11. Template Variables

Prompt strings and file-based prompts may reference runtime context using `{{ variable }}` syntax. Variables resolve at step execution time from the persisted pipeline run log, not from in-memory state.

| Variable | Value |
|---|---|
| `{{ step.invocation.prompt }}` | The input that triggered this pipeline run. |
| `{{ step.invocation.response }}` | The runner's response before any pipeline steps ran. |
| `{{ last_response }}` | The full response from the immediately preceding step. |
| `{{ step.<id>.response }}` | The response from a specific named step in this pipeline run. |
| `{{ step.<id>.tool_calls }}` | The tool calls made by a specific named step (array). |
| `{{ session.tool }}` | The underlying runner name (e.g. `aider`, `claude-code`). |
| `{{ session.cwd }}` | The current working directory of the session. |
| `{{ pipeline.run_id }}` | Unique ID for this pipeline execution. |
| `{{ env.VAR_NAME }}` | An environment variable. Fails loudly if not set — use this form for required variables. Use `{{ env.VAR_NAME \| default("value") }}` to fall back to a string literal. v0.1 supports string literals only. File and inline fallbacks for complex JSON or prompt content are a planned extension — for now, declare required variables explicitly and handle optional configuration via separate steps. |

> **Note:** There are no convenience aliases. All variable references use the dot-path structure above. This keeps the mental model consistent — every step, including `invocation`, is accessed the same way.

**Skipped step variables:** If a template variable references a step that was skipped by its `condition`, `ail` raises a **parse-time error** if the reference is unconditional, or returns an empty string if the referencing step itself has a matching condition guard. Silently empty references are never permitted.

**Future step variables:** Template variables may only reference steps that have already run. A reference to a step that has not yet executed at the point of resolution raises a fatal parse error.

---

## 12. Conditions

The `condition` field allows declarative skip logic. If false, the step is skipped and the pipeline continues.

### 12.1 Built-in Conditions

| Expression | Meaning |
|---|---|
| `if_code_changed` | True if the runner's response contains a code block. |
| `if_files_modified` | True if the runner modified files on disk. |
| `if_last_response_empty` | True if the previous step's response was blank. |
| `if_first_run` | True if this is the first pipeline run in this session. |
| `always` | Always true. Equivalent to omitting `condition`. |
| `never` | Always false. Identical to `disabled: true`. |

### 12.2 Expression Syntax

> **Status: Deferred — not in v0.1 scope.**

A general condition expression language — supporting dot-path comparisons, step response checks, and logical operators — is planned for a future version. The named conditions in §12.1 cover the common cases and are the only supported form in the current implementation.

```yaml
# DEFERRED — not yet implemented
condition: "context.file_count > 0"
condition: "step.security_audit.response contains 'VULNERABILITY'"
condition: "if_code_changed and not if_first_run"
```

The named conditions (`if_code_changed`, `if_files_modified`, etc.) are fully supported and are the recommended approach.

---

## 13. Human-in-the-Loop (HITL) Gates

HITL gates are intentional checkpoints, not error states.

### 13.1 Explicit HITL Step

```yaml
- id: approve_before_deploy
  action: pause_for_human
  message: "Pipeline complete. Approve to continue."
  timeout_seconds: 3600
  on_timeout: abort_pipeline
```

### 13.2 HITL Responses

| Response | Effect |
|---|---|
| **Approve** | Gate clears. Pipeline continues unchanged. |
| **Reject** | Pipeline aborts. Reason logged to pipeline run log. |
| **Modify** | Human edits the step output or tool input before execution resumes. |
| **Allow for session** | Tool is added to the in-memory session allowlist. Subsequent identical tool calls in this session are auto-approved silently. |

### 13.3 Tool Permission HITL

When a pipeline step invokes a tool not covered by `tools.allow` or `tools.deny` (see §5.6), `ail` intercepts the permission callback via `--permission-prompt-tool stdio` and presents it to the human before the tool executes.

`ail` reads permission request events from the NDJSON stream, renders them in the TUI, and writes a JSON response back to Claude CLI's stdin. The full set of valid responses is:

```json
// Allow this tool call once
{ "behavior": "allow" }

// Allow with modified tool input (the Modify response)
{ "behavior": "allow", "updatedInput": { ...corrected parameters... } }

// Deny this tool call
{ "behavior": "deny", "message": "User rejected this action" }
```

**`updatedInput`** is how the **Modify** HITL response is implemented. The human edits the tool's input parameters in `ail`'s TUI — correcting a file path, adjusting a command, removing a sensitive value — before allowing execution to proceed with the corrected values.

**Allow for session** is managed entirely in `ail`'s session state, not in the Claude CLI. When the user selects this option, `ail` records the tool name and input pattern in an in-memory allowlist. For the remainder of the session, matching permission requests receive an automatic `{"behavior": "allow"}` without prompting.

#### Permission Mode

`ail` launches Claude CLI in `default` permission mode unless configured otherwise. The supported modes — `default`, `accept_edits`, `plan`, `bypass_permissions` — map to `--permission-mode` flag values. This may be exposed as a session-level option in a future version. For headless runs, `--dangerously-skip-permissions` (or `bypass_permissions` mode) is the correct approach.

> **Implementation note:** The `--permission-prompt-tool stdio` behaviour when combined with `-p` (non-interactive prompt mode) must be validated in the v0.0.1 spike. The VSCode extension uses this mechanism in interactive mode; `ail`'s non-interactive usage pattern may differ. See `RUNNER-SPEC.md`.

### 13.4 Tool Permission Flow

```
Claude CLI emits tool_use event
  ↓
Is tool in step's tools.allow?
  YES → { "behavior": "allow" } — silent, no prompt
  ↓ NO
Is tool in step's tools.deny?
  YES → { "behavior": "deny" } — silent, no prompt
  ↓ NO
Is tool in session allowlist?
  YES → { "behavior": "allow" } — silent, no prompt
  ↓ NO
Present HITL prompt to human
  → Approve      → { "behavior": "allow" }
  → Allow for session → { "behavior": "allow" } + add to session allowlist
  → Modify       → { "behavior": "allow", "updatedInput": <edited> }
  → Reject       → { "behavior": "deny", "message": "User rejected" }
```

### 13.5 Implicit HITL via `on_result`

Preferred over explicit gates — interrupts only when something genuinely requires attention. See §5.3.

### 13.6 Headless / Automated Mode

For automated runs (CI, the autonomous agent use case, Docker sandbox), HITL prompts are not viable. Pass `--dangerously-skip-permissions` to the Claude CLI invocation to bypass all tool permission checks. This is only appropriate in a fully trusted, sandboxed environment. `ail` will expose this as a session-level flag — not a pipeline YAML option — to prevent it from being accidentally committed to a shared pipeline file.

---

## 14. Built-in Modules

`ail`'s built-in modules are referenceable via `skill: ail/<name>`. Each is implemented as an Agent Skills-compliant `SKILL.md` package — inspectable, forkable, and overridable.

| Module | Description |
|---|---|
| `ail/janitor` | Context distillation. Compresses working context to reduce token usage. |
| `ail/dry-refactor` | Refactors code for DRY compliance. |
| `ail/security-audit` | Security-focused review. Pauses for human if findings exist. |
| `ail/test-writer` | Generates unit tests for new functions in the preceding response. |
| `ail/model-compare` | Runs the same prompt against two providers. Presents outputs side by side. |
| `ail/commit-checkpoint` | Prompts user to commit current changes before pipeline continues. |

```yaml
pipeline:
  - id: distill
    skill: ail/janitor

  - id: security
    skill: ail/security-audit
    on_result:
      contains: "VULNERABILITY"
      if_true:
        action: abort_pipeline
```

> **Note:** Skill parameterisation (`with:` or equivalent) is deferred. How a `SKILL.md` declares and receives parameters is an open question that will be resolved alongside structured output schema research. See §23.

---

## 15. Providers

### 15.1 Provider String Format

```yaml
provider: openai/gpt-4o
provider: anthropic/claude-opus-4-5
provider: groq/llama-3.1-70b-versatile
provider: cerebras/llama-3.3-70b
provider: fast       # named alias
provider: frontier   # named alias
```

### 15.2 Provider Aliases

Defined in `~/.config/ail/providers.yaml` or in a `providers` block in the pipeline file.

```yaml
providers:
  fast:     groq/llama-3.1-70b-versatile
  balanced: openai/gpt-4o-mini
  frontier: anthropic/claude-opus-4-5

defaults:
  provider: balanced
```

### 15.3 Credentials

Provider API keys are read from environment variables. `ail` never stores credentials. The expected environment variable names follow each provider's standard conventions (e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`). See your provider's documentation.

### 15.4 Session Resumption — `resume:`

By default, each pipeline step is a fresh, isolated invocation against its provider. Steps on different providers are always isolated from each other. Steps on the same provider are also isolated by default — no implicit session continuity is assumed.

When a step declares `resume: true`, `ail` uses the session ID from the most recent preceding step on the same provider within this pipeline run and passes it to the current step's invocation. This requests session continuity at the provider level — the LLM receives the conversation history from the prior step.

```yaml
- id: security_audit
  provider: anthropic/claude-opus-4-5
  resume: true
  prompt: "Now review the refactored code for security vulnerabilities."
```

**Scoping rule:** `resume: true` resumes the session from the **most recent preceding step on the same provider** in this pipeline run. If no preceding step on the same provider exists, the step behaves as a fresh session and a warning is emitted.

**Provider capability:** Session resumption depends on the provider supporting it. `resume: true` on a runner or provider that does not support session resumption raises a warning at parse time and falls back to an isolated invocation. Whether and how session resumption is implemented is defined in `RUNNER-SPEC.md`.

**Session ID capture:** `ail` captures the session ID returned by a provider invocation whenever the provider makes one available, and writes it to the pipeline run log.  This happens regardless of whether `resume: true` is declared — session IDs are always logged when present. `resume: true` consumes the most recently logged session ID for the same provider.

---

## 16. Error Handling & Resilience

| Value | Effect |
|---|---|
| `continue` | Log error, proceed. Only for explicitly non-critical steps. |
| `pause_for_human` | Suspend pipeline, surface error in HITL panel. **Default.** |
| `abort_pipeline` | Stop immediately. Log full error context to pipeline run log. |
| `retry` | Retry up to `max_retries` times, then escalate to `pause_for_human`. |

```yaml
defaults:
  on_error: pause_for_human

pipeline:
  - id: optional_style_check
    on_error: continue
    prompt: "Check for style guide violations."

  - id: critical_security_scan
    on_error: abort_pipeline
    max_retries: 3
    prompt: "Scan for security vulnerabilities."
```

---

## 17. The `materialize-chain` Command

Resolves the full inheritance chain and writes the complete flattened pipeline to disk — steps in exact execution order, with origin comments.

```bash
ail materialize-chain
ail materialize-chain --out materialized.yaml
ail materialize-chain --pipeline ./deploy.yaml --out materialized.yaml
ail materialize-chain --expand-pipelines   # recurse into pipeline: steps
```

**Example output:**

```yaml
# materialized.yaml — generated by `ail materialize-chain`
# Source chain:
#   [1] --pipeline ./deploy.yaml
#   [2] .ail.yaml
#   [3] ~/.config/ail/default.yaml
#   [4] FROM: ./org-base.yaml

version: "0.1"

pipeline:

  # step: invocation (implicit)

  # origin: [1] deploy.yaml  (run_before: security_audit — outer shell)
  - id: deploy_pre_check
    prompt: "..."

  # origin: [4] org-base.yaml  (run_before: security_audit — inner shell)
  - id: org_context_loader
    prompt: "..."

  # origin: [4] org-base.yaml
  - id: security_audit
    prompt: "..."

  # origin: [4] org-base.yaml  (run_after: security_audit — inner shell)
  - id: org_audit_logger
    prompt: "..."

  # origin: [1] deploy.yaml  (run_after: security_audit — outer shell)
  - id: deploy_post_check
    prompt: "..."
```

`materialize-chain` is the primary debugging tool. Reach for it first when a pipeline behaves unexpectedly. It also emits warnings for governance-relevant configurations — including `before:` chains on `invocation` — so human operators can inspect what they've inherited before running a session.

---

## 18. Complete Examples

### 18.1 The Simplest Possible Pipeline

```yaml
version: "0.1"

pipeline:
  - id: review
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

### 18.2 Solo Developer Quality Loop

```yaml
version: "0.1"

meta:
  name: "Personal Quality Gates"

defaults:
  provider: openai/gpt-4o-mini
  on_error: pause_for_human

pipeline:
  - id: dry_refactor
    condition: if_code_changed
    prompt: ./prompts/dry-refactor.md

  - id: test_writer
    condition: if_code_changed
    prompt: ./prompts/test-writer.md
```

### 18.3 Session Setup with `run_before: invocation`

*Check a local cache for an architecture description before the first prompt. If not found, ask the human whether to generate one. Teaches `run_before: invocation` and the pipeline-as-step pattern together.*

```yaml
version: "0.1"

meta:
  name: "Architecture-Aware Session"

pipeline:
  - run_before: invocation
    id: load_architecture_context
    pipeline: ./pipelines/load-or-generate-architecture.yaml

  - id: dry_refactor
    condition: if_code_changed
    skill: ail/dry-refactor

  - id: security_audit
    condition: if_code_changed
    skill: ail/security-audit
```

`load-or-generate-architecture.yaml` might check for a cached `.ail/architecture.md`, load it into context if present, or `pause_for_human` offering to run an architecture exploration if not.

### 18.4 Org Base Pipeline

```yaml
version: "0.1"

meta:
  name: "ACME Corp Engineering Standards"

providers:
  fast:     groq/llama-3.1-70b-versatile
  frontier: anthropic/claude-opus-4-5

defaults:
  provider: fast
  on_error: abort_pipeline

pipeline:
  - id: dry_refactor
    condition: if_code_changed
    skill: ail/dry-refactor

  - id: security_audit
    provider: frontier
    condition: if_code_changed
    skill: ail/security-audit
    prompt: ./prompts/acme-security-context.md
    on_result:
      contains: "SECURITY_CLEAN"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Security findings require review before this code proceeds."
```

### 18.5 Project Inheriting from Org Base

```yaml
version: "0.1"

FROM: /etc/ail/acme-base.yaml

meta:
  name: "Payments Team — Project Phoenix"

pipeline:
  - run_before: security_audit
    id: pci_compliance_check
    provider: frontier
    skill: ./skills/pci-checker/
    on_result:
      contains: "COMPLIANT"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "PCI compliance issue requires security team review."

  - disable: commit_checkpoint
```

### 18.6 LLM Researcher Model Comparison

```yaml
version: "0.1"

pipeline:
  - id: compare
    skill: ail/model-compare
    on_result:
      always:
        action: pause_for_human
        message: "Comparison complete. Review outputs above."
```

### 18.7 Multi-Speed Pipeline

```yaml
version: "0.1"

providers:
  fast:     groq/llama-3.1-70b-versatile
  frontier: anthropic/claude-opus-4-5

pipeline:
  - id: syntax_check
    provider: fast
    prompt: |
      Is the code above syntactically valid and free of obvious runtime errors?
      Answer VALID or list the issues. Be terse.
    on_result:
      contains: "VALID"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Syntax issues found before deep review."

  - id: architecture_review
    provider: frontier
    condition: if_code_changed
    prompt: ./prompts/architectural-review.md
```

---

## 19. Runners & Adapters

> **Note:** This section describes the conceptual model for how `ail` connects to underlying CLI tools. The detailed contract for runner compliance is defined in a separate document — `RUNNER-SPEC.md` — which is currently a stub under active development. The interface described here should be considered directional, not final.

### What a Runner Is

A runner is the underlying CLI agent that `ail` wraps. It receives the human's prompt, produces a response, and signals completion. `ail` orchestrates everything that happens after that signal fires.

The runner is deliberately outside the pipeline language. `SPEC.md` defines what pipelines do. The runner is what the pipeline acts upon.

### Three Tiers of Runner Support

**Tier 1 — First-class runners**
Built-in adapters shipped with `ail` and maintained by the core team. Tested against every `ail` release. The runner's behaviour, output format, completion signal, and error codes are fully understood and handled.

Initial first-class runner: **Claude CLI** (`claude`). The v0.0.1 proof of concept targets Claude exclusively.

Roadmap for first-class support (not yet committed): Aider, OpenCode, Codex CLI, Gemini CLI, Qwen CLI, DeepSeek CLI, llama.cpp.

**Tier 2 — AIL-compliant runners**
Any CLI tool that implements the AIL Runner Contract defined in `RUNNER-SPEC.md`. A compliant runner works with `ail`'s built-in generic adapter without requiring a custom implementation. The tool author reads `RUNNER-SPEC.md` and ships their CLI accordingly. `ail` makes no guarantees about compliant runners beyond what the contract specifies.

**Tier 3 — Custom adapters**
Any CLI tool that does not implement the runner contract can be wrapped in a community-written or private adapter. Adapters implement the `Runner` trait defined in `ail`'s Rust core and are loaded at runtime as dynamic libraries. See `ARCHITECTURE.md` *(forthcoming)* for the trait interface and dynamic loading system.

### Runner Configuration

The active runner is declared in the pipeline file or in `~/.config/ail/config.yaml`:

```yaml
# In .ail.yaml
runner:
  id: claude
  command: claude
  args: ["--print"]         # invocation flags; runner-specific

# Or reference a custom adapter
runner:
  id: my-custom-runner
  adapter: ~/.ail/adapters/my-runner.so
```

If no runner is declared, `ail` defaults to the Claude CLI.

### The AIL Runner Contract (Summary)

The full contract is defined in `RUNNER-SPEC.md`. At a high level, a compliant runner must:

- Accept a prompt via a flag or stdin in non-interactive mode
- Write its response to stdout
- Exit with code `0` on success, non-zero on error
- Optionally declare supported capabilities (structured output, extended thinking, tool calls, session resumption) via a `--ail-capabilities` flag

Runners that implement the optional capability declarations unlock richer `ail` features — structured step output, thinking traces, tool call inspection, and `resume: true` support. Runners that implement only the minimum contract work with Tier 1 text-based pipeline features.

> **Note:** Session continuity behaviour — what "isolated" means per runner, and how session IDs are captured and passed for `resume: true` — is defined in `RUNNER-SPEC.md`, not here. The pipeline language declares intent; the runner contract defines mechanics.

### Further Reading

- `RUNNER-SPEC.md` — The AIL Runner Contract. Read this if you are a CLI tool author who wants first-class `ail` compatibility.
- `ARCHITECTURE.md` *(forthcoming)* — The Rust trait interface and dynamic loading system. Read this if you are writing a custom runner adapter.

---

## 20. MVP — v0.0.1 Scope

The goal of v0.0.1 is a working demo: one pipeline, one runner, one follow-up prompt, visibly running. Nothing more. This is the proof of concept that validates the core guarantee before any additional complexity is added.

**In scope for v0.0.1:**

| Feature | Notes |
|---|---|
| Single pipeline file (`.ail.yaml`) | No inheritance, no `FROM` |
| `pipeline:` array with ordered steps | Top-to-bottom execution only |
| `prompt:` field — inline string only | No file path resolution yet |
| `id:` field | Required for all steps |
| `provider:` field | At least one working provider |
| `on_result: contains` + `continue` / `pause_for_human` / `abort_pipeline` | Minimal branching |
| `condition: always` and `condition: never` | Trivial conditions — proves the condition system exists |
| `{{ step.invocation.response }}` and `{{ last_response }}` | Core template variables |
| Passthrough mode when no `.ail.yaml` found | Safe default |
| `ail materialize-chain` | Flattens a single-file pipeline — no inheritance to traverse yet, but establishes the command |
| Basic TUI — streaming stdout passthrough | Human can see the runner working |
| Pipeline run log — persisted to disk | Step responses durable before next step |
| Completion detection via process exit code 0 | For CLI runner steps |

**Explicitly out of scope for v0.0.1:**

- `FROM` inheritance and all hook operations
- `skill:` field
- `pipeline:` field (calling sub-pipelines)
- `action: pause_for_human` (HITL gates)
- `condition:` expressions beyond `always` / `never`
- File path resolution for `prompt:`
- `defaults:` block
- `resume:` field
- Multiple named pipelines
- All built-in modules
- Everything in §22 Planned Extensions

**The v0.0.1 demo case:**

```yaml
version: "0.1"

pipeline:
  - id: dont_be_stupid
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

One file. One step. Always runs. Ships.

---

## 21. Planned Extensions

These features are designed and their syntax is reserved. Not yet implemented. Do not use in production pipelines — the current parser will reject them.

> **For contributors:** Open an issue referencing this section before beginning implementation work.

---

### Structured Step I/O Schemas

> **Status: Planned — v0.1 target**

#### Motivation

The `on_result` prose-matching operators (`contains`, `matches`, `starts_with`) match against free-form LLM text. LLMs are not deterministic — a step instructed to respond `CLEAN` may produce `CLEAN.`, `Yes, CLEAN`, or variations that fail the match. Prose-based branching is documented as best-effort and unsuitable for reliable control flow.

Structured I/O schemas provide a reliable alternative: a step declares what structured output it expects to receive, the runtime validates it before the step runs, and `on_result` can branch on known fields with exact equality.

#### `input_schema`

A step may declare an `input_schema` — a JSON Schema describing the structure it expects from the preceding step's output. The runtime validates the preceding step's response against this schema before the step executes. A validation failure is treated as a step error and escalates via `on_error`.

```yaml
- id: security_audit
  skill: ail/security-audit
  input_schema:
    type: object
    properties:
      result:
        type: string
        enum: ["CLEAN", "VULNERABILITIES_FOUND"]
      findings:
        type: array
        items:
          type: string
    required: [result]
  on_result:
    field: result
    equals: "CLEAN"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security findings require review."
```

#### `output_schema`

A step may declare an `output_schema` — a JSON Schema describing the structure it promises to produce. The runtime validates the step's response against this schema after execution. A validation failure is treated as a step error and escalates via `on_error`.

`output_schema` is optional and independent of `input_schema`. Both may be declared on the same step.

#### Parse-time compatibility

When a step declares `input_schema` and the immediately preceding step declares `output_schema`, the runtime validates schema compatibility at **parse time** — before execution begins. A mismatch between declared output and declared input is a parse error, not a runtime error.

#### The `field:` + `equals:` operator

When a step declares `input_schema`, `on_result` gains access to the `field:` + `equals:` operator — exact equality matching against a named field in the validated input:

```yaml
on_result:
  field: result
  equals: "CLEAN"
  if_true:
    action: continue
  if_false:
    action: pause_for_human
```

**Parse-time rule:** A step that declares `on_result` using `field:` + `equals:` **must** also declare an `input_schema` that includes the referenced field. This is enforced at parse time.

#### Relationship to the expression language

This feature deliberately stops short of a general expression language. Field access and equality matching on validated JSON covers the reliable branching use cases without reopening the full expression language question. If more complex conditions prove necessary in practice, the condition expression language (§12.2) is the correct extension point.

#### Provider compatibility

`output_schema` may optionally be passed to the provider as a structured output constraint — using the provider's native JSON mode or schema enforcement — where supported. Providers that do not support native schema enforcement will have their output validated by `ail` after receipt. A validation failure is treated as a step error.

---

### Parallel Step Execution

> **Status: Planned**

```yaml
- id: parallel_review
  parallel:
    - id: security
      skill: ./skills/security-reviewer/
    - id: dry_check
      skill: ail/dry-refactor
```

---

### Fan-Out / Fan-In with Synthesis

> **Status: Planned**

```yaml
- id: full_review
  parallel:
    - id: security
      skill: ./skills/security-reviewer/
    - id: dry
      skill: ail/dry-refactor
  synthesize:
    prompt: |
      Security: {{ step.full_review.security.response }}
      DRY: {{ step.full_review.dry.response }}
      Produce a single consolidated report.
```

---

### Remote `FROM` Targets

> **Status: Planned**

Pipeline URIs, versioning, and a registry system analogous to Docker Hub. `FROM` currently accepts file paths only. URI support will be designed alongside tagging and version pinning.

---

### Step Output Visibility Control

> **Status: Planned — pending streaming research**

Per-step control over what the TUI displays: full streaming, final response only, or silent (pipeline-internal steps).

```yaml
- id: quality_score
  prompt: "Rate code quality 0.0–1.0. Respond with the number only."
  display: silent
```

---

### Dry Run Mode

> **Status: Planned**

Renders all prompts with live template variable values without making LLM calls.

```bash
ail --dry-run
```

---

### Direct MCP Tool Invocation

> **Status: Exploratory** — The concept and value are clear. The design has unresolved questions. Syntax is not yet reserved.

MCP (Model Context Protocol) is an open standard for connecting LLMs to external tools and data sources. An MCP server exposes named tools — functions that can be called with structured arguments to read data, query systems, or perform actions — and returns structured results.

There are two ways MCP can interact with `ail`:

**Mode 1 — LLM-driven (already implied by the step model)**  
During a pipeline step that calls an LLM directly, the LLM may emit tool calls targeting an MCP server. `ail` acts as the MCP client, executes the tool, and returns the result to the LLM. This is the standard MCP use case and requires no new pipeline syntax — it is a provider/runtime concern.

**Mode 2 — Pipeline-driven (this extension)**  
A pipeline step calls an MCP tool directly, bypassing the LLM entirely. The tool result flows forward as the step's response, available to subsequent steps via `{{ step.<id>.response }}`. No tokens are consumed.

```yaml
# Proposed syntax — not final
- id: get_repo_structure
  mcp:
    server: filesystem
    tool: list_directory
    arguments:
      path: "{{ session.cwd }}"

- id: analyse_structure
  prompt: |
    Here is the repository structure:
    {{ step.get_repo_structure.response }}
    Identify any architectural concerns.
```

This fits the Unix pipe philosophy already established in the spec: a deterministic, zero-token step that gathers data and passes it downstream.

**Unresolved questions before this can be specced:**

- How does `ail` discover and connect to MCP servers? Via a config block, a running process, or a URI?
- How are MCP servers declared — per-pipeline, per-session, or globally in `~/.config/ail/`?
- How is authentication handled for MCP servers that require credentials?
- If an MCP tool call fails, how does it interact with `on_error`? MCP errors are structured — can `on_result` match against them?
- Is `mcp:` a primary field (peer to `prompt:`, `skill:`, `pipeline:`) or a sub-field of `action:`?
- Should the pipeline be able to *register* MCP tools that the LLM can use in subsequent steps, or only call them directly?

**Why this matters:**  
Direct MCP invocation makes `ail` pipelines genuinely composable with the broader MCP ecosystem. A pipeline could gather live data from any MCP-compatible source — filesystem, database, web search, calendar, code analysis tools — before passing it to an LLM step, without burning tokens on the retrieval itself. This is particularly valuable for research pipelines and compliance workflows where the data gathering step must be deterministic and auditable.

---

### Native LLM Provider Support (OpenAI-Compatible REST)

> **Status: Planned — no CLI runner required**

A native runner tier that calls OpenAI-compatible `/v1/chat/completions` REST endpoints directly, without wrapping a CLI tool. Enables `ail` pipelines to run against any model host that exposes the OpenAI-compatible API — Ollama, Together AI, Groq, LiteLLM, corporate LLM proxies, and Anthropic's compatibility layer — without requiring a CLI agent to be installed.

```yaml
# Proposed syntax — not final
providers:
  local_ollama:
    type: openai-compatible
    base_url: http://localhost:11434/v1
    model: llama3.2
  hosted_groq:
    type: openai-compatible
    base_url: https://api.groq.com/openai/v1
    api_key_env: GROQ_API_KEY
    model: mixtral-8x7b-32768

pipeline:
  - id: fast_triage
    prompt: "Is this a security issue? Answer yes or no."
    provider: local_ollama
```

When a step uses a native REST provider, `ail` acts as the agent for that turn — managing conversation history, tool call dispatch, streaming reassembly, and context window state. This is a significant responsibility boundary shift from the CLI runner model (where the runner manages these concerns) and is called out explicitly in ARCHITECTURE.md §8.

The pipeline language syntax for `providers:` is forward-compatible with this extension. The `type:` field is reserved for this purpose and currently has no effect.

**Prerequisite:** native runner support depends on the `ail serve` server mode (see below) being designed first, as the two share the same HTTP client infrastructure and auth model.

---

### Safety Guardrails

> **Status: Exploratory** — The structural pattern is clear. Several design questions remain open, and one fundamental limitation must be stated plainly. Syntax is not yet reserved.

#### The Two Layers

Safety in `ail` operates across two distinct layers with very different reliability guarantees:

**Layer 1 — What `ail` can enforce deterministically**
Constraints on the pipeline itself: what gets injected into prompts, which skills and pipelines are permitted, which configurations are blocked at parse time. These are runtime guarantees — `ail` either allows the pipeline to run or it does not.

**Layer 2 — What depends on the underlying model**
Whether a model actually follows an injected instruction is a model concern, not a runtime concern. `ail` can inject "never include credentials in your response" into every prompt, but it cannot guarantee the model obeys it. For hard safety requirements, the reliable pattern is a dedicated validation step after any step that might produce sensitive content — using `input_schema` + `field:` + `equals:` for deterministic branching. A pipeline step is deterministic in a way that a model instruction is not.

This distinction must be understood by anyone relying on `ail` for safety-critical workflows.

#### Proposed Structure

Safety resources follow the same pattern as `observability:` — declared as a top-level block, inheritable via `FROM`, with org-declared directives carrying a `required: true` flag that child pipelines cannot override or remove.

```yaml
safety:

  # Injected into every prompt in this pipeline, unconditionally
  directives:
    - id: no_credentials
      inject: "Never include credentials, API keys, or passwords in your response."
      position: prepend         # or: append
      required: true            # child pipelines cannot disable this directive

    - id: pii_reminder
      inject: "Do not reproduce personally identifiable information."
      position: prepend
      required: true

  # Step configurations rejected at parse time
  blocklist:
    - pattern: "disable.*hitl"
      message: "HITL gates cannot be disabled by policy."

  # Only these skill and pipeline paths are permitted
  skill_allowlist:
    - ail/*
    - ./skills/*
    - /etc/ail/approved-skills/*

  pipeline_allowlist:
    - ./pipelines/*
    - /etc/ail/approved-pipelines/*
```

#### Inheritance via `FROM`

Safety directives declared in a `FROM` base pipeline are inherited by all child pipelines. A child may add its own directives but cannot remove or override a directive marked `required: true`. Any attempt to do so is a parse error, not a silent failure.

This makes safety directives a governance guarantee at the org level — the same model used for observability compliance.

#### The Reliable Safety Pattern

For any output that must be validated — not just instructed — a dedicated validation step provides stronger guarantees than a model directive alone. Where deterministic branching is required, `input_schema` + `field:` + `equals:` is the recommended approach:

```yaml
pipeline:
  - id: generate_code
    prompt: ./prompts/generate.md

  - id: safety_check
    prompt: |
      Review the above output strictly for the following:
      - No credentials, tokens, or secrets
      - No personally identifiable information
      - No hardcoded environment-specific values
      Answer only with SAFE or UNSAFE.
    input_schema:
      type: object
      properties:
        result:
          type: string
          enum: ["SAFE", "UNSAFE"]
      required: [result]
    on_result:
      field: result
      equals: "SAFE"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Safety violation detected. Review required before proceeding."
```

This is deterministic — the pipeline either continues or it does not — in a way that prompt injection alone cannot be.

#### Unresolved Questions

- Are `blocklist` patterns evaluated at parse time (static analysis of the YAML) or at runtime (when a step is about to execute)? Parse time is safer but may not catch dynamically constructed step configurations.
- How does the `skill_allowlist` interact with built-in `ail/*` modules? They should probably be implicitly permitted unless explicitly removed.
- Should `required: true` directives be visible to child pipeline authors — i.e. should `materialize-chain` show injected directive content — or should they be opaque to prevent circumvention?
- Can a directive declare which step types it applies to (e.g. only `prompt:` steps, not `action:` steps), or does it apply universally?
- How does sensitive directive content interact with the observability layer? A safety directive that contains policy language may itself be sensitive and should not appear in trace exports by default.

---

### Model Benchmarking

> **Status: Exploratory** — The building blocks exist in the current spec. A dedicated benchmarking execution model requires design work that goes beyond the current single-run pipeline model. Syntax is not yet reserved.

#### What the Current Spec Already Supports

The `ail/model-compare` built-in and multi-provider routing cover the simplest benchmarking case — same prompt, two models, side-by-side output reviewed by a human. For casual model comparison this is sufficient and available today.

#### What a Real Benchmarking Workflow Needs

A serious benchmarking workflow requires capabilities the current spec does not express:

- **Dataset input** — run against a controlled set of prompts, not just a single human input
- **Repeatability** — run the same prompt N times against the same model to measure variance
- **Structured scoring** — a quality score per run captured as data, not just human review
- **Aggregation** — collect results across all runs into a comparable report
- **Isolation** — benchmark runs must not share context with production pipeline runs or with each other

None of these fit the current execution model, which is designed around a single `invocation` triggering a single pipeline run. Benchmarking is fundamentally a multi-run, dataset-driven execution model.

#### Proposed Direction

Benchmarking is a strong candidate for the plugin extensibility layer (see below). Rather than adding `benchmark:` as a core spec keyword, it would be expressed as an `x-benchmark:` extension that a benchmarking plugin handles:

```yaml
# Proposed — not final syntax
x-benchmark:
  dataset: ./benchmarks/security-prompts.jsonl
  runs_per_prompt: 5
  models:
    - openai/gpt-4o
    - anthropic/claude-opus-4-5
    - groq/llama-3.1-70b-versatile
  scoring:
    skill: ./skills/quality-scorer/
    output_schema: ./schemas/benchmark-score.json
  report:
    format: markdown
    destination: ./reports/{{ benchmark.run_id }}.md
```

The benchmark plugin would manage the multi-run execution loop, invoke the pipeline's steps for each input, collect structured scores, and produce a report — all without the core runtime needing to know about datasets or aggregation.

#### Why This Matters as a Vertical

For the LLM researcher segment, benchmarking is table-stakes. The ability to run a controlled dataset through multiple models, score the outputs with a custom skill, and produce a comparable report — entirely from a YAML file — is a genuinely novel capability that no current tool provides cleanly. It is also a natural lead-in to the learning loop use case: a benchmark run that identifies which model performs best on a given task type can feed directly into routing decisions.

#### Unresolved Questions

- Is a `trigger: dataset` the right execution model, or should benchmarking be entirely plugin-managed outside the normal pipeline trigger system?
- How does context isolation work between benchmark runs? Each run must be completely independent.
- Should scoring be a skill (LLM-evaluated) or a deterministic function (regex, JSON schema validation, exact match)? Both are useful; the spec should support both.
- How are benchmark results stored and compared across runs over time — is this a persistence concern for the `ail` runtime, or for the plugin?

---

### Plugin Extensibility Layer

> **Status: Exploratory** — The `x-` prefix model is the leading candidate. Core runtime changes are minimal. Full plugin dispatch mechanism requires design. Syntax partially reserved (`x-` prefix).

#### Motivation

As `ail` grows, vertical use cases will emerge that are too specific to belong in the core spec — benchmarking, Datadog integration, custom compliance frameworks, team-specific reporting. Adding each as a core keyword pollutes the spec and creates a maintenance burden. A plugin extensibility layer allows third parties to extend the YAML language itself, adding new top-level keywords and new step fields, without forking the spec or the runtime.

#### The Leading Candidate: `x-` Prefix Model

Following the Docker Compose convention, any top-level key or step field prefixed with `x-` is reserved for extensions. The core runtime passes `x-` fields to registered plugin handlers. If no handler is registered for a given `x-` field, it is either silently ignored or raises a warning — configurable by policy.

```yaml
# Third-party plugins extend the top-level namespace
x-datadog:
  api_key: "{{ env.DD_API_KEY }}"
  service: "ail-pipelines"
  env: production

x-benchmark:
  dataset: ./benchmarks/prompts.jsonl
  runs_per_prompt: 3

# Plugins can also add step-level fields
pipeline:
  - id: security_audit
    prompt: ./prompts/security.md
    x-notify:
      channel: "#security-alerts"
      on: pause_for_human
```

#### Plugin Registration

Plugins are declared in `~/.config/ail/plugins.yaml` or in a `plugins:` block in the pipeline file:

```yaml
# Proposed — not final syntax
plugins:
  - id: datadog
    path: ~/.ail/plugins/datadog/
    handles: [x-datadog]

  - id: benchmark
    path: ~/.ail/plugins/benchmark/
    handles: [x-benchmark]
    trigger: manual            # benchmark plugin registers its own trigger type
```

A plugin is itself an Agent Skills-compatible directory — a `PLUGIN.md` file declaring its capabilities, accepted `x-` fields, and handler entry point. This keeps the plugin format consistent with the skill format already established in the spec.

#### What a Plugin Can Do

A plugin handler receives the full parsed pipeline and the values of its declared `x-` fields. It may:

- Add steps to the pipeline before execution begins
- Register new trigger types
- Subscribe to step lifecycle events (before step, after step, on error)
- Write to the audit trail
- Export to external systems

A plugin may not modify core pipeline behaviour — step execution order, `on_result` logic, HITL gate behaviour — without those modifications being visible in `materialize-chain` output.

#### Governance and Safety Interaction

Plugins declared in a `FROM` base pipeline are inherited by child pipelines. A base pipeline may declare a plugin as `required: true`, preventing child pipelines from removing it — the same governance model used for safety directives and observability resources.

```yaml
plugins:
  - id: compliance-reporter
    path: /etc/ail/plugins/compliance-reporter/
    handles: [x-compliance]
    required: true             # child pipelines cannot remove this plugin
```

#### Unresolved Questions

- Should the plugin entry point be a compiled binary, a script, or a WASM module? Each has different portability and security implications. WASM is the most sandboxed but has the highest implementation cost.
- How does `materialize-chain` represent plugin-injected steps? They must be visible to maintain the "no surprises" guarantee.
- Can a plugin declare new `on_result` action types, or are those reserved for the core spec?
- Should plugins be sandboxed from the filesystem and network by default, with explicit capability grants? Given that plugins run as part of a pipeline that may handle sensitive data, this seems important but adds implementation complexity.
- Is `PLUGIN.md` the right format, or should plugins have a more structured manifest (JSON schema, TOML) given that they declare machine-readable capabilities rather than human-readable instructions?

---

### Pipeline Registry & Versioning

> **Status: Planned**

Named pipeline identity, versioning, and a registry system. Enables `FROM: org/security-base@2.1` style references. Will be designed alongside remote `FROM` support.

---

### Observability — Tracing & Logging

> **Status: Exploratory** — The structure and value are clear. Several design questions remain open. Syntax is not yet reserved.

#### Motivation

`ail` pipelines are multi-step, multi-provider, potentially multi-pipeline executions. Without structured observability, debugging a misbehaving pipeline means reading raw stdout. For teams with compliance requirements, there is no auditable record of what ran, when, against what input, and what it produced.

The 15-factor app standard already mandates that the runtime emits structured logs to stdout. This extension goes further: it allows pipelines to declare named observability resources — traces and loggers — and reference them from individual steps, in the same way Docker Compose declares `networks:` and `volumes:` as top-level resources that services reference by name.

#### Proposed Structure

Observability resources are declared in a top-level `observability:` block and referenced from steps. This keeps configuration centralised and step definitions clean.

```yaml
observability:

  traces:
    - id: main_trace
      exporter: otlp
      endpoint: "{{ env.OTEL_ENDPOINT }}"
      service_name: "ail/{{ pipeline.run_id }}"

  logs:
    - id: audit_log
      format: json
      destination: stdout          # 15-factor compliant; the default for all logs

    - id: step_detail_log
      format: json
      destination: file
      path: ./.ail/logs/{{ pipeline.run_id }}.jsonl

defaults:
  trace: main_trace                # applied to every step unless overridden
  log: [audit_log]

pipeline:
  - id: security_audit
    prompt: "..."
    log: [audit_log, step_detail_log]   # step-level override adds a destination

  - id: internal_score
    prompt: "Rate quality 0.0–1.0. Number only."
    trace: none                         # opt this step out of tracing
```

#### OpenTelemetry Compatibility

OTEL is the target standard for trace export. Each pipeline run maps to an OTEL trace; each step maps to a span. Span attributes would carry at minimum:

- `ail.step.id`
- `ail.step.provider`
- `ail.step.token_usage.input`
- `ail.step.token_usage.output`
- `ail.step.condition.result`
- `ail.step.on_result.matched`
- `ail.pipeline.run_id`
- `ail.pipeline.file`

This makes `ail` pipeline executions visible in any OTEL-compatible backend — Grafana, Jaeger, Honeycomb, Datadog — with zero additional integration work.

The pipeline run log (§4.4) is the authoritative source; the OTEL exporter is a consumer of it.

#### Inheritance via `FROM`

Observability resources declared in a `FROM` base pipeline are inherited by all child pipelines. An organisation can declare a mandatory `compliance_trace` in its base pipeline and all inheriting pipelines emit to it automatically. A child pipeline may add additional loggers but cannot silently remove an inherited tracer — any attempt to `disable` a base-declared observability resource creates an explicit audit event rather than silently succeeding.

This makes compliance observability a governance guarantee, not a convention.

#### Unresolved Questions

- Are inherited observability resources merged (child adds to parent's list) or overridable (child can replace parent's config)? Merging is safer for compliance; overriding is more flexible for development.
- Can a step opt out of a default tracer? The `trace: none` syntax above is proposed but not decided. There may be compliance contexts where opting out should be impossible.
- How does sensitive data interact with trace export? Prompt content may contain credentials, PII, or proprietary information. There should be a way to mark prompt content as redacted in trace spans without disabling tracing entirely. This needs explicit design.
- Is the `observability:` block itself inheritable via `FROM` hook operations (`run_before`, `run_after`, `override`, `disable`), or does it follow a simpler merge model separate from the step hook system?
- What is the minimum viable observability for v0.0.1? Almost certainly structured JSON to stdout — already implied by 15-factor compliance. Everything else in this section is additive on top of that baseline.

---

### Template Variable Fallbacks

> **Status: Planned**

The `default()` filter in v0.1 accepts string literals only:
```yaml
{{ env.VAR_NAME | default("") }}
{{ env.VAR_NAME | default("NOT_SET") }}
```

A future version will support file references and inline blocks for cases where the fallback is a full prompt, markdown document, or complex JSON object:
```yaml
# File reference — fallback loaded from disk at resolution time
{{ env.SYSTEM_PROMPT | default(file: ./defaults/fallback-prompt.md) }}
{{ step.optional_context.response | default(file: ./defaults/empty-context.json) }}

# Inline block — for structured content without a separate file
{{ env.CONFIG | default(inline: '{"mode": "safe", "timeout": 120}') }}
```

This is particularly relevant for optional context injection steps where a missing variable should produce a well-formed prompt or configuration object rather than an empty string or short token. Until this extension is implemented, declare required variables explicitly and handle optional configuration via dedicated steps with `condition:` guards.

---

## 22. Open Questions

These are unresolved questions that require either implementation experience or dedicated research before they can be specced. They are tracked here so they are not lost.

---

### Completion Detection

**Status: Resolved for Claude CLI.**

The Claude CLI `--output-format stream-json` flag produces a newline-delimited NDJSON stream. Completion is signalled by a `{"type": "result", "subtype": "success", ...}` event — unambiguous, structured, and carrying cost metadata. PTY wrapping is not required for the Claude CLI runner.

For other runners without a structured output mode, process exit code 0 remains the fallback hypothesis. This should be validated per runner during each integration sprint. See `RUNNER-SPEC.md`.

**Remaining work:** Verify that `--output-format stream-json` is available in all Claude CLI invocation modes used by `ail`, and document error event shapes (`subtype: error`) for `on_error` handling.

---

### Context Accumulation

**Status: Resolved. See §4.4.**

The pipeline run log (§4.4) is the context system. Steps access prior results by querying the persisted log via template variables. Provider isolation is the default; session continuity is opt-in via `resume: true` (§15.4). The spike must validate the exact mechanics of `--input-format stream-json` for same-provider session resumption.

**Remaining work:** Spike must determine whether `--input-format stream-json` supports sending a new pipeline step prompt within the same session, or whether each step requires a new subprocess invocation with context passed via `{{ step.invocation.response }}` and `{{ last_response }}` template variables.

---

### Step Turns & Structured Output Data Model

**Status: Concrete model established for Claude CLI. Full spec deferred.**

The `--output-format stream-json` stream provides structured event types that map directly to the proposed `turns[]` model:

```
step.<id>
  .response              ← content of the result event; flows to next step
  .cost_usd              ← total_cost_usd from result event
  .turns[]               ← full NDJSON event sequence
    .type                ← "assistant" | "user" | "system"
    .content[]
      .type              ← "tool_use" | "tool_result" | "text"
      .name              ← tool name (tool_use events)
      .input             ← tool input parameters (tool_use events)
      .result            ← tool result (tool_result events)
      .text              ← text content (text events)
```

Full speccing of `step.<id>.turns[]` template variable access is deferred until the spike validates the exact event shapes and confirms whether partial message streaming (`--include-partial-messages`) is needed for the MVP.

**Remaining work:** Spike validation of event shapes, especially error events and extended thinking blocks.

---

### Hot Reload

**Question:** If a user edits `.ail.yaml` while a session is running, does the change take effect immediately or require a session restart?

**Note:** This may be a tool implementation decision rather than a spec decision. The spec defines what pipelines *are*; whether the runtime watches for file changes is an operational concern. Flagged here until implementation experience clarifies whether it needs to be specced.

---

### Skill Parameterisation

**Question:** How does a `SKILL.md` declare what parameters it accepts, and how are they injected — as template variables, environment variables, or a structured input block?

**Status:** Deferred. The `with:` syntax has been removed from the spec pending this design. Will be revisited when structured I/O schema support (§22) is implemented.

---

*This is a living document. Open a PR against this file to propose changes to the spec.*
