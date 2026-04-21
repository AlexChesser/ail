# ail Chat

A streaming chat interface for [ail](https://github.com/alexchesser/ail) pipelines inside VS Code.

ail orchestrates multi-step AI workflows defined in `.ail.yaml` files. This extension lets you run those pipelines from a chat sidebar, watch step-by-step execution in real time, and inspect the pipeline graph without leaving the editor.

## Features

- **Chat sidebar** — send prompts to your pipeline and watch streaming responses, tool calls, and thinking blocks as they arrive.
- **Pipeline graph panel** — visualise pipeline structure with collapsible sub-pipelines and per-step detail.
- **Session management** — switch between sessions, fork from prior runs, or start fresh.
- **Bundled `ail` binary** — platform-specific VSIXes include the `ail` runtime. No Rust toolchain needed.

## Installation

Download the `.vsix` for your platform from the [GitHub Releases](https://github.com/alexchesser/ail/releases) page and install it:

```bash
code --install-extension ail-<platform>-<version>.vsix
```

Or from the VS Code UI: **Extensions** → **⋯** → **Install from VSIX…**

## Commands

| Command | Description |
|---|---|
| `ail Chat: Open Chat` | Focus the chat sidebar |
| `ail Chat: New Session` | Start a fresh session |
| `ail Chat: Open Pipeline Graph` | Open the pipeline graph panel |

## Configuration

| Setting | Default | Description |
|---|---|---|
| `ail-chat.binaryPath` | `""` | Path to the `ail` binary. Empty uses the bundled binary or `PATH`. |
| `ail-chat.defaultPipeline` | `""` | Path to the default pipeline file. Empty lets ail discover it automatically. |
| `ail-chat.headless` | `false` | Run ail with `--headless` (`--dangerously-skip-permissions`). Trusted workspaces only. |

## Pipeline Discovery

The extension uses ail's built-in pipeline discovery order:

1. `ail-chat.defaultPipeline` setting
2. `.ail.yaml` in the workspace root
3. `.ail/default.yaml` in the workspace root
4. `~/.config/ail/default.yaml`

If nothing is found, ail runs in passthrough mode (your prompt goes straight to the underlying agent with no declared pipeline steps).

## License

MIT. See [LICENSE](./LICENSE) for details. Other components of the `ail` project are under different licenses — see the [main repository](https://github.com/alexchesser/ail) for the full table.
