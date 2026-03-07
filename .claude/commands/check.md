Run the full pre-commit quality gate: build, lint, format, and tests.

```bash
cargo build 2>&1 && cargo clippy -- -D warnings 2>&1 && cargo fmt --check 2>&1 && cargo nextest run 2>&1
```

All four must pass. Expected test result: 56 pass, 14 skipped (`#[ignore]`).

If anything fails, stop and fix before reporting success.
