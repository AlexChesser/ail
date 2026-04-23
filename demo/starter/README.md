# Starter Pipeline

A minimal AIL pipeline with a single explicit invocation step. Start here if you're new to AIL.

## What's installed

```
.ail/
└── default.yaml    ← your pipeline (discovered automatically by ail and the VS Code extension)
```

## Next steps

1. **Run a prompt** — open the ail Chat sidebar and type anything. The pipeline runs transparently.
2. **Inspect the log** — click a run in the "Run History" panel to see exactly what happened.
3. **Add a step** — uncomment the example steps in `default.yaml` to see how chaining works.
4. **Explore templates** — check out the [Oh My AIL](https://github.com/AlexChesser/ail/tree/main/demo/oh-my-ail) and [Superpowers](https://github.com/AlexChesser/ail/tree/main/demo/superpowers) demos for more advanced orchestration.

## How AIL works

```
you type a prompt
    └─▶ invocation step  (your prompt → agent → response)
            └─▶ step 2   (optional — runs automatically)
                    └─▶ step 3 ...
                            └─▶ response returned to you
```

Every step in the pipeline runs before you see the final response. Steps can read files, run shell commands, call LLMs, or invoke skills — in any combination.

## Reference

- [AIL spec](https://github.com/AlexChesser/ail/tree/main/spec/core)
- [Pipeline YAML reference](https://github.com/AlexChesser/ail/blob/main/spec/core/s03-pipeline-file.md)
- [Template variables](https://github.com/AlexChesser/ail/blob/main/spec/core/s11-template.md)
