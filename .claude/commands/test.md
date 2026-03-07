Run the full test suite and report results.

```bash
cargo nextest run 2>&1
```

Expected baseline: 56 pass, 14 skipped (`#[ignore]`).

The skipped tests are `ClaudeCliRunner` integration tests that require a live `claude` CLI session and cannot run inside Claude Code. To run them manually outside this session:

```bash
cargo nextest run --include-ignored
```

If any non-ignored tests fail, investigate and fix before proceeding.
