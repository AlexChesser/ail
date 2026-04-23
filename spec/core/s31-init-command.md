# §31 The `ail init` Command

**Status:** alpha — implemented in `ail-init` crate.

Scaffolds an ail workspace in the current working directory from one of a
small set of bundled starter templates. `ail init` is implemented in a
dedicated workspace crate (`ail-init`) separate from `ail-core` to keep
scaffolding, HTTP registry work, and template distribution out of the core
library's dependency graph.

## §31.1 Usage

```bash
ail init                      # interactive picker (TTY) OR list templates (non-TTY)
ail init <TEMPLATE>           # install a named template
ail init <ALIAS>              # install via alias (e.g. `ail init oma`)
ail init <TEMPLATE> --force   # overwrite existing files
ail init <TEMPLATE> --dry-run # print the install plan without writing
```

On a TTY, `ail init` with no argument opens an arrow-key picker listing every
bundled template with its aliases and short description. Selection is
confirmed with Enter; Esc / Ctrl-C cancels (exit 0, no files written).

When stdin is not a TTY (CI, pipes, scripts), `ail init` with no argument
prints the template list and exits 0. Callers should pass the template name
explicitly in non-interactive contexts.

## §31.2 Install Target

Every template installs under `$CWD/.ail/`, preserving the template's internal
directory structure. The `.ail/` prefix is a hard rule — it is not configurable
per template. Rationale:

- Discovery rule 3 (§3.1) already picks up `.ail/default.yaml` automatically,
  so the starter template is runnable the moment install completes.
- Templates that ship many files (e.g. superpowers with 10 pipelines +
  prompts) do not pollute the user's project root.
- A single install rule keeps manifest authoring trivial — authors do not
  declare install paths.

Files inside a template's source directory map 1:1 to `$CWD/.ail/<relative-path>`.
The template's manifest file (`template.yaml`) is never installed.

## §31.3 Bundled Templates

Three templates ship with `ail` in v0.3:

| Name | Aliases | Short Description |
|---|---|---|
| `starter` | — | Minimal single-step pipeline for onboarding |
| `superpowers` | — | Reference implementation of obra/superpowers skills as AIL pipelines |
| `oh-my-ail` | `oma` | Multi-agent intent-gate orchestration with four complexity tiers |

Bundled templates are the authoritative content — `ail-init` embeds the
`demo/<name>/` directories via `include_dir!` at compile time. Edits to any
pipeline YAML under `demo/` flow through automatically on the next
`cargo build`. A CI invariant (`bundled_templates_validate` test in the
`ail-init` crate) calls `ail_core::config::load` against every pipeline file
on every test run; schema drift in `ail-core` that isn't reflected in the
demos fails the test before release.

## §31.4 Conflict Semantics

Before writing, `ail init` computes the full install plan and compares every
target path against the filesystem. If any target already exists:

- **Default (no `--force`):** `ail init` exits non-zero with a typed error
  listing every conflicting file. No files are written. The user's existing
  content is preserved.
- **With `--force`:** conflicting files are overwritten. No backups are made.

`--dry-run` always avoids writing and is compatible with `--force` (in which
case the dry-run output marks conflicts as "would overwrite").

## §31.5 Template Manifest (`template.yaml`)

Each bundled template has a `template.yaml` manifest at its source root:

```yaml
name: oh-my-ail                        # required — canonical template name
short_description: >-                  # required — one-line UI string
  Multi-agent intent-gate orchestration with four complexity tiers
aliases: [oma]                         # optional — shorthand names
tags: [advanced, multi-agent]          # optional — future filtering
```

Reserved fields (not yet used; added later without breaking existing manifests):
`exclude` (file-path globs to skip), `install` (target-path mapping —
currently "preserve structure under `.ail/`" is hardcoded).

Manifests not matching this shape abort `ail init` with
`[ail:config/validation-failed]` at `BundledSource::new()` time. The
`bundled_templates_validate` test guarantees every shipped manifest parses.

## §31.6 Extension Points (Internal)

The `ail-init` crate defines a private `TemplateSource` trait with two
methods (`list() -> Vec<TemplateMeta>`, `fetch(&str) -> Option<Template>`).
v0.3 ships a single implementation, `BundledSource`. Future implementations
(local `~/.ail/templates/` directory, remote GitHub-hosted registry) plug in
without touching the install or picker logic. The trait is intentionally not
part of the public API until the second source lands.

## §31.7 Non-Goals

- **Not a plugin discovery mechanism for subcommands.** `ail init` is a
  first-party built-in command; adding a third-party subcommand like
  `ail my-tool` is a separate future concern (see the runner plugin model
  in §19 / r10–r11 for the established precedent).
- **Not a template installer for arbitrary content.** Bundled templates are
  pipeline-YAML-centric (+ accompanying README/prompts). Non-pipeline
  scaffolding (dotfiles, config schemas, IDE integrations) is out of scope.
- **No post-install hooks, parameter prompts, or template parameters in v0.3.**
  Templates are static files. Dynamic scaffolding (à la `dotnet new`) is a
  potential future extension via a `post_actions:` manifest field.
