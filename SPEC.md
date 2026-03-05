# AIL Pipeline Language Specification

> **ail** — Alexander's Impressive Loops  
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
10. [Named Pipelines & Composition](#10-named-pipelines--composition)
11. [Template Variables](#11-template-variables)
12. [Conditions](#12-conditions)
13. [Human-in-the-Loop (HITL) Gates](#13-human-in-the-loop-hitl-gates)
14. [Built-in Modules](#14-built-in-modules)
15. [Providers](#15-providers)
16. [Triggers](#16-triggers)
17. [Error Handling & Resilience](#17-error-handling--resilience)
18. [The `materialize-chain` Command](#18-the-materialize-chain-command)
19. [Complete Examples](#19-complete-examples)
20. [MVP — v0.0.1 Scope](#20-mvp--v001-scope)
21. [Planned Extensions](#21-planned-extensions)
22. [Open Questions](#22-open-questions)

---

## 1. Purpose & Philosophy

Current agentic coding tools treat a human prompt as a single transactional event. If a developer wants a refactor or a security audit after code is generated, they must manually type the follow-up prompt every single time. This creates inconsistent quality and *prompt fatigue*.

**ail** introduces the **Deterministic Post-Processor**: a YAML-orchestrated pipeline runtime that ensures a specific, pre-determined chain of automated prompts fires after every human prompt — consistently, without manual intervention.

> **The Core Invariant**  
> For every completion event produced by an underlying agent, the pipeline defined in the active `.ail.yaml` file **must** execute in full, in order, before control returns to the human. This is the contract `ail` provides.

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
| `trigger` | The event that causes the pipeline to begin executing. The default trigger is `human_prompt_complete`. |
| `session` | One running instance of an underlying agent (e.g. Aider, Claude Code) managed by `ail`. |
| `completion event` | The signal that the underlying runner has finished. For CLI tools, this is typically process exit with code 0. See §22 Open Questions. |
| `HITL gate` | A Human-in-the-Loop gate. The pipeline pauses and waits for explicit human input before continuing. |
| `context` | The working memory passed between pipeline steps. See §22 Open Questions for accumulation semantics. |
| `provider` | The LLM backend a step routes its prompt to. May differ per step. |
| `condition` | A boolean expression evaluated before a step runs. If false, the step is skipped. |
| `on_result` | Declarative branching logic that fires after a step completes, based on the content of the response. |
| `FROM` | Keyword declaring that this pipeline inherits from another. Accepts a file path. Infinitely chainable. |
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
                            # accepts file paths only — see §21 for future URI support

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

pipeline:                   # required; ordered list of steps
  - id: dry_refactor
    prompt: "Refactor the code above to eliminate unnecessary repetition."

  - id: security_audit
    prompt: "Review the changes for common security vulnerabilities."
```

---

## 4. The Pipeline Execution Model

### 4.1 `invocation` — The Implicit First Step

Every pipeline has an implicit first step called `invocation`. It is never written in the YAML file, but it always exists and can always be referenced.

`invocation` represents the triggering event and the runner's response to it. The trigger may be:

- A human typing a prompt into the underlying agent
- Another pipeline calling this one as a step
- A scheduled or manual trigger

The pipeline's authored steps begin executing only after `invocation` completes. `ail` never intercepts or replaces the triggering interaction — it extends it.

```
invocation           ← implicit; always step zero
  ↓
step_1               ← first authored step in the pipeline
  ↓
step_2
  ↓
  ...
  ↓
[control returns to caller]
```

Because `invocation` names the event rather than the actor, the template variables are unambiguous regardless of what triggered the pipeline:

- `{{ step.invocation.prompt }}` — the input that triggered this pipeline run
- `{{ step.invocation.response }}` — the runner's response before any pipeline steps ran

### 4.2 Execution Guarantee

Once an `invocation` completion event fires, the full pipeline executes before control returns to the caller. If a HITL gate fires mid-pipeline, control remains locked until the human responds.

### 4.3 Hooks on `invocation`

Hook operations may target `invocation` directly, enabling session setup before the first prompt is processed.

```yaml
- run_before: invocation
  id: session_banner
  action: pause_for_human
  message: "Reminder: all outputs in this session are subject to compliance review."
```

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
| `id` | String. Unique identifier for this step. Snake_case recommended. Required if this step will be targeted by hooks, conditions, or template variables. |
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

### 5.2 `prompt:` — Inline and File

```yaml
# Inline prompt
- id: simple_check
  prompt: "Don't be stupid. Review the above output and fix anything obviously wrong."

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
| `abort_pipeline` | Stop immediately. Log to audit trail. |
| `repeat_step` | Re-run this step. Respects `max_retries`. |
| `goto: <step_id>` | Jump to a specific step by ID. Use sparingly. |
| `run_pipeline: <id>` | Invoke a named pipeline. See §10. |

**Match operators:**

| Operator | Meaning |
|---|---|
| `contains: "TEXT"` | Response contains literal string (case-insensitive). |
| `matches: "REGEX"` | Response matches regular expression. |
| `starts_with: "TEXT"` | Response begins with literal string. |
| `is_empty` | Response is blank or whitespace only. |
| `always` | Unconditionally fires. |

### 5.4 Step Output Model

Each step captures its output as `step.<id>.response` — the final text produced, available to subsequent steps via template variables.

For steps where `ail` calls an LLM provider directly, structured output (thinking traces, tool call sequences) is additionally captured. The full structured model is under active research and defined in §22 Open Questions.

For steps that wrap third-party CLI runners, `ail` captures stdout as the response. Completion is detected via process exit code 0. See §22 Open Questions for details on runner-specific behaviour.

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

`FROM` accepts file paths only — relative, absolute, or home-relative. Remote URI support is a planned extension (see §21).

`FROM` is infinitely chainable. The full chain is resolved at startup and inspectable via `ail materialize-chain` (§18).

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

- `disable:` targeting a step ID that does not exist in the chain → **parse error**
- `override:` declaring an `id` different from the step being overridden → **parse error**
- Two hooks in the same file both declaring `run_before` or `run_after` targeting the same step ID → **parse error**. Use sequential steps instead.
- Renaming a step ID in a `FROM`-able pipeline breaks all inheritors → treat step IDs as a **public API**

### 7.3 `FROM` and Pipeline Identity

Pipelines do not currently have a registry identity beyond their file path. When specifying `FROM`, use the file path directly. Pipeline registries, versioning, and remote URIs are planned extensions (§21).

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

If the called pipeline aborts internally, `ail` surfaces a pipeline stack trace to the TUI — equivalent to a call stack — showing which pipeline failed, at which step, and why. The caller's `on_error` field governs what happens next. The full internal trace is written to the audit trail.

---

## 10. Named Pipelines & Composition

A single `.ail.yaml` file may define multiple named pipelines under a `pipelines` key. Only `default` runs automatically. Others are invocable via `run_pipeline` in `on_result`.

```yaml
pipelines:

  default:
    - id: dry_check
      prompt: "Refactor for DRY principles."
      on_result:
        always:
          action: run_pipeline
          target: security_gates

  security_gates:
    - id: vuln_scan
      prompt: "Identify vulnerabilities."
      on_result:
        contains: "VULNERABILITY"
        if_true:
          action: run_pipeline
          target: escalation

  escalation:
    - id: notify_human
      action: pause_for_human
      message: "Security escalation required."
```

---

## 11. Template Variables

Prompt strings and file-based prompts may reference runtime context using `{{ variable }}` syntax. Variables resolve at step execution time, not at load time.

| Variable | Value |
|---|---|
| `{{ step.invocation.prompt }}` | The input that triggered this pipeline run. |
| `{{ step.invocation.response }}` | The runner's response before any pipeline steps ran. |
| `{{ last_response }}` | The full response from the immediately preceding step. |
| `{{ step.<id>.response }}` | The response from a specific named step in this pipeline run. |
| `{{ session.tool }}` | The underlying runner name (e.g. `aider`, `claude-code`). |
| `{{ session.cwd }}` | The current working directory of the session. |
| `{{ pipeline.run_id }}` | Unique ID for this pipeline execution. |
| `{{ env.VAR_NAME }}` | An environment variable. Fails loudly if not set. |

> **Note:** There are no convenience aliases. All variable references use the dot-path structure above. This keeps the mental model consistent — every step, including `invocation`, is accessed the same way.

**Skipped step variables:** If a template variable references a step that was skipped by its `condition`, `ail` raises a **parse-time error** if the reference is unconditional, or returns an empty string if the referencing step itself has a matching condition guard. Silently empty references are never permitted.

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

```yaml
# Built-in
condition: if_code_changed

# Dot-path comparison
condition: "context.file_count > 0"

# Step response check
condition: "step.security_audit.response contains 'VULNERABILITY'"

# Logical operators
condition: "if_code_changed and not if_first_run"
```

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
| **Reject** | Pipeline aborts. Reason logged to audit trail. |
| **Modify** | Human edits the preceding step's output or provides a correction. Pipeline re-evaluates from this step. |

### 13.3 Implicit HITL via `on_result`

Preferred over explicit gates — interrupts only when something genuinely requires attention. See §5.3.

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
    with:
      target_tokens: 512

  - id: security
    skill: ail/security-audit
    on_result:
      contains: "VULNERABILITY"
      if_true:
        action: abort_pipeline
```

> **Note:** `with:` syntax for passing parameters to skills is provisionally supported. Full parameter declaration semantics — how a SKILL.md declares accepted parameters — are to be defined after the Claude CLI research phase. See §22.

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

---

## 16. Triggers

| Trigger | When it fires |
|---|---|
| `human_prompt_complete` | Default. Every time the runner finishes responding to human input. |
| `code_file_saved` | When the runner writes a code file to disk. |
| `session_end` | When the session is terminated. Useful for summary or commit steps. |
| `manual` | Only when explicitly invoked via `ail run`. Never automatic. |
| `scheduled: "cron"` | On a cron schedule. For background tasks. |

> **Note:** Session setup — steps that run once before the first human prompt — is handled via `run_before: invocation` (see §4.3), not a separate trigger. This keeps the trigger list focused on when a pipeline fires, not on lifecycle hooks.

---

## 17. Error Handling & Resilience

| Value | Effect |
|---|---|
| `continue` | Log error, proceed. Only for explicitly non-critical steps. |
| `pause_for_human` | Suspend pipeline, surface error in HITL panel. **Default.** |
| `abort_pipeline` | Stop immediately. Log full error context to audit trail. |
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

## 18. The `materialize-chain` Command

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

`materialize-chain` is the primary debugging tool. Reach for it first when a pipeline behaves unexpectedly.

---

## 19. Complete Examples

### 19.1 The Simplest Possible Pipeline

```yaml
version: "0.1"

pipeline:
  - prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

### 19.2 Solo Developer Quality Loop

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

### 19.3 Session Setup with `run_before: invocation`

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

### 19.4 Org Base Pipeline

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

### 19.5 Project Inheriting from Org Base

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

### 19.6 LLM Researcher Model Comparison

```yaml
version: "0.1"

pipeline:
  - id: compare
    skill: ail/model-compare
    with:
      model_a: openai/gpt-4o
      model_b: anthropic/claude-opus-4-5
      prompt: "{{ step.invocation.prompt }}"
    on_result:
      always:
        action: pause_for_human
        message: "Comparison complete. Review outputs above."
```

### 19.7 Multi-Speed Pipeline

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

## 20. MVP — v0.0.1 Scope

The goal of v0.0.1 is a working demo: one pipeline, one runner, one follow-up prompt, visibly running. Nothing more. This is the proof of concept that validates the core invariant before any additional complexity is added.

**In scope for v0.0.1:**

| Feature | Notes |
|---|---|
| Single pipeline file (`.ail.yaml`) | No inheritance, no `FROM` |
| `pipeline:` array with ordered steps | Top-to-bottom execution only |
| `prompt:` field — inline string only | No file path resolution yet |
| `id:` field | Required for all steps in v0.0.1 |
| `provider:` field | At least one working provider |
| `on_result: contains` + `continue` / `pause_for_human` / `abort_pipeline` | Minimal branching |
| `condition: always` and `condition: never` | Trivial conditions — proves the condition system exists |
| `{{ step.invocation.response }}` and `{{ last_response }}` | Core template variables |
| Passthrough mode when no `.ail.yaml` found | Safe default |
| `ail materialize-chain` | Flattens a single-file pipeline — no inheritance to traverse yet, but establishes the command |
| Basic TUI — streaming stdout passthrough | Human can see the runner working |
| Completion detection via process exit code 0 | For CLI runner steps |

**Explicitly out of scope for v0.0.1:**

- `FROM` inheritance and all hook operations
- `skill:` field
- `pipeline:` field (calling sub-pipelines)
- `action: pause_for_human` (HITL gates)
- `condition:` expressions beyond `always` / `never`
- File path resolution for `prompt:`
- `defaults:` block
- Multiple named pipelines
- All built-in modules
- `session_end` trigger
- Everything in §21 Planned Extensions

**The v0.0.1 demo case:**

```yaml
version: "0.0.1"

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

### Structured Step Output Schema

> **Status: Planned — pending Claude CLI research**

Steps declare an expected JSON schema. `ail` validates and retries on malformed output.

```yaml
- id: vulnerability_scan
  prompt: ./prompts/vuln-scan.md
  output_schema: ./schemas/vulnerability-report.json
```

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

### Pipeline Registry & Versioning

> **Status: Planned**

Named pipeline identity, versioning, and a registry system. Enables `FROM: org/security-base@2.1` style references. Will be designed alongside remote `FROM` support.

---

## 22. Open Questions

These are unresolved questions that require either implementation experience or dedicated research before they can be specced. They are tracked here so they are not lost.

---

### Completion Detection

**Question:** How does `ail` reliably detect that an underlying CLI runner has finished responding?

**Current hypothesis:** For runners invoked as discrete CLI processes (e.g. `claude --print "..."`, `aider --message "..."`), process exit code 0 signals completion. This is the Unix standard and requires no PTY parsing.

**What needs research:** Whether all target runners support a non-interactive `--print` or `--message` invocation mode, what their exit code behaviour is on error, and whether there are cases where a runner exits 0 but has not produced useful output.

**Blocking:** Phase 0 spike. This is the most critical unknown before any code is written.

---

### Context Accumulation

**Question:** When a pipeline step runs, what context does it receive? Only the immediately preceding step's response (`{{ last_response }}`)? The full conversation history? Something configurable?

**Why it matters:** This directly affects every prompt template a user writes. A step that needs to reference code from three steps ago behaves very differently depending on the accumulation model.

**What needs research:** The Claude CLI and other runner CLIs likely have specific mechanisms for context passing — `--context`, session files, or conversation history flags. This should be researched against the Claude CLI reference before speccing.

**Related:** The step `turns[]` structured data model (see below) may be the same research effort.

---

### Step Turns & Structured Output Data Model

**Question:** For steps where `ail` calls an LLM directly, a single "step" may involve multiple round trips (tool calls, thinking traces, intermediate responses). How is this represented?

**Proposed model (pending validation):**

```
step.<id>
  .response          ← final text output; what flows to next step
  .turns[]           ← full round-trip sequence
    .thinking        ← reasoning trace if present
    .text            ← text blocks
    .tool_calls[]
      .name
      .input
      .result
```

**What needs research:** What the Claude API actually returns for extended thinking and tool use responses, whether other providers have equivalent structures, and how this maps to stdout capture from CLI runners.

**Related:** Context accumulation research above.

---

### Hot Reload

**Question:** If a user edits `.ail.yaml` while a session is running, does the change take effect immediately or require a session restart?

**Note:** This may be a tool implementation decision rather than a spec decision. The spec defines what pipelines *are*; whether the runtime watches for file changes is an operational concern. Flagged here until implementation experience clarifies whether it needs to be specced.

---

### `with:` Parameter Semantics for Skills

**Question:** The `with:` block on a `skill:` step passes parameters to the skill. How does a SKILL.md declare what parameters it accepts? How are they injected — as template variables, environment variables, or a structured input block?

**Blocked on:** Claude CLI research phase. The answer likely depends on how the Claude skills system handles parameterisation.

---

*This is a living document. Open a PR against this file to propose changes to the spec.*
