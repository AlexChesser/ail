# §33 URL-Based Template Source

**Status:** alpha — implemented in `ail-init` (`source::url::UrlSource`).

Extends §32 with a second template source: `ail init <URL>` installs a template
fetched over HTTP from a manifest URL. The bundled source (§32.3) is unchanged
and continues to handle every plain-name argument.

This is the **direct-URL install primitive**. An online registry that serves a
catalog of templates is the tier above this and is out of scope for v0.4 (see
§33.7).

## §33.1 Usage

```bash
ail init github:owner/repo                          # default ref (HEAD), root
ail init github:owner/repo@v1.2                     # specific ref, root
ail init github:owner/repo@v1/templates/basic       # specific ref + subpath
ail init github:owner/repo/templates/basic          # HEAD + subpath
ail init https://github.com/owner/repo              # full web URL
ail init https://github.com/owner/repo/tree/main/templates/basic
ail init https://raw.githubusercontent.com/owner/repo/v1/basic/template.yaml
ail init https://example.com/my/templates/basic/template.yaml     # any static host
ail init <URL> --dry-run                            # plan-only, same as bundled
ail init <URL> --force                              # overwrite, same as bundled
```

Dispatch rule (§33.2): arguments containing `://` or starting with `github:`
route through `UrlSource`; every other argument falls through to the bundled
flow.

Installed templates land under `$CWD/.ail/` using the same rule as bundled
templates (§32.2). `--force` / `--dry-run` / conflict semantics (§32.4) apply
identically.

## §33.2 URL Grammar

### GitHub shorthand

```
github:<owner>/<repo>[@<ref>][/<subpath>]
```

- `owner`, `repo`: alphanumerics, `-`, `_`, `.`. Non-empty.
- `ref`: a tag, branch, or SHA. Defaults to `HEAD` (repo default branch) when
  omitted. Refs containing `/` (e.g. `feature/foo`) are not expressible in
  either shorthand or the `/tree/<ref>` web URL — both split on `/`. Users
  who need them must point directly at a raw manifest URL (see *Generic
  manifest URL* below).
- `subpath`: `/`-separated directory path from the repo root. May not start
  with `/`, end with `/`, contain `..` or `.` segments, or contain empty
  segments.

### GitHub web URL

```
https://github.com/<owner>/<repo>
https://github.com/<owner>/<repo>/tree/<ref>[/<subpath>]
```

Same `owner` / `repo` / `ref` / `subpath` rules as the shorthand. Paths other
than `/tree/...` after the repo are rejected.

### Generic manifest URL (escape hatch)

```
https://<host>/<...>/template.yaml
```

Any `http(s)` URL whose path ends in `/template.yaml` is accepted. The parent
directory of the manifest becomes the *base URL* for fetching listed files.
This is how `raw.githubusercontent.com/.../template.yaml` works and is the
supported way to target a non-GitHub host (self-hosted, GitLab raw, object
storage, etc.) or a ref containing `/`.

Any other URL shape (unknown scheme, no `template.yaml` suffix, malformed)
returns `ail:init/failed` with a `url-invalid:` detail prefix.

## §33.3 Fetch Pipeline

1. Parse the argument → `UrlTemplateRef { manifest_url, base_url, label }`.
2. `GET manifest_url`. UTF-8 decode, parse as `template.yaml` (§32.5).
3. Require a non-empty `files:` list (§32.5.1). Reject every listed entry that
   fails the path-safety checks in the fetcher.
4. For each entry: `GET base_url + <entry>`, enforce per-file and cumulative
   byte caps, build a `TemplateFile` in memory.
5. Hand the resulting `Template { meta, files }` to the same install plan /
   apply pipeline the bundled source uses (§32.2, §32.4).

Caps (enforced by the fetcher; all exceedances map to `limit-exceeded:`):

| Control | Limit |
|---|---|
| Manifest body                | 1 MiB |
| Per-file body                | 2 MiB |
| Cumulative download          | 16 MiB |
| File count in `files:`       | 128 |
| Connect timeout              | 10 s |
| Read timeout (per request)   | 30 s |

`--dry-run` still fetches (a URL-sourced template has no in-process
representation until it's fetched), but does not write to disk.

## §33.4 Security Envelope

Enforced in every URL install:

| Risk | Enforcement |
|---|---|
| Path traversal (`..`, absolute, leading `/`) in `files:` | Rejected at parse time; double-checked before every file URL join. `files-unsafe:`. |
| Backslashes, null bytes, Windows drive prefixes (`C:/...`) in `files:` | Rejected. `files-unsafe:`. |
| `template.yaml` listed in its own `files:` (manifests are never installed content) | Rejected. `files-unsafe:`. |
| Duplicate entries in `files:` | Rejected. `manifest-invalid:`. |
| Oversized response body (manifest, per-file, cumulative) | Connection aborted mid-read. `limit-exceeded:`. |
| File-count blow-up | `limit-exceeded:`. |
| TLS / certificate verification | `ureq` defaults (rustls-tls); verification always on, no overrides. |
| Private-repo auth | **Out of scope.** `UrlSource` carries a `token: Option<String>` seam that is always `None` in v0.4. |
| Symlinks / special tar entries | N/A — no archive extraction; every file is fetched individually. |

## §33.5 Manifest Contract for URL Sources

Cross-references §32.5.1. A URL-sourced manifest MUST declare `files:`. The
absence of `files:` is reported as `manifest-invalid:` (not `not-found:` or
`manifest-missing:`) because the HTTP request succeeded and the YAML parsed —
the file inventory was what was missing.

Ordering within `files:` is preserved in the install plan, and duplicate
entries are rejected at parse time.

## §33.6 Error Surface

All URL-install failures are `AilError::InitFailed` (`ail:init/failed`). The
first tag of the detail string identifies the subclass and is stable across
releases:

| Detail prefix | Cause |
|---|---|
| `url-invalid:` | Argument was URL-shaped but did not parse (bad grammar, unsupported scheme, missing host, non-`template.yaml` fallback URL) |
| `transport:` | Connection failure, DNS failure, timeout, HTTP 5xx, non-specific non-2xx |
| `not-found:` | HTTP 404 on manifest or on any listed file |
| `rate-limited:` | HTTP 429, or HTTP 403 accompanied by a `retry-after` / `x-ratelimit-*` header |
| `manifest-invalid:` | Manifest parsed but is missing `files:`, is not valid UTF-8, has an empty or duplicate entry in `files:`, or otherwise fails §32.5 |
| `files-unsafe:` | A `files:` entry failed the path-safety checks |
| `limit-exceeded:` | Any byte cap (manifest / per-file / cumulative) or the file-count cap was exceeded |

Downstream tooling (VS Code extension, NDJSON consumers) may pattern-match on
these prefixes; they are part of the contract.

## §33.7 Non-Goals

- **No online registry.** `ail init <URL>` installs a single template from a
  manifest URL. A discoverable catalog of URL templates (e.g. `ail init
  --search authoring`) is a future concern that this primitive supports but
  does not itself provide.
- **No private-repo auth.** The `token: Option<String>` seam is reserved; no
  CLI surface exposes it in v0.4.
- **No cache.** Every `ail init <URL>` is a fresh fetch. Caching would be a
  decorator over `UrlSource::fetch_url` if added later; `--refresh` would be
  the explicit-invalidate flag. Not needed now because init is not a hot path.
- **No archive transport.** Per-file fetch was chosen over tarball extraction
  because AIL templates are small (a handful of YAML files). Revisit if
  template sizes grow enough that the extra round-trips become noticeable.
- **`UrlSource::list()` is absent by design.** A direct URL has nothing to
  enumerate. `ail init` with no argument continues to list only the bundled
  catalog (§32.1).
