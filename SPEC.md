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
20. [Planned Extensions](#20-planned-extensions)

---

## 1. Purpose & Philosophy

Current agentic coding tools treat a human prompt as a single transactional event. If a developer wants a refactor or a security audit after code is generated, they must manually type the follow-up prompt every single time. This creates inconsistent quality and *prompt fatigue*.

**ail** introduces the **Deterministic Post-Processor**: a YAML-orchestrated pipeline runtime that ensures a specific, pre-determined chain of automated prompts fires after every human prompt — consistently, without manual intervention.

> **The Core Invariant**  
> For every completion event produced by an underlying agent, the pipeline defined in the active `.ail.yaml` file **must** execute in full, in order, before control returns to the human. This is the contract `ail` provides.

The AIL Pipeline Language (APL) is the product. The orchestration engine is its runtime. Everything else — context distillation, learning loops, multi-model routing — are optional pipeline steps, not architectural prerequisites.

### The Two Layers

`ail` operates across two distinct layers that should never be confused:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime engine | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable.

---

## 2. Concepts & Vocabulary

These terms have precise meanings throughout this specification and in all `ail` source code.

| Term | Definition |
|---|---|
| `pipeline` | A named, ordered sequence of steps defined in a `.ail.yaml` file. One pipeline is "active" per session. |
| `step` | A single unit of work within a pipeline. A step invokes a prompt, skill, sub-pipeline, or action, then optionally branches on the result. |
| `human_input` | The implicit first step of every pipeline. Represents the human's prompt and the agent's completion. All subsequent steps react to it. |
| `skill` | A directory containing a `SKILL.md` file — natural language instructions that tell the model how to perform a specialised task. Read by the LLM, not the runtime. |
| `trigger` | The event that causes the pipeline to begin executing. The default trigger is `human_prompt_complete`. |
| `session` | One running instance of an underlying agent (e.g. Aider, Claude Code) managed by `ail`. |
| `completion event` | The signal that the underlying agent has finished responding to the human. `ail` begins the pipeline on this signal. |
| `HITL gate` | A Human-in-the-Loop gate. The pipeline pauses and waits for explicit human input before continuing. |
| `context` | The working memory passed between pipeline steps. Each step may read and append to it. |
| `provider` | The LLM backend a step routes its prompt to. May differ per step. |
| `condition` | A boolean expression evaluated before a step runs. If false, the step is skipped. |
| `on_result` | Declarative branching logic that fires after a step completes, based on the content of the response. |
| `FROM` | Keyword declaring that this pipeline inherits from another, layering its steps on top of the parent. Infinitely chainable. |
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

meta:                       # optional block
  name: "My Quality Gates"
  description: "DRY refactor + security audit on every output"
  author: "alex@example.com"

providers:                  # optional; define named provider aliases (see §15)
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

### 4.1 `human_input` — The Implicit First Step

Every pipeline has an implicit first step called `human_input`. It is never written in the YAML file, but it always exists and can always be referenced.

`human_input` represents two things in sequence:

1. The human typing a prompt into the underlying agent.
2. The agent completing its response.

The pipeline's authored steps begin executing only after `human_input` completes. `ail` never intercepts or replaces the human's interaction with the agent — it extends it.

```
human_input          ← implicit; always step zero
  ↓
step_1               ← first authored step in the pipeline
  ↓
step_2
  ↓
  ...
  ↓
[control returns to human]
```

### 4.2 Template Anchors

`human_input` being named and explicit gives template variables an unambiguous anchor:

- `{{ human_prompt }}` — the human's raw prompt text. Alias for `{{ step.human_input.prompt }}`.
- `{{ step.human_input.response }}` — the agent's raw completion, before any pipeline steps ran.

### 4.3 Execution Guarantee

Once a `human_input` completion event fires, the full pipeline executes before the human's input prompt is re-enabled. The human cannot type a new prompt while the pipeline is running. If a HITL gate fires mid-pipeline, the input prompt remains locked until the human responds to the gate.

### 4.4 Hooks on `human_input`

Hook operations (`run_before`, `run_after`) may target `human_input` directly, enabling session setup steps or pre-flight checks before the human types their first prompt.

```yaml
- run_before: human_input
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
| `prompt` | String or file path. The text sent to the LLM. If the value begins with `./`, `../`, `~/`, or `/`, it is treated as a path to a markdown file. Otherwise it is treated as an inline string. |
| `skill` | Path or alias. Loads an Agent Skills-compliant `SKILL.md` package. See §6. |
| `pipeline` | Path. Calls another `.ail.yaml` as an isolated sub-pipeline. See §9. |
| `action` | String. A non-LLM operation. Currently supported: `pause_for_human`. |
| `provider` | String. Overrides the default provider for this step. Format: `vendor/model-name` or alias. |
| `timeout_seconds` | Integer. Maximum seconds to wait for a response. Default: `120`. |
| `condition` | Expression string. Step is skipped if this evaluates to false. See §12. |
| `on_error` | Enum. Behaviour on failure: `continue` \| `pause_for_human` \| `abort_pipeline` \| `retry`. Default: `pause_for_human`. |
| `max_retries` | Integer. Maximum retry attempts when `on_error: retry`. Default: `2`. |
| `on_result` | Block. Declarative branching after step completion. See §5.3. |
| `disabled` | Boolean. If `true`, step is skipped unconditionally. Useful during development. |

### 5.2 `prompt:` — Inline and File

```yaml
# Inline prompt
- id: simple_check
  prompt: "Don't be stupid. Review the above output and fix anything obviously wrong."

# Prompt loaded from a markdown file
- id: detailed_review
  prompt: ./prompts/architectural-review.md

# Prompt from outside the project
- id: org_style_check
  prompt: ../org-prompts/style-guide-check.md

# Prompt from user's home directory
- id: personal_conventions
  prompt: ~/prompts/my-conventions.md
```

When a path is provided, `ail` reads the file at pipeline load time and uses its content as the prompt string. Template variables in the file are resolved at step execution time, not at load time.

### 5.3 `on_result` — Declarative Branching

`on_result` inspects the step's response and takes action without requiring code. It is evaluated after the step completes and before the next step begins.

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

**Supported `on_result` actions:**

| Action | Effect |
|---|---|
| `continue` | Proceed to the next step. Default if `on_result` is omitted. |
| `pause_for_human` | Suspend the pipeline. Display the step output in the HITL panel. Wait for human Approve / Reject / Modify. |
| `abort_pipeline` | Stop immediately. Log the reason to the audit trail. |
| `repeat_step` | Re-run this step. Respects `max_retries`. |
| `goto: <step_id>` | Jump to a specific step by ID. Use sparingly. |
| `run_pipeline: <id>` | Invoke a named pipeline as a sub-routine. See §10. |

**`on_result` match operators:**

| Operator | Meaning |
|---|---|
| `contains: "TEXT"` | True if the response contains the literal string (case-insensitive). |
| `matches: "REGEX"` | True if the response matches the regular expression. |
| `starts_with: "TEXT"` | True if the response begins with the literal string. |
| `is_empty` | True if the response is blank or whitespace only. |
| `always` | Unconditionally fires. |

---

## 6. Skills

A skill is a directory containing a `SKILL.md` file — natural language instructions that tell the model how to perform a specialised task. Skills follow the [Agent Skills open standard](https://agentskills.io), making any skill authored for Claude, Gemini CLI, GitHub Copilot, Cursor, or other compatible tools directly usable in `ail` pipelines without modification.

### 6.1 The Skill/Pipeline Distinction

| | Skill | Pipeline |
|---|---|---|
| **Format** | Markdown | YAML |
| **Read by** | The LLM | The `ail` runtime |
| **Contains** | Instructions, examples, guidelines | Control flow, sequencing, branching |
| **Scope** | How to think about a task | When to run it and what to do with the result |

### 6.2 Using a Skill in a Step

```yaml
# Skill from a local directory
- id: security_review
  skill: ./skills/security-reviewer/

# Skill from outside the project
- id: org_review
  skill: ../org-skills/compliance-checker/

# Skill from user's home directory
- id: personal_style
  skill: ~/skills/my-conventions/

# Built-in ail skill (implemented as Agent Skills-compliant packages)
- id: dry_check
  skill: ail/dry-refactor
```

### 6.3 Combining `skill:` and `prompt:`

A step may declare both a `skill:` and a `prompt:`. The skill provides standing instructions — persistent expertise the model carries for this step. The prompt provides the specific task for this invocation.

```yaml
- id: security_review
  skill: ./skills/security-reviewer/   # how to think about security
  prompt: "{{ step.human_input.response }}"  # what to review right now
  provider: frontier
  on_result:
    contains: "CLEAN"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security findings require human review."
```

When both are present, the skill content is provided as system/instruction context and the prompt is the user-level task. The model sees them as distinct layers.

### 6.4 Agent Skills Compatibility

`ail`'s built-in modules (see §14) are implemented as Agent Skills-compliant `SKILL.md` packages. Any skill from the broader Agent Skills ecosystem is usable in an `ail` pipeline by path reference. `ail` does not require skills to be registered or declared — a path is sufficient.

---

## 7. Pipeline Inheritance

### 7.1 `FROM`

A pipeline may declare that it inherits from another using the `FROM` keyword. The inheriting pipeline receives all steps from the parent and may add, modify, or remove steps using hook operations.

```yaml
FROM: ./org-base.yaml
```

`FROM` is infinitely chainable. A pipeline may inherit from a parent that itself inherits from a grandparent, forming a chain of any depth. The full chain is resolved at startup and inspectable via `ail materialize-chain` (see §18).

**Supported `FROM` targets:**

| Format | Example | Notes |
|---|---|---|
| Relative path | `FROM: ./base.yaml` | Resolved relative to the inheriting file. |
| Absolute path | `FROM: /etc/ail/org-base.yaml` | Useful for system-wide org policies. |
| Named alias | `FROM: org/security-base` | Resolved via alias config. |
| Remote URL | `FROM: https://...` | See §20 — Planned Extensions. |

### 7.2 Hook Operations

When a pipeline inherits via `FROM`, it modifies the inherited pipeline using four operations, all targeting a step by its `id`.

| Operation | Effect |
|---|---|
| `run_before: <id>` | Insert one or more steps immediately before the named step. |
| `run_after: <id>` | Insert one or more steps immediately after the named step. |
| `override: <id>` | Replace the named step entirely. The replacement inherits the original's `id`. |
| `disable: <id>` | Remove the named step. It will not appear in the materialized chain. |

```yaml
FROM: ./org-base.yaml

pipeline:
  - run_before: security_audit
    id: license_header_check
    prompt: "Verify all modified files have the correct license header."

  - run_after: test_writer
    id: coverage_reminder
    prompt: "Does the new test coverage meet the 80% threshold?"

  - override: dry_refactor
    prompt: "Refactor using this project's conventions in CONTRIBUTING.md."

  - disable: commit_checkpoint
```

### 7.3 Breaking Changes in Inherited Pipelines

If a base pipeline renames or removes a step ID, all pipelines targeting that ID via hook operations will fail at parse time with a clear error. Renaming a step ID in a `FROM`-able pipeline is a **breaking change**. Base pipeline authors should treat step IDs as a public API.

---

## 8. Hook Ordering — The Onion Model

When multiple layers of an inheritance chain declare hooks targeting the same step ID, execution follows one rule:

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

An organisation's base pipeline can guarantee that its `run_before: security_audit` hook fires as the last thing before the audit runs — and its `run_after` fires first after — regardless of what any project-level pipeline adds around the outside. The base pipeline governs what happens immediately adjacent to the step itself.

### 8.4 Multiple Hooks at the Same Layer

Multiple hooks targeting the same step ID within a single file execute in top-to-bottom declaration order.

---

## 9. Calling Pipelines as Steps

A pipeline may call another pipeline as a step using the `pipeline:` primary field. The called pipeline runs as a fully isolated sub-routine.

### 9.1 Isolation Model

```
Caller pipeline context
  ↓ (full current context passed as input)
Called pipeline
  └─ human_input = caller's current context snapshot
  └─ runs its own steps in complete isolation
  └─ its own {{ last_response }}, {{ step.x.response }} are all local
  └─ returns its final step's output as a single response
  ↓
Caller pipeline receives {{ step.<call_id>.response }}
```

The caller sees only the called pipeline's final output. It never has visibility into the called pipeline's internal steps, intermediate responses, or context.

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

If the called pipeline aborts internally, that failure surfaces to the caller as an `on_error` event. The caller's `on_error` field governs what happens next. The called pipeline's internal failure is logged in full to the audit trail.

---

## 10. Named Pipelines & Composition

A single `.ail.yaml` file may define multiple named pipelines under a `pipelines` key. Only the pipeline named `default` (or explicitly activated) runs automatically. Others are available for composition via `run_pipeline` in `on_result`.

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

Prompt strings and file-based prompts may reference runtime context using `{{ variable }}` syntax. Variables are resolved at step execution time, not at load time.

| Variable | Value |
|---|---|
| `{{ human_prompt }}` | The human's raw prompt. Alias for `{{ step.human_input.prompt }}`. |
| `{{ last_response }}` | The full response from the immediately preceding step. |
| `{{ step.human_input.prompt }}` | The human's raw prompt text. |
| `{{ step.human_input.response }}` | The agent's raw completion before any pipeline steps ran. |
| `{{ step.<id>.response }}` | The response from a specific named step in the current pipeline run. |
| `{{ session.tool }}` | The underlying agent tool name (e.g. `aider`, `claude-code`). |
| `{{ session.cwd }}` | The current working directory of the session. |
| `{{ pipeline.run_id }}` | A unique ID for this pipeline execution. Useful for logging. |
| `{{ env.VAR_NAME }}` | An environment variable. Fails loudly if not set — never silently empty. |

```yaml
- id: context_aware_refactor
  prompt: |
    The human asked: "{{ human_prompt }}"
    The agent responded: "{{ step.human_input.response }}"
    Refactor the code to remove duplication without changing behaviour.
    Working directory: {{ session.cwd }}
```

Template variables in file-based prompts work identically to inline prompts.

---

## 12. Conditions

The `condition` field on a step allows declarative skip logic. If the expression evaluates to false, the step is silently skipped and the pipeline continues.

### 12.1 Built-in Conditions

| Expression | Meaning |
|---|---|
| `if_code_changed` | True if the agent's response contains a code block (``` fence detected). |
| `if_files_modified` | True if the underlying tool modified one or more files on disk. |
| `if_last_response_empty` | True if the previous step's response was blank. |
| `if_first_run` | True if this is the first pipeline run in this session. |
| `always` | Always true. Equivalent to omitting `condition`. |
| `never` | Always false. Identical to `disabled: true`. Useful during development. |

### 12.2 Expression Syntax

```yaml
# Built-in condition
condition: if_code_changed

# Dot-path comparison
condition: "context.file_count > 0"

# Step response inspection
condition: "step.security_audit.response contains 'VULNERABILITY'"

# Logical operators
condition: "if_code_changed and not if_first_run"
```

---

## 13. Human-in-the-Loop (HITL) Gates

HITL gates are first-class constructs in APL — intentional checkpoints, not error states.

### 13.1 Explicit HITL Step

```yaml
- id: approve_before_deploy
  action: pause_for_human
  message: "Pipeline complete. Approve to continue to deployment."
  timeout_seconds: 3600
  on_timeout: abort_pipeline
```

### 13.2 HITL Responses

| Response | Effect |
|---|---|
| **Approve** | The gate clears. Pipeline continues with context unchanged. |
| **Reject** | Pipeline aborts. Rejection reason logged to the audit trail. |
| **Modify** | Human edits the preceding step's output or provides a correction prompt. Pipeline re-evaluates from this step with modified context. |

### 13.3 Implicit HITL via `on_result`

The most common HITL pattern — a step finds something, and the pipeline pauses only if warranted. Preferred over explicit gates because it only interrupts when something genuinely requires human attention. See §5.3.

---

## 14. Built-in Modules

`ail` ships with a library of pre-authored modules referenceable via the `skill:` field using the `ail/` prefix. Each built-in is implemented as an Agent Skills-compliant `SKILL.md` package — inspectable, forkable, and overridable by any user.

> **Design Principle**  
> Nothing in the built-in module library is special. Every built-in is implemented using the same constructs available to any user-defined skill. A user can inspect, fork, and override any built-in by pointing `skill:` at their modified copy.

| Module | Description |
|---|---|
| `ail/janitor` | Context distillation. Compresses working context to reduce token usage. Configurable target token budget. |
| `ail/dry-refactor` | Refactors code output for DRY (Don't Repeat Yourself) compliance. |
| `ail/security-audit` | Security-focused review. Pauses for human if findings exist. |
| `ail/test-writer` | Generates unit tests for new functions detected in the preceding response. |
| `ail/model-compare` | Runs the same prompt against two configurable providers and presents outputs side by side. |
| `ail/budget-gate` | Checks accumulated token spend against a configurable limit. Pauses for human if exceeded. |
| `ail/commit-checkpoint` | Prompts the user to commit current changes before the pipeline continues. |

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

---

## 15. Providers

Every step that makes an LLM call must resolve to a provider. Providers are configured separately from pipeline logic — keeping pipeline YAML readable and credentials out of version control.

### 15.1 Provider String Format

```yaml
# Format: vendor/model-name
provider: openai/gpt-4o
provider: anthropic/claude-opus-4-5
provider: groq/llama-3.1-70b-versatile
provider: cerebras/llama-3.3-70b

# Named alias (see §15.2)
provider: fast
provider: frontier
```

### 15.2 Provider Aliases

Defined in `~/.config/ail/providers.yaml` or in a `providers` block in the pipeline file. Aliases allow pipelines to be written without coupling to a specific vendor.

```yaml
providers:
  fast:     groq/llama-3.1-70b-versatile
  balanced: openai/gpt-4o-mini
  frontier: anthropic/claude-opus-4-5

defaults:
  provider: balanced

pipeline:
  - id: quick_check
    provider: fast
    prompt: "Is this code syntactically valid? Answer YES or NO only."

  - id: deep_review
    provider: frontier
    prompt: "Perform a thorough architectural review."
```

---

## 16. Triggers

By default, a pipeline fires on every `human_prompt_complete` event.

| Trigger | When it fires |
|---|---|
| `human_prompt_complete` | Default. Every time the agent finishes responding to human input. |
| `code_file_saved` | When a code file is written to disk by the agent. |
| `session_start` | Once when a new `ail` session begins. |
| `session_end` | When the session is terminated. |
| `manual` | Only when explicitly invoked (e.g. `ail run`). Never fires automatically. |
| `scheduled: "cron"` | On a cron schedule. For background quality analysis tasks. |

```yaml
pipelines:

  default:
    trigger: human_prompt_complete
    steps:
      - skill: ail/dry-refactor

  session_init:
    trigger: session_start
    steps:
      - id: load_conventions
        prompt: "Summarise the coding conventions in this repo's README."
```

---

## 17. Error Handling & Resilience

Pipeline errors must never pass silently.

| Value | Effect |
|---|---|
| `continue` | Log the error and proceed. Use only for explicitly non-critical steps. |
| `pause_for_human` | Suspend the pipeline, surface the error in the HITL panel. **Default.** |
| `abort_pipeline` | Stop immediately. Log the full error context to the audit trail. |
| `retry` | Retry up to `max_retries` times (default: `2`) before escalating to `pause_for_human`. |

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

Because pipelines can inherit across multiple files with hooks from several layers targeting the same steps, the actual execution order may not be obvious from reading any single file.

`ail materialize-chain` resolves the full inheritance chain and writes the complete, flattened pipeline to disk. No inheritance, no hooks — just steps in the exact order they will execute, with comments indicating each step's origin.

```bash
# Print to stdout
ail materialize-chain

# Write to file
ail materialize-chain --out materialized.yaml

# Materialize a specific pipeline
ail materialize-chain --pipeline ./deploy.yaml --out materialized.yaml

# Expand pipeline-as-step calls recursively
ail materialize-chain --expand-pipelines
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

  # step: human_input (implicit)

  # origin: [1] deploy.yaml  (run_before: security_audit — outer shell)
  - id: deploy_pre_check
    prompt: "..."

  # origin: [2] .ail.yaml  (run_before: security_audit)
  - id: license_header_check
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

  # origin: [2] .ail.yaml  (run_after: security_audit)
  - id: project_findings_formatter
    prompt: "..."

  # origin: [1] deploy.yaml  (run_after: security_audit — outer shell)
  - id: deploy_post_check
    prompt: "..."
```

`materialize-chain` is the authoritative answer to "what will actually happen when I run this?" It is the first debugging tool to reach for when a pipeline behaves unexpectedly.

---

## 19. Complete Examples

### 19.1 The Simplest Possible Pipeline

*"Don't be stupid" — one step, always runs.*

```yaml
version: "0.1"

pipeline:
  - prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

### 19.2 Solo Developer Quality Loop

*DRY refactor + tests, only when code changed. Prompts loaded from files.*

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

### 19.3 Org Base Pipeline

*Defined once at `/etc/ail/acme-base.yaml`. Inherited by all teams.*

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

  - id: budget_check
    skill: ail/budget-gate
    with:
      limit_usd: 2.00
```

### 19.4 Project Pipeline Inheriting from Org Base

*Adds PCI compliance check inside the org's security audit. Disables the budget gate.*

```yaml
version: "0.1"

FROM: /etc/ail/acme-base.yaml

meta:
  name: "Payments Team — Project Phoenix"

pipeline:
  # Fires immediately before security_audit (innermost position for this layer)
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

  - disable: budget_check
```

### 19.5 Pipeline Calling a Sub-Pipeline

*Quality gates delegate to an isolated security pipeline.*

```yaml
version: "0.1"

pipeline:
  - id: dry_refactor
    condition: if_code_changed
    skill: ail/dry-refactor

  - id: security_suite
    condition: if_code_changed
    pipeline: ./pipelines/security-suite.yaml
    on_result:
      contains: "ALL_CLEAR"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Security suite found issues. See details above."
```

### 19.6 LLM Researcher Model Comparison

```yaml
version: "0.1"

meta:
  name: "Model Comparison Harness"

pipeline:
  - id: compare
    skill: ail/model-compare
    with:
      model_a: openai/gpt-4o
      model_b: anthropic/claude-opus-4-5
      prompt: "{{ human_prompt }}"
    on_result:
      always:
        action: pause_for_human
        message: "Comparison complete. Review outputs above."
```

### 19.7 Multi-Speed Pipeline

*Fast model for syntax, frontier for architecture.*

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

## 20. Planned Extensions

These features are designed and their syntax is reserved. They are not yet implemented. Do not use these keywords in production pipelines — they will be rejected by the current parser.

> **For contributors:** If you are interested in implementing a planned extension, open an issue referencing this section before beginning work.

---

### Parallel Step Execution

> **Status: Planned** — Syntax reserved. Not implemented.

Multiple targets within a single step, executed concurrently. All branches receive the same input context. The step completes when all branches complete.

```yaml
- id: parallel_review
  parallel:
    - id: security
      skill: ./skills/security-reviewer/
    - id: dry_check
      skill: ail/dry-refactor
    - id: test_suite
      pipeline: ./pipelines/tests.yaml
  on_result:
    # on_result receives a structured object with named branch results
    # {{ step.parallel_review.security.response }}, etc.
    always:
      action: pause_for_human
      message: "Parallel review complete."
```

---

### Fan-Out / Fan-In with Synthesis

> **Status: Planned** — Syntax reserved. Not implemented.

An extension of parallel execution where a `synthesize:` step reconciles multiple branch outputs into a single response before `on_result` is evaluated.

```yaml
- id: full_review
  parallel:
    - id: security
      skill: ./skills/security-reviewer/
    - id: dry
      skill: ail/dry-refactor
  synthesize:
    prompt: |
      Security review: {{ step.full_review.security.response }}
      DRY review: {{ step.full_review.dry.response }}
      Produce a single consolidated report with overall recommendation.
```

---

### Remote `FROM` Targets

> **Status: Planned** — Syntax reserved. Not implemented.

Allows organisations to publish canonical base pipelines that teams inherit across repositories without copying files.

```yaml
FROM: https://pipelines.acme-corp.internal/engineering-standards/base.yaml
```

Authentication, caching, and integrity verification semantics are to be defined.

---

### Structured Step Output

> **Status: Planned** — Syntax reserved. Not implemented.

Steps may declare an expected JSON schema for their output. `ail` validates the response against the schema and retries automatically on malformed output before escalating to `on_error`.

```yaml
- id: vulnerability_scan
  prompt: ./prompts/vuln-scan.md
  output_schema: ./schemas/vulnerability-report.json
  on_error: retry
  max_retries: 3
```

---

### Dry Run Mode

> **Status: Planned** — Not implemented.

Renders all prompts with live template variable values and prints the full execution plan without making any LLM calls. Complements `materialize-chain`, which covers structure but not rendered content.

```bash
ail --dry-run
ail --dry-run --pipeline ./deploy.yaml
```

---

*This is a living document. Open a PR against this file to propose changes to the spec.*
