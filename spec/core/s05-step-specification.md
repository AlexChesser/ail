## 5. Step Specification

Every item in the `pipeline` array is a step. Each step is of exactly one of four types, declared by its primary field:

| Step type | Primary field | Task source | LLM call | Token cost |
|---|---|---|---|---|
| Prompt | `prompt:` | Inline text or file | Yes | Yes |
| Skill | `skill:` | `SKILL.md` body | Yes | Yes |
| Context | `context:` | `shell:` / `mcp:` | No | No |
| Sub-pipeline | `pipeline:` | Another `.ail.yaml` | Delegated | Delegated |

Exactly one primary field is required per step. All other fields are optional.

`prompt:` and `skill:` are both LLM invocations — the distinction is where the task instruction comes from. `prompt:` gives it as text; `skill:` uses the body of a `SKILL.md` file, making the step self-contained and directly analogous to a `/skill-name` invocation in the `ail` REPL.

### 5.1 Core Fields (all step types)

| Field | Description |
|---|---|
| `id` | String. **Required.** Unique identifier for this step within the resolved pipeline. Snake_case recommended. Step IDs are the public API of a `FROM`-able pipeline — treat them as stable identifiers. |
| `runner` | String. Optional. Name of the runner to use for this step. Overrides `AIL_DEFAULT_RUNNER` and the pipeline-level default. See §19 and `RunnerFactory`. Recognised values: `claude`, `stub`. |
| `condition` | Expression string. Step is skipped if false. See §12. |
| `on_error` | Enum: `continue` \| `pause_for_human` \| `abort_pipeline` \| `retry`. Default: `pause_for_human`. |
| `max_retries` | Integer. Retry attempts when `on_error: retry`. Default: `2`. |
| `disabled` | Boolean. Skips step unconditionally. Useful during development. |

#### Runner selection hierarchy

For any given step, the runner is resolved in this order (highest priority first):

1. Per-step `runner:` field in the YAML
2. `AIL_DEFAULT_RUNNER` environment variable
3. Hardcoded fallback: `"claude"` → `ClaudeCliRunner`

```yaml
pipeline:
  - id: analyze
    prompt: "Analyze this code."
    runner: claude        # explicit — uses ClaudeCliRunner

  - id: quick_check
    prompt: "Any obvious issues?"
    # runner not set — uses AIL_DEFAULT_RUNNER or "claude"
```

**`id` is always required.** Because any pipeline may be inherited from via `FROM`, `ail` cannot know at parse time which steps will be targeted by hook operations in inheriting pipelines. Step IDs must be stable identifiers — renaming a step ID in a `FROM`-able pipeline is a breaking change for all inheritors.

### 5.2 `prompt:` Steps — LLM Invocations

A `prompt:` step invokes the LLM with an optional system context and a user-level prompt. It is the only step type that costs tokens.

#### Fields

| Field | Description |
|---|---|
| `prompt` | String or file path. **Required** when `system_prompt:` or `append_system_prompt:` is declared; optional otherwise. Inline text or path to a `.md` file. Path detected by prefix: `./` `../` `~/` `/`. |
| `system_prompt` | String or file path. Sets the full system prompt for this step. Replaces any provider default. |
| `append_system_prompt` | Array. Each entry extends the system context in declared order. See §5.9. |
| `provider` | String. Overrides the default provider for this step. |
| `model` | String. Overrides the default model for this step. |
| `timeout_seconds` | Integer. Maximum seconds to wait. Default: `120`. |
| `on_result` | Array or block. Declarative branching after completion. See §5.4. |
| `before` | List. Private pre-processing steps that run before this step's prompt fires. See §5.10. |
| `then` | List. Private post-processing steps chained to this step. See §5.7. |
| `tools` | Block. Pre-approve or pre-deny tool calls for this step. See §5.8. |
| `resume` | Boolean. When `true`, resumes the most recent preceding session on the same provider. See §15.4. |

**Rule:** A step with `system_prompt:` or `append_system_prompt:` but neither `prompt:` nor `skill:` is a parse error — you are configuring LLM context with no task instruction.

#### Inline and file prompts

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

#### System context fields

```yaml
- id: security_review
  system_prompt: ./prompts/base-system.md                    # sets the base system prompt
  append_system_prompt:
    - file: ./skills/security-reviewer/SKILL.md              # skill content as context (see §5.9, §6)
    - file: ./skills/dry-refactor/SKILL.md                   # built-in skill content
    - "Always flag hardcoded credentials."                   # inline string
    - ./prompts/extra-context.md                             # file, detected by path prefix
  prompt: "{{ step.invocation.response }}"
```

`system_prompt:` sets the base. `append_system_prompt:` layers on top in declared order. Both may be present in the same step.

> **Note:** `skill:` entries are **not** supported in `append_system_prompt:`. To use a skill's `SKILL.md` content as system context, reference the file directly via `file:`. To invoke the skill as an LLM call, use a `skill:` step. See §6.3.

### 5.3 `skill:` Steps — Self-Contained Skill Invocations

A `skill:` step invokes the LLM using the body of a `SKILL.md` file as the task instruction. It is the pipeline equivalent of typing `/skill-name` in the `ail` REPL — self-contained, no additional prompt text required.

```yaml
# Invoke a skill standalone
- id: commit
  skill: ./skills/commit/

# Invoke a built-in ail skill
- id: janitor
  skill: ail/janitor

# Skill with additional system context
- id: security_review
  skill: ./skills/security-reviewer/
  append_system_prompt:
    - "Also check for hardcoded credentials."
    - "{{ step.claude_md.result }}"
```

#### Fields

`skill:` steps support all `prompt:` step fields except `prompt:` itself. The skill's `SKILL.md` body is the task instruction — you cannot also specify a `prompt:`.

| Field | Description |
|---|---|
| `skill` | Path to a skill directory containing `SKILL.md`. **Required.** Accepts same path prefixes as `prompt:` file paths, plus `ail/` for built-in skills. Directory paths auto-discover `SKILL.md`. |
| `system_prompt` | String or file path. Sets the full system prompt for this step. Replaces any provider default. |
| `append_system_prompt` | Array. Each entry extends the system context in declared order. See §5.9. |
| `provider` | String. Overrides the default provider for this step. |
| `model` | String. Overrides the default model for this step. |
| `timeout_seconds` | Integer. Maximum seconds to wait. Default: `120`. |
| `on_result` | Array or block. Declarative branching after completion. See §5.4. |
| `before` | List. Private pre-processing steps. See §5.10. |
| `then` | List. Private post-processing steps. See §5.7. |
| `tools` | Block. Pre-approve or pre-deny tool calls. See §5.8. |
| `resume` | Boolean. Resume most recent preceding session on same provider. See §15.4. |

#### Skill path resolution

| Prefix | Resolution |
|---|---|
| `./` | Relative to the current pipeline file |
| `../` | Parent directory of the current pipeline file |
| `~/` | User home directory |
| `/` | Absolute path |
| `ail/` | Built-in `ail` skill package |

The path must resolve to a directory containing a `SKILL.md` file. If no `SKILL.md` is found, `ail` raises a parse error.

#### REPL invocation

In the `ail` interactive REPL, `/skill-name [args]` is equivalent to a `skill:` step. `ail` resolves the skill via the same discovery order, executes it, and pauses for human review before returning control. Arguments are available as `$ARGUMENTS` within the skill body.

### 5.4 `on_result` — Declarative Branching

`on_result` fires after a step completes. It supports both single-match syntax and multi-branch array syntax.

#### Single-match syntax

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

#### Multi-branch array syntax

Rules are evaluated in declared order; the first match fires. Used when different conditions require different responses.

```yaml
- id: lint
  context:
    shell: "cargo clippy -- -D warnings"
  on_result:
    - exit_code: 0
      action: continue
    - exit_code: any
      action: pause_for_human
      message: "Lint failures. Review before continuing."
```

#### Match operators

| Operator | Meaning |
|---|---|
| `contains: "TEXT"` | Response contains literal string (case-insensitive). |
| `matches: "REGEX"` | Response matches regular expression. |
| `starts_with: "TEXT"` | Response begins with literal string. |
| `is_empty` | Response is blank or whitespace only. |
| `exit_code: N` | Process exit code equals N. Valid on `shell:` sources within `context:` steps only. |
| `exit_code: any` | Any non-zero exit code. Valid on `shell:` sources within `context:` steps only. |
| `always` | Unconditionally fires. |

#### Supported actions

| Action | Effect |
|---|---|
| `continue` | Proceed to next step. Default if `on_result` omitted. |
| `pause_for_human` | Suspend pipeline. Wait for Approve / Reject / Modify. |
| `preview_for_human` | Show transformed prompt alongside original. Human chooses: use transformed, use original, or edit. See §5.10. |
| `use_original` | Discard `before:` transformation. Pass raw prompt to parent step unchanged. Only valid inside a `before:` chain. |
| `abort_pipeline` | Stop immediately, treating the pipeline as failed. Logged to audit trail. |
| `repeat_step` | Re-run this step. Respects `max_retries`. |
| `break` | Exit the current pipeline cleanly. Remaining steps are skipped. Not an error — the pipeline completed successfully with an intentional early exit. In a sub-pipeline, returns control to the caller. |
| `pipeline: <path>` | Conditionally call another pipeline. Equivalent to a `pipeline:` step but triggered by `on_result` match. Follows the same isolation model as §9. |

**`break` vs `abort_pipeline`:**

| Action | Intent | Exit state | Caller behaviour |
|---|---|---|---|
| `break` | Intentional early exit | Success | Sub-pipeline returns cleanly; caller continues |
| `abort_pipeline` | Something went wrong | Failure | Caller's `on_error` fires |

> ⚠️ **Reliability warning — prose matching is best-effort.**
> The `contains`, `matches`, and `starts_with` operators match against free-form LLM text output. LLMs are not deterministic. A step instructed to respond `CLEAN` may respond `CLEAN.`, `Yes, CLEAN`, or `The code is clean` — all of which fail a `contains: "CLEAN"` check. **Prose-based `on_result` branching is not a reliable control flow mechanism.**
>
> **Improving reliability with constrained prompts.** Instruct the model to respond with a single, exact token: `"Answer only with CLEAN or VULNERABILITIES_FOUND, nothing else."` This narrows the output space substantially and makes `contains` checks much more reliable in practice.
>
> For a hard contract, use structured output with `input_schema` (see §22 — Planned Extensions).

### 5.5 `context:` Steps — Deterministic Information Gathering

A `context:` step gathers information deterministically, without invoking an LLM. It costs no tokens. The step's result is available to subsequent steps via `{{ step.<id>.result }}`.

Each `context:` step declares exactly one source — the step `id` is the identifier for the result. To gather multiple independent pieces of information, declare multiple `context:` steps.

```yaml
- id: lint_output
  context:
    shell: "cargo clippy -- -D warnings"
  on_result:
    - exit_code: 0
      action: continue
    - exit_code: any
      action: pause_for_human
      message: "Lint failures. Review before continuing."

- id: test_output
  context:
    shell: "cargo test --quiet"
  on_result:
    - exit_code: 0
      action: continue
    - exit_code: any
      action: pause_for_human
      message: "Tests failing. Review output."
```

#### Sources

The value of `context:` is a single-source map — the key declares the source type and the value is its configuration.

| Source type | Field | Description | Status |
|---|---|---|---|
| Shell command | `shell:` | Executes a shell command. Captures stdout and stderr separately; exposes combined output as `result`. | Implemented |
| MCP tool | `mcp:` | Calls a named tool on a named MCP server. Value is a map with `server:`, `tool:`, and optional `arguments:`. | Planned |

`on_result` is a standard step field (see §5.4) — it applies at the step level. The `exit_code:` operator is valid only on `shell:` sources.

Steps without `on_result` continue past non-zero exit codes by default.

#### Shell execution semantics

| Property | Behaviour |
|---|---|
| **Working directory** | `session.cwd` — the directory `ail` was launched from. |
| **Shell** | `/bin/sh -c <command>`. Command is passed as a single string argument. |
| **Timeout** | Inherits step `timeout_seconds` (default: 120). Timeout is a step error — triggers `on_error`, not `on_result`. |
| **Environment** | Full parent environment inherited. No additional env vars are injected beyond what `ail` itself received. |
| **Output capture** | stdout and stderr captured on separate streams. No size limit in v0.1 — avoid commands that produce unbounded output in a pipeline context. |
| **Security model** | Shell execution runs as the `ail` process user with full filesystem access. Pipeline files are trusted input — do not execute pipelines from untrusted sources. |

Non-zero exit codes are **results**, not errors: they fire `on_result`, not `on_error`. An `on_error` escalation from a `shell:` step means the process failed to start, timed out, or the system could not fork — not that the command returned a non-zero code.

#### Template access

Context results are available from any step that runs after the context step completes.

**`shell:` accessors:**

| Accessor | Value |
|---|---|
| `{{ step.<id>.result }}` | stdout + stderr combined — the default for LLM consumption. No `2>&1` needed. |
| `{{ step.<id>.stdout }}` | Standard output only. |
| `{{ step.<id>.stderr }}` | Standard error only. |
| `{{ step.<id>.exit_code }}` | Process exit code as a string. |

> **Note:** `stdout` and `stderr` are captured on separate streams. `result` is a concatenation (stdout then stderr), not a true interleave — relative ordering between the two streams is not preserved. For diagnostics passed to an LLM this is acceptable; avoid parsing `result` when stream ordering matters.

**`mcp:` accessors:** `{{ step.<id>.result }}` contains the tool output. No `stdout`/`stderr`/`exit_code` accessors apply. MCP error semantics are under active design — see §22.

```yaml
- id: claude_md
  context:
    shell: "cat CLAUDE.md 2>/dev/null || echo ''"

- id: lint
  context:
    shell: "cargo clippy -- -D warnings"
  on_result:
    - exit_code: 0
      action: continue
    - exit_code: any
      action: pause_for_human
      message: "Lint failed (exit {{ step.lint.exit_code }}).\n{{ step.lint.stderr }}"

- id: implement
  append_system_prompt:
    - "{{ step.claude_md.result }}"
  prompt: "{{ step.invocation.prompt }}"
```

### 5.6 Step Output Model


Each `prompt:` step captures its output as `step.<id>.response` — the final text produced, available to subsequent steps via template variables resolved from the pipeline run log.

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

For `context:` steps, the step's output is captured as `{{ step.<id>.result }}`. `shell:` sources additionally expose `stdout`, `stderr`, and `exit_code`. Context steps do not produce a `step.<id>.response`.

### 5.7 `then:` — Private Post-Processing Chains

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
    - prompt: "Summarise the findings in one sentence for the audit log."
      provider: fast
```

#### Mixed short and full form

```yaml
- id: my_step
  prompt: "Generate the feature implementation."
  then:
    - ail/janitor                    # short-form
    - prompt: ./prompts/score.md     # full-form with prompt file
      provider: fast
```

#### `materialize` representation

`then:` steps appear in `materialize` output subordinated under their parent, annotated as private and non-hookable:

```yaml
# origin: [2] .ail.yaml
- id: security_audit
  prompt: "..."
  # then: (private — not hookable)
  #   - id: security_audit::then::0  prompt: ./prompts/cleanup.md
```

#### When not to use `then:`

If a post-processing step needs to:
- Be visible or hookable by inheriting pipelines
- Be referenceable by later steps via `{{ step.<id>.response }}`
- Branch via `on_result`

...it should be a top-level step, not a `then:` entry.

### 5.8 `tools:` — Pre-Approved and Pre-Denied Tool Calls

`tools:` on a `prompt:` step declares which Claude CLI tools are unconditionally allowed or denied before the permission callback is consulted. This eliminates HITL prompts for tools the pipeline author has already deemed safe or unsafe for a given step.

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

### 5.9 `append_system_prompt:` — System Context Composition

`append_system_prompt:` is an ordered array of system context additions. Each entry is appended to the system prompt in declared order. Entries use typed map keys — an unlabeled string is shorthand for `text:`.

To invoke a skill as a self-contained task, use a `skill:` step (§5.3). To load a skill's instructions as system context, use `file:` with an explicit path to the `SKILL.md`.

```yaml
- id: security_review
  append_system_prompt:
    - shell: "git log --oneline -10"               # run command, inject stdout+stderr
    - file: ./skills/security-reviewer/SKILL.md   # load skill instructions explicitly
    - file: ./prompts/extra-context.md             # load any file
    - text: "Always flag hardcoded credentials."   # explicit raw text
    - "Also check for SQL injection."              # unlabeled — same as text:
    - "{{ step.claude_md.result }}"                # template variable in raw text
  prompt: "{{ step.invocation.response }}"
```

#### Entry types

| Entry key | Value | Behaviour | Status |
|---|---|---|---|
| `shell:` | Shell command string | Executes command at step runtime; injects stdout+stderr combined. No `on_result` — use a `context:` step if branching is needed. | Planned |
| `file:` | File path | Reads file at step runtime; injects content. Accepts path prefixes `./` `../` `~/` `/`. | Planned |
| `mcp:` | Map: `server:`, `tool:`, `arguments:` | Calls MCP server tool at step runtime; injects output. | Planned |
| `text:` | Inline string | Appended verbatim. Template variables resolved at runtime. | Planned |
| *(unlabeled)* | Inline string | Shorthand for `text:`. Same behaviour. | Planned |

**`shell:` vs `context:` step:**

| | `context:` step | `append_system_prompt: shell:` |
|---|---|---|
| Result stored as `{{ step.<id>.result }}`? | Yes | No — injected directly |
| `on_result` branching? | Yes | No |
| Referenceable by later steps? | Yes | No |
| Use when | You need stored output or exit-code branching | You want command output in system context |

#### Order matters

Entries are appended in declared order. `system_prompt:` sets the base (if present); `append_system_prompt:` entries layer on top. When multiple skills are loaded, their instructions accumulate in the order declared.

#### `system_prompt:` vs `append_system_prompt:`

| Field | Effect |
|---|---|
| `system_prompt:` | Replaces the full system prompt. Use when you need complete control over what the model sees. |
| `append_system_prompt:` | Extends the existing system prompt. Use for layering skills and context on top of the provider default. |

Both may be present in the same step: `system_prompt:` sets the base, `append_system_prompt:` layers on top.

### 5.10 `before:` — Private Pre-Processing Chains

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
    before:
      - prompt: "Rewrite this prompt for maximum clarity and precision."
        provider: fast
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

**Context gathering.** Retrieve relevant information and inject it as context before the parent step's LLM call:

```yaml
- id: code_critic
  append_system_prompt:
    - skill: ./skills/critic/
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
  - prompt: "Optimise this prompt."  # full-form
    provider: fast
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

#### `use_original` Semantics

`use_original` is only valid inside a `before:` chain. It instructs the pipeline executor to discard the `before:` chain's output and pass the parent step's original prompt unchanged. The `before:` steps still execute and their outputs are recorded in the pipeline run log — transparency is preserved — but they do not affect what the LLM receives.

`use_original` used outside a `before:` chain raises a parse error.

#### `materialize` Representation

`before:` steps appear in `materialize` output subordinated under their parent, annotated as private and non-hookable, above the parent step prompt:

```yaml
# origin: [2] .ail.yaml
- id: security_audit
  # before: (private — not hookable)
  #   - id: security_audit::before::0  prompt: "Optimise this prompt."
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

**Detection:** When `ail materialize` resolves a pipeline that contains a `before:` chain on `invocation` — whether declared directly or inherited via `FROM` — it emits a prominent warning identifying the origin pipeline and noting that prompt transformation is active on every invocation. This warning is rendered in the interactive TUI at session start.

This warning is a **UI-layer concern only**. It is not a parse error, not a lint failure, and is not emitted in headless or agent-driven modes. A pipeline with `before:` on `invocation` and no `preview_for_human` is fully valid — the warning exists to surface the configuration to humans who may not have inspected their full inheritance chain. Requiring `preview_for_human` would make such pipelines incompatible with unattended runs, in direct conflict with the Agent-First Design principle.

**The recommended pattern for `FROM` base pipelines:**

If you use `before:` on `invocation` in a base pipeline, always include `preview_for_human` with `show_original: true` for interactive sessions. Give inheritors the ability to see and reject the transformation. Do not silently transform prompts in shared infrastructure.

#### When Not to Use `before:`

If a pre-processing step needs to:
- Be visible or hookable by inheriting pipelines
- Produce output referenceable by later steps via `{{ step.<id>.response }}`
- Branch via `on_result` in a way that affects the wider pipeline

...it should be a top-level step preceding the parent, not a `before:` entry.

---
