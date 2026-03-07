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
