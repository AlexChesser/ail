Run all linting and formatting checks. Both must be clean before committing.

```bash
cargo clippy -- -D warnings 2>&1 && cargo fmt --check 2>&1
```

Rules:
- `clippy::result_large_err` is suppressed with `#[allow]` in `validation.rs`, `config/mod.rs`, `runner/mod.rs`, `template.rs`, and `executor.rs` — this is intentional, do not remove those attributes.
- If `fmt --check` fails, run `cargo fmt` to fix formatting, then re-check.
- All other clippy warnings must be fixed, not suppressed.
