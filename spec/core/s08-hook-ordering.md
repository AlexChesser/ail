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

### 8.4 Implementation Status

As of v0.2, the `FROM` inheritance chain implements the onion model for hooks within a single inheritance chain (child hooks wrap base hooks). Multi-layer discovery merging (stacking hooks from `--pipeline`, `.ail.yaml`, `.ail/default.yaml`, and `~/.config/ail/default.yaml`) is deferred to a future release.

---
