# AIL Server API

> **ail serve** — run `ail` as an HTTP server, exposing pipeline execution as a fully-documented, OpenAPI 3.1-described REST API with Server-Sent Events streaming and a built-in web UI.

---

> ⚠️ **This document describes a planned feature.**
>
> `ail serve` is targeted for v0.2. The endpoint surface and schema design documented here are intended to drive implementation, not describe existing behaviour. Shape may change before release based on implementation findings. See `ARCHITECTURE.md §12` for the full design rationale.
>
> The canonical, always-current specification is the live OpenAPI JSON served by a running `ail serve` instance at `/api/v1/openapi.json`. This document is the human-readable companion to that machine-readable spec.

---

## Table of Contents

1. [Quick Start](#1-quick-start)
2. [Base URL and Versioning](#2-base-url-and-versioning)
3. [Authentication](#3-authentication)
4. [Core Concepts](#4-core-concepts)
5. [Sessions](#5-sessions)
6. [Turns](#6-turns)
7. [Streaming — Server-Sent Events](#7-streaming--server-sent-events)
8. [HITL Gates](#8-hitl-gates)
9. [Pipelines](#9-pipelines)
10. [Error Responses](#10-error-responses)
11. [SDK Generation](#11-sdk-generation)
12. [Deployment](#12-deployment)

---

## 1. Quick Start

```bash
# Start the server
ail serve --port 7823

# Open the web UI
open http://localhost:7823/ui

# Or use the API directly
curl -X POST http://localhost:7823/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{"pipeline_path": "./my-pipeline.ail.yaml"}'

# Response
{
  "session_key": "ses_abc123",
  "status": "ready",
  "pipeline": "my-pipeline.ail.yaml",
  "created_at": "2026-03-06T12:00:00Z"
}
```

---

## 2. Base URL and Versioning

All endpoints are prefixed with `/api/v1`. The version segment is part of the URL, not a header. When `ail serve` releases v2 of the API, `/api/v1` continues to be served until an explicit deprecation window closes.

| Environment | Base URL |
|---|---|
| Local default | `http://localhost:7823/api/v1` |
| Custom port | `http://localhost:{PORT}/api/v1` |
| Self-hosted | `https://your-host.example.com/api/v1` |
| Hosted (planned) | `https://api.ail.sh/v1` |

The OpenAPI spec is always available at `{BASE_URL}/openapi.json` and as a human-readable UI at `{HOST}/ui`.

---

## 3. Authentication

In its default local mode, `ail serve` runs without authentication — it binds to `localhost` and is intended for single-user development use.

For multi-user or networked deployments, token-based authentication is available via the `--auth-token` flag or `AIL_SERVER_TOKEN` environment variable:

```bash
ail serve --auth-token my-secret-token
```

All requests must then include:

```
Authorization: Bearer my-secret-token
```

A more complete authentication model (OAuth2, API keys, team management) is planned for the hosted offering at `api.ail.sh`. The local server's simple token model is intentional — it is not the authentication surface for the hosted product.

---

## 4. Core Concepts

**Session** — a live connection between the server and a pipeline + runner. A session holds state across multiple turns: the runner instance, the in-memory tool allowlist, the turn log, and any pending HITL gates. Sessions have a key that can be used to resume after interruption.

**Turn** — one complete pipeline execution within a session. A turn is created by submitting a human prompt. It progresses through `before:` chains, the runner invocation, `on_result` branching, and `then:` chains before completing. A turn's final state includes the full response, every step's output, cost data, and the complete event sequence.

**HITL Gate** — a point in pipeline execution where human (or agent) input is required before execution can continue. Gates are created when a tool permission is unresolved, a `pause_for_human` action fires, or a `preview_for_human` action presents a transformed prompt. Gates are surfaced via the `/hitl` endpoints and the SSE stream.

**Pipeline Event** — a typed, timestamped event emitted by `ail-core` during pipeline execution. Events are the lingua franca of the system — the TUI subscribes to them, the SSE stream serialises them, and the turn log records them. See §7 for the full event catalogue.

---

## 5. Sessions

### Create a Session

```
POST /api/v1/sessions
```

**Request body:**

```json
{
  "pipeline_path": "./my-pipeline.ail.yaml",
  "runner": "claude-cli",
  "session_key": "my-named-session"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `pipeline_path` | string | no | Path to a `.ail.yaml` file. Defaults to pipeline discovery order from SPEC §3.1. |
| `pipeline_inline` | object | no | Inline pipeline definition. Alternative to `pipeline_path`. |
| `runner` | string | no | Runner to use. Default: configured default runner. |
| `session_key` | string | no | Named key for this session. If omitted, a key is generated. Useful for resumption. |

**Response `201 Created`:**

```json
{
  "session_key": "ses_abc123",
  "status": "ready",
  "pipeline": {
    "name": "my-pipeline",
    "path": "./my-pipeline.ail.yaml",
    "step_count": 4
  },
  "runner": "claude-cli",
  "created_at": "2026-03-06T12:00:00Z"
}
```

### Get Session State

```
GET /api/v1/sessions/{key}
```

Returns current session state including status, pending HITL gates, turn count, and accumulated cost.

### End a Session

```
DELETE /api/v1/sessions/{key}
```

Flushes the turn log, terminates the runner process, and releases all session resources. Returns `204 No Content`.

---

## 6. Turns

### Submit a Prompt

```
POST /api/v1/sessions/{key}/turns
```

Submits a human prompt to the session. The server runs the full pipeline — `before:` chains, runner invocation, `on_result` branching, `then:` chains — and returns when the pipeline completes or suspends on a HITL gate.

**Request body:**

```json
{
  "prompt": "refactor the auth module for DRY compliance",
  "wait": true
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `prompt` | string | required | The human prompt to submit. |
| `wait` | boolean | `true` | If true, blocks until pipeline completes or a HITL gate suspends it. If false, returns immediately with a `turn_id` and the caller polls or streams. |

**Response `201 Created` (pipeline completed):**

```json
{
  "turn_id": "trn_xyz789",
  "status": "completed",
  "response": "I've refactored the auth module...",
  "steps": [
    {
      "step_id": "main",
      "status": "completed",
      "response": "I've refactored the auth module...",
      "cost_usd": 0.012
    },
    {
      "step_id": "security_audit",
      "status": "completed",
      "response": "No security issues found.",
      "cost_usd": 0.004
    }
  ],
  "total_cost_usd": 0.016,
  "duration_ms": 6241,
  "created_at": "2026-03-06T12:00:01Z",
  "completed_at": "2026-03-06T12:00:07Z"
}
```

**Response `202 Accepted` (suspended on HITL gate):**

```json
{
  "turn_id": "trn_xyz789",
  "status": "suspended",
  "suspended_on": {
    "gate_id": "gate_001",
    "gate_type": "tool_permission",
    "tool": "WebFetch",
    "tool_input": { "url": "https://example.com" }
  },
  "message": "Pipeline suspended pending approval. POST to /hitl/gate_001 to continue."
}
```

### Get Turn Detail

```
GET /api/v1/sessions/{key}/turns/{turn_id}
```

Returns the complete turn record including all step outputs, every pipeline event in the turn log, cost breakdown, and timing.

### List Turns

```
GET /api/v1/sessions/{key}/turns
```

Returns a paginated list of completed turns for the session, newest first.

---

## 7. Streaming — Server-Sent Events

```
GET /api/v1/sessions/{key}/stream
```

Opens an SSE stream that emits typed pipeline events in real time. The stream stays open for the lifetime of the session. Clients subscribe once and receive events for all subsequent turns.

**Connection:**

```javascript
const stream = new EventSource(
  `http://localhost:7823/api/v1/sessions/${key}/stream`
);

stream.addEventListener('step.completed', (e) => {
  const data = JSON.parse(e.data);
  console.log(`Step ${data.step_id} completed. Cost: $${data.cost_usd}`);
});
```

### Event Catalogue

All events carry `session_key`, `turn_id` (once a turn is active), and `timestamp`. Additional fields are event-specific.

| Event type | When | Key fields |
|---|---|---|
| `session.ready` | Session created and runner ready | `session_key`, `pipeline` |
| `turn.started` | Prompt submitted, pipeline beginning | `turn_id`, `prompt` |
| `step.started` | A pipeline step begins executing | `step_id`, `step_index` |
| `step.text_delta` | Streaming text fragment from runner | `step_id`, `delta` |
| `step.tool_use` | Runner invoked a tool | `step_id`, `tool`, `tool_input` |
| `step.tool_result` | Tool returned a result | `step_id`, `tool`, `result` |
| `step.completed` | Step finished | `step_id`, `response`, `cost_usd` |
| `step.skipped` | Step condition evaluated false | `step_id`, `condition` |
| `step.failed` | Step errored | `step_id`, `error` |
| `hitl.gate_opened` | Pipeline suspended; input required | `gate_id`, `gate_type`, details |
| `hitl.gate_resolved` | HITL gate answered; pipeline resuming | `gate_id`, `resolution` |
| `turn.completed` | All steps finished | `turn_id`, `total_cost_usd`, `duration_ms` |
| `turn.failed` | Pipeline aborted | `turn_id`, `error` |
| `session.ended` | Session terminated | `session_key` |

---

## 8. HITL Gates

HITL gates suspend pipeline execution and wait for a response before continuing. In `ail serve`, gates are surfaced via the API rather than a TUI prompt. An agent or human responds via HTTP.

### List Pending Gates

```
GET /api/v1/sessions/{key}/hitl/pending
```

Returns all unresolved HITL gates for the session. A suspended pipeline remains suspended until the gate is resolved.

### Respond to a Gate

```
POST /api/v1/sessions/{key}/hitl/{gate_id}
```

**Tool permission gate:**

```json
{ "behavior": "allow" }
```
```json
{ "behavior": "deny", "message": "Not permitted in this context" }
```
```json
{ "behavior": "allow", "updated_input": { "url": "https://safe-mirror.example.com" } }
```

**Prompt preview gate (`preview_for_human`):**

```json
{ "choice": "use_transformed" }
```
```json
{ "choice": "use_original" }
```
```json
{ "choice": "use_edited", "edited_prompt": "Please refactor the authentication module." }
```

**Pause gate (`pause_for_human`):**

```json
{ "choice": "approve" }
```
```json
{ "choice": "reject", "message": "Do not proceed with this change." }
```

Resolving a gate resumes pipeline execution. The response includes the updated turn status.

---

## 9. Pipelines

### Validate a Pipeline

```
POST /api/v1/pipelines/validate
```

Parses and validates a `.ail.yaml` definition without executing it. Returns any parse errors with span information.

**Response `200 OK` (valid):**

```json
{
  "valid": true,
  "step_count": 4,
  "warnings": []
}
```

**Response `422 Unprocessable Entity` (invalid):**

```json
{
  "valid": false,
  "errors": [
    {
      "error_type": "ail:config/missing-field",
      "title": "Required field missing",
      "detail": "Step at index 2 is missing a required primary field (prompt, skill, or action)",
      "context": { "step_index": 2, "line": 14 }
    }
  ]
}
```

### Materialize a Pipeline

```
POST /api/v1/pipelines/materialize
```

Resolves the full `FROM` inheritance chain and returns the complete materialised pipeline as it will actually execute. Equivalent to `ail materialize-chain` from the CLI.

---

## 10. Error Responses

All error responses follow the RFC 9457 Problem Details format, adapted for `ail`. See `ARCHITECTURE.md §2.8` for the design rationale.

```json
{
  "error_type": "ail:session/not-found",
  "title": "Session not found",
  "detail": "No session with key 'ses_abc123' exists. Sessions expire after 24 hours of inactivity.",
  "context": {
    "session_key": "ses_abc123"
  }
}
```

| HTTP status | When |
|---|---|
| `400 Bad Request` | Malformed request body |
| `401 Unauthorized` | Missing or invalid auth token |
| `404 Not Found` | Session, turn, or gate not found |
| `409 Conflict` | Turn submitted while a HITL gate is pending |
| `422 Unprocessable Entity` | Request is well-formed but semantically invalid |
| `503 Service Unavailable` | Runner is not available or crashed |

---

## 11. SDK Generation

The OpenAPI spec is always current and always available from a running `ail serve` instance. SDK generation requires only the spec and an OpenAPI generator.

### Generate a Client

```bash
# Get the spec
ail serve --openapi > ail-openapi.json

# Python
openapi-generator generate \
  -i ail-openapi.json -g python \
  --package-name ail_client -o ./sdk/python

# TypeScript
openapi-generator generate \
  -i ail-openapi.json -g typescript-fetch \
  --additional-properties=npmName=ail-client -o ./sdk/typescript

# C#
openapi-generator generate \
  -i ail-openapi.json -g csharp \
  --package-name Ail.Client -o ./sdk/csharp

# Go
openapi-generator generate \
  -i ail-openapi.json -g go \
  --package-name ailclient -o ./sdk/go
```

Or generate directly from the hosted spec without cloning the repo:

```bash
openapi-generator generate \
  -i https://api.ail.sh/openapi.json \
  -g python -o ./my-app/ail_client
```

### Using a Generated Python Client

```python
from ail_client import AilClient, CreateSessionRequest, CreateTurnRequest
from ail_client.models import HitlResponse

client = AilClient(base_url="http://localhost:7823")

session = client.sessions.create(CreateSessionRequest(
    pipeline_path="./team-pipeline.ail.yaml"
))

# Subscribe to the live event stream
async for event in client.sessions.stream(session.session_key):
    if event.type == "step.completed":
        print(f"Step {event.step_id}: {event.response[:80]}...")
    elif event.type == "hitl.gate_opened":
        # Autonomous agent: auto-approve read-only tools
        if event.gate_type == "tool_permission" and event.tool in ("Read", "Glob"):
            client.sessions.hitl.respond(
                session.session_key, event.gate_id,
                HitlResponse(behavior="allow")
            )

# Or just submit and wait synchronously
turn = client.sessions.turns.create(
    session.session_key,
    CreateTurnRequest(prompt="write unit tests for src/auth.rs")
)
print(turn.response)
print(f"Total cost: ${turn.total_cost_usd:.4f}")
```

### Official SDKs (Planned)

| Language | Package | Target |
|---|---|---|
| Python | `ail-client` on PyPI | v0.3 |
| TypeScript / Node | `ail-client` on npm | v0.3 |
| Go | `github.com/ail-sh/ail-go` | v0.4 |

Official SDKs add typed SSE stream helpers, retry logic, HITL gate helpers for common autonomous agent patterns, and integration test utilities. Community-maintained SDKs for other languages can be generated from the published spec at any time.

---

## 12. Deployment

### Local Development

```bash
ail serve
# Listening on http://localhost:7823
# UI: http://localhost:7823/ui
# Spec: http://localhost:7823/api/v1/openapi.json
```

### Docker

```bash
docker run -p 7823:7823 \
  -v $(pwd)/pipelines:/pipelines \
  -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \
  ghcr.io/ail-sh/ail serve \
  --pipeline /pipelines/team.ail.yaml \
  --bind 0.0.0.0
```

The Docker image is built `FROM scratch` with only the statically-linked binary and a TLS certificate bundle. Target image size: under 20MB.

### Behind a Reverse Proxy

`ail serve` does not handle TLS termination. Run it behind nginx, Caddy, or Cloudflare Tunnel:

```
# Caddyfile
ail.internal.example.com {
    reverse_proxy localhost:7823
}
```

### Environment Variables

| Variable | Description |
|---|---|
| `AIL_SERVER_PORT` | Port to bind (default: `7823`) |
| `AIL_SERVER_BIND` | Bind address (default: `127.0.0.1`) |
| `AIL_SERVER_TOKEN` | Auth token for protected deployments |
| `AIL_SERVER_PIPELINE` | Default pipeline path |
| `AIL_LOG_LEVEL` | Log level: `error` `warn` `info` `debug` `trace` (default: `info`) |
| `ANTHROPIC_API_KEY` | Passed through to the Claude CLI runner |

---

*`API.md` documents the intended design for `ail serve`. The authoritative, implementation-derived specification is always the OpenAPI JSON served live at `/api/v1/openapi.json`. When in doubt, the live spec is correct and this document should be updated to match.*
