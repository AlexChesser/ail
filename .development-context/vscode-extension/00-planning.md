# Planning Prompt: VSCode Extension for `ail`

## Context

You are planning a VSCode extension that brings interactive `ail` orchestration capabilities directly into the developer's workspace. This extension should allow users to:
- Define, inspect, and execute `ail` pipelines from within VSCode
- Monitor live pipeline execution with real-time status visibility
- Interrupt, pause, and resume pipelines with HITL (human-in-the-loop) gate management
- Navigate and edit `.ail.yaml` pipeline definitions with syntax awareness
- Interact with the `ail` orchestrator in both headless and interactive modes
- Load and Switch active pipeline
- "pause" pipeline individual bullets at runtime (automatic skip -> patthrough to next step). This is a current-session only step. Reloading or closing the window will set the prompt back to full mode.   
- edit a pipeline file -> hot releoad on file change

## Architectural Constraints

`ail` is **not an agent itself**—it is an orchestrator that runs other CLIs as runners. The extension must respect this position:
- The extension is a UI layer that communicates with `ail` via its existing CLI interface
- `ail` will run in **headless mode** (non-interactive background process) when invoked by the extension
- The extension itself becomes the **interactive permissions arbiter**—user confirmations, interrupts, and decisions are made in the VSCode UI and forwarded to `ail`
- No native agent capability is required or desired in the extension

## Key Design Areas to Plan

### 1. Headless Mode & CLI Integration

**What headless mode means for this extension:**
- When the extension launches an `ail` pipeline, it invokes `ail` as a subprocess with a flag/mode that suppresses interactive prompts and TUI
- The orchestrator runs silently, outputting structured data (JSON, event logs) that the extension consumes
- All user decisions (pause/resume, step skip, pipeline switch, HITL gate approval) are triggered from the VSCode UI and sent back to `ail` via a control channel (IPC, signals, or API)

**Planning questions to address:**
- How should `ail` expose a headless execution mode? (e.g., `ail run --headless --output-format json`, or a separate headless server mode?)
- What is the control channel architecture? (stdin/stdout streams, Unix sockets, a lightweight local API, message queues?)
- How does `ail` report events (step started, step completed, HITL gate encountered, error) in headless mode?

### 2. Interactive Permissions & HITL Gate Management

**Interactive permissions** = the extension acting as the gatekeeper for decisions that require human approval:
- Approving/rejecting destructive actions (e.g., modifying live state, deleting artifacts)
- Responding to HITL gates (pause points where the pipeline awaits human input before continuing)
- Interrupting or resuming execution
- Injecting guidance into live pipelines (Path B from TUI spec: feed new context into the OODA loop)

**Planning questions to address:**
- How should HITL gates be represented in the VSCode UI? (modal, sidebar panel, inline code annotation?)
- What metadata does a gate expose? (gate name, required decision type, timeout, context/reasoning)
- Can the user provide text input (e.g., guidance injection) at a gate, or only yes/no approvals?
- What happens if the user denies a gate? Does the pipeline halt, rollback, or proceed with a default action?

### 3. VSCode UI Structure

**Core panels/views to plan:**
- **Pipeline Explorer**: Tree view showing available `.ail.yaml` files in the workspace, with quick-run and edit actions
- **Pipeline Editor**: Syntax highlighting and validation for `.ail.yaml` and skill markdown (two-layer model)
- **Execution Monitor**: Shows real-time status of running pipelines (current step, elapsed time, token/cost metrics if available)
- **HITL Gate Panel**: Displays active gates with context and approval/rejection buttons
- **Output/Logs**: Structured logs from the `ail` process (step results, errors, debug traces)
- **Pipeline History**: Session browsing and results review (future scope, but plan for it)

**Planning questions to address:**
- Should the extension support multiple simultaneous pipeline runs, or one at a time?
- How should the pipeline editor integrate with VSCode's native YAML support? (custom language server, or extend existing?)
- What telemetry/metrics should be visible in the Execution Monitor? (steps, step duration, token usage, cost)
- Should there be a unified view for managing multiple pipelines, or separate editors per file?

### 4. Interrupt & Resume Behavior

**Three interrupt paths from TUI spec (adapt for extension):**
- **Path A**: Resume with zero side effects (lightweight state reset, no external calls re-run)
- **Path B**: Inject guidance into live OODA loop (send new context to current step without pausing)
- **Path C**: Hard kill via interrupt signal (clean shutdown, cleanup, report final state)

**Planning questions to address:**
- Which interrupt paths should the extension expose in its first iteration? (Recommend: Path A for v1, Path B/C as enhancements)
- How should the user trigger each path? (Buttons, keyboard shortcuts, command palette)
- What happens to in-progress steps when interrupted? (timeout handling, partial results, cleanup)

### 5. Command & Workflow Integration

**Commands the extension should expose:**
- `ail.run`: Run the pipeline in the active editor
- `ail.pause`: Pause execution (if supported by headless mode)
- `ail.resume`: Resume a paused pipeline
- `ail.interrupt`: Send hard interrupt (Path C)
- `ail.injectGuidance`: Open a dialog to provide guidance (Path B, if supported)
- `ail.approveGate`: Approve the current HITL gate
- `ail.rejectGate`: Reject the current HITL gate
- `ail.switchPipeline`: Quick-switch to another pipeline in the workspace
- `ail.viewHistory`: Browse past execution sessions
- `ail.validatePipeline`: Lint and validate the `.ail.yaml` file

**Planning questions to address:**
- Which commands are high-priority for v1 vs. future iterations?
- Should these be exposed in the command palette, sidebar buttons, or inline code actions?
- What keyboard shortcuts make sense for frequent operations?

### 6. State & Session Management

**What state must the extension track:**
- Current running pipeline (if any)
- Pipeline execution context (step history, results, error state)
- User preferences (editor layout, HITL gate approval history for future undo)
- Workspace-level `ail` configuration (headless server address, timeout settings, etc.)

**Planning questions to address:**
- Should session state persist across VSCode restarts, or reset on close?
- How should the extension handle multiple workspaces or `ail` projects?
- What happens if the user closes the pipeline editor while execution is in progress?

### 7. Error Handling & Diagnostics

**Failure modes to plan for:**
- `ail` subprocess crashes or hangs
- Control channel breakdown (IPC/socket failure)
- HITL gate timeout (user doesn't approve in time)
- Step execution failure (runner CLI exits with error)
- Malformed pipeline YAML

**Planning questions to address:**
- How should the extension recover from each failure mode?
- What diagnostics should be logged and exposed to the user?
- Should there be a debug panel for troubleshooting?

## Implementation Scope & Phasing

**Phase 1 (MVP):** 
- Basic headless execution (run a pipeline, get structured output)
- Simple Execution Monitor panel (current step, elapsed time)
- HITL gate approval/rejection via modal or panel
- Path A interrupt (resume with zero side effects, or hard kill)
- Pipeline Editor with basic YAML syntax highlighting

**Phase 2:**
- Path B (inject guidance)
- Pipeline Explorer with workspace scanning
- Command palette integration
- More detailed metrics (token count, cost estimate)

**Phase 3:**
- Pipeline History / session browsing
- Workspace-level `ail` configuration UI
- Advanced error diagnostics
- Multi-pipeline concurrent execution

**Out of scope (unless justified):**
- Native `ail` agent capability
- Sandbox/containerization (deferred unless REST provider support is added to `ail`)
- Remote execution or `ail serve` attachment (multi-agent awareness phase)

## Research & Reference

The extension should draw design lessons from:
- **Aider's Architect/Editor pipeline pattern**: Separating user intent from execution
- **Plandex's cumulative diff review UX**: Showing what changed and why before committing
- **Openclaw's hard context reset**: Clean state boundaries for interruption
- **Existing VSCode extensions**: LSP integration, task runners, debug adapters

## Questions for Implementation Phase

Once planning is complete, these should be resolved before coding:

1. **Headless Mode Design**: Should `ail` support a dedicated `--headless` flag, or is the extension expected to parse standard output and suppress interactive prompts via env variables?

2. **Control Channel**: IPC (pipe/stdin-stdout), Unix socket, HTTP/local API, or something else? Trade-offs?

3. **Event Schema**: What is the canonical JSON schema for events emitted by `ail` in headless mode? (step start, step result, gate encountered, error, etc.)

4. **YAML Validation**: Should the extension run a local `.ail.yaml` validator, or rely on `ail` to report schema errors at runtime?

5. **Concurrency**: Should the extension allow multiple pipelines to run simultaneously, or serialize execution with a queue?

6. **Cost/Token Metrics**: If `ail` tracks LLM usage, how should that data be exposed? (real-time in UI, post-execution summary, etc.)

7. **Undo/History**: Should interrupted or failed pipeline runs be resumable, or only viewable in history?

---

## Deliverables Expected from Planning

1. **Headless Mode Specification**: Exact CLI flags, output format, and control channel design
2. **UI Wireframes / Layout Sketch**: Panels, views, modals (ASCII or conceptual)
3. **Command & Keybinding Inventory**: Commands, shortcuts, and trigger points
4. **Event/Message Schema**: JSON shapes for all headless mode communications
5. **State Machine**: Pipeline execution states and valid transitions
6. **Phasing & Priority**: Which features in v1, v2, v3, and rationale for each
7. **Risk & Dependency Analysis**: What blockers or unknowns exist before coding starts?
8. **Implementation Checklist**: Actionable next steps for handoff to coding phase

---

## Tone & Approach

- **Respect the orchestrator position**: The extension is a UI wrapper around `ail`'s CLI, not an agent or autonomous system.
- **First-principles framing**: Explain *what* and *why* before *how*. Use cognitive science and architecture theory to justify design choices.
- **Scope discipline**: Use the three-question test: Does this feature belong in an orchestration UI? Is it required for Phase 1? Does it validate the orchestrator model?
- **Plan before coding**: Resolve architectural unknowns (especially headless mode and control channels) before implementation starts.
