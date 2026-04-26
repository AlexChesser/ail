## 18. Complete Examples

### 18.1 The Simplest Possible Pipeline

> Pinned: this example is mechanically validated by the CI check at
> `ail-core/tests/spec/s31_compact.rs`. The leading `# spec:validate`
> comment opts the block into validation. Other examples in this
> section illustrate aspirational syntax and are documentation-only.

```yaml
# spec:validate
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
    skill: ail/dry_refactor

  - id: security_audit
    condition: if_code_changed
    skill: ail/security_audit
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
    skill: ail/dry_refactor

  - id: security_audit
    provider: frontier
    condition: if_code_changed
    skill: ail/security_audit
    append_system_prompt:
      - file: ./prompts/acme-security-context.md
    on_result:
      contains: "SECURITY_CLEAN"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Security findings require review before this code proceeds."

  - id: commit_checkpoint
    condition: if_code_changed
    skill: ail/commit_checkpoint
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
