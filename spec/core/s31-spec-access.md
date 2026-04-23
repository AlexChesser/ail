## 31. Specification Access & Injection

> **Implementation status:** Fully implemented. `ail spec` CLI command, `context: spec:` context source, and `append_system_prompt: - spec:` system prompt entry all functional.

### 31.1 Purpose

The AIL specification is compiled into the binary as a set of embedded resources. This enables pipelines to inject spec content into LLM context windows at runtime — supporting the use case of **pipelines that design their own pipelines**.

Since no LLM has been trained on AIL, spec injection provides the LLM with the knowledge it needs to generate correct `.ail.yaml` files.

### 31.2 Tiers

Three compression tiers trade detail for token budget:

| Tier | Name | Tokens | Contents | Use case |
|---|---|---|---|---|
| T1 | `schema` | ~2-3K | Annotated YAML schema with inline constraints | Syntax reminder for someone who roughly knows AIL |
| T2 | `compact` | ~10-12K | Compressed NL — all rules, no examples | Teach an LLM enough to write correct pipelines |
| T3 | `prose` | ~80K | Full specification verbatim | Deep reference for edge cases and rationale |

T1 and T2 are hand-authored checked-in files under `spec/compressed/`. T3 is the raw concatenation of all `spec/core/s*.md` and `spec/runner/r*.md` files.

### 31.3 CLI Command — `ail spec`

```
ail spec                          # full spec (T3 prose, default)
ail spec --format schema          # T1 annotated YAML schema
ail spec --format compact         # T2 compressed NL reference
ail spec --format prose           # T3 full prose (explicit)
ail spec --section s05            # one section (prose)
ail spec --section s05,s11,r02    # multiple sections (comma-separated)
ail spec --list                   # section IDs, titles, word counts
ail spec --core                   # core sections only
ail spec --runner                 # runner sections only
```

Output goes to stdout and is pipeable.

### 31.4 Pipeline Integration — `context: spec:`

A new context source type injects embedded spec content as a step result:

```yaml
pipeline:
  - id: learn_ail
    context:
      spec: compact              # injects T2 as {{ step.learn_ail.result }}
```

Accepted query values: `compact`, `schema`, `prose`, `core`, `runner`, or any section ID (`s05`, `r02`, etc.).

The step produces a `TurnEntry` with the spec content in `stdout` and `exit_code: 0`. All standard context step template variables apply (`{{ step.<id>.result }}`, `{{ step.<id>.stdout }}`).

### 31.5 Pipeline Integration — `append_system_prompt: - spec:`

A new system prompt entry type injects spec content into the LLM's system prompt:

```yaml
pipeline:
  - id: design_pipeline
    prompt: "Design an AIL pipeline for: {{ step.invocation.prompt }}"
    append_system_prompt:
      - spec: compact             # inject T2 into system prompt
      - spec: s05                 # inject specific section
```

Accepts the same query values as `context: spec:`.

### 31.6 Zero-Code Fallback

The CLI command can be used with existing `context: shell:` steps without any executor changes:

```yaml
pipeline:
  - id: learn_ail
    context:
      shell: "ail spec --format compact"
```

### 31.7 Architecture

Spec content is compiled into the `ail-spec` crate via `build.rs` at build time. The crate:

- Scans `spec/core/s*.md` and `spec/runner/r*.md` using `include_str!`
- Embeds T1 from `spec/compressed/schema.yaml` and T2 from `spec/compressed/compact.md`
- Exposes a public API: `section(id)`, `list_sections()`, `full_prose()`, `compact()`, `schema()`
- Automatically picks up new spec files on rebuild (no Rust code changes needed)

Dependency chain: `ail-spec` → `ail-core` (for `context: spec:` and `append_system_prompt: - spec:`) → `ail` (for the CLI command).
