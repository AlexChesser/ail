Run the full pre-commit quality gate: build, lint, format, and tests.

```bash
cargo build 2>&1 && cargo clippy -- -D warnings 2>&1 && cargo fmt --check 2>&1 && cargo nextest run 2>&1
```

All four must pass. Expected test result: 56 pass, 14 skipped (`#[ignore]`).

Also run the vscode-ail-chat quality gate (requires Node 24):

```bash
source ~/.nvm/nvm.sh && nvm use 24 && cd vscode-ail-chat && npm run check
```

Expected: 0 TypeScript errors, lint clean, all vitest tests passing.

If anything fails, stop and fix before reporting success.
