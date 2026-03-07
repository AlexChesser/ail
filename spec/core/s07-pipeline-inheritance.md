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

The full resolved chain is inspectable via `ail materialize` (§18).

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
