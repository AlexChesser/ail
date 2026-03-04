# Goal
Develop a comprehensive technical architecture and implementation strategy for "Alexander's Impressive Loops" (`ail`), a high-integrity, Rust-native Agentic Orchestrator.

# Phase 1: Context & Core Constraints
- **Identity:** `ail` acts as a deterministic "Kernel" managing stochastic 3rd-party CLI tools (e.g., Aider, Claude Code).
- **Language:** Performance-critical Rust utilizing the `tokio` runtime for asynchronous concurrency.
- **Infrastructure:** A `docker-compose` topology including the Rust binary, an LLM Proxy (LiteLLM/Bifrost), Redis (state caching), and PostgreSQL (audit/learning logs).
- **Design Philosophy:** Adherence to "Accelerate" (DORA) metrics, 15-Factor App standards, and Hexagonal (Ports & Adapters) architecture.

# Phase 2: Functional Requirements Decomposition
Reason through and provide technical specifications for the following modules:

## 1. PTY-Managed Interaction Layer
- **Mechanism:** Utilize `portable-pty` to wrap child processes.
- **Challenge:** Logic for "bubbling up" interactive TTY prompts (e.g., sudo, git auth, y/n) from child binaries to a Ratatui-based TUI without blocking the main event loop.

## 3. The "Janitor" Memory Protocol
- **Constraint:** Implement a mandatory "Context Distillation" step between state transitions.
- **Metric:** Reduce token density by >90% while maintaining "load-bearing" information for the next reasoning turn.

## 4. Recursive Meta-Learning Engine
- **Strategy:** Parallel prompting between "Commodity" and "Frontier" models.
- **Feedback:** A PID-controlled sampling rate (0.0–1.0) adjusted by a real-time "Quality Score" (S).
- **Mutation:** Requirements for atomic, Git-backed YAML/Prompt mutation on disk to codify learned optimizations.

## 5. Observability & HITL (Human-in-the-Loop)
- **Middleware:** Strategy for deep inspection of raw HTTP metadata (headers, latent provider filters) via the Proxy.
- **Circuit Breakers:** Non-blocking asynchronous triggers for Confidence Breach, Budget Gates (e.g., >$2.00/loop), and High-Risk commands.

# Phase 3: Domain-Driven Design (DDD) & Best Practices
Ensure the architecture reflects the following:
- **Ubiquitous Language:** Structs/Traits must align with: `Orchestrator`, `AggregateRoot`, `Janitor`, `RefinementLoop`, `CircuitBreaker`.
- **SOLID/DI:** Define how Dependency Injection is used to make Model Providers and Persistence layers swappable.
- **State Management:** Formalize the 15-Factor requirement for statelessness (externalizing memory to backing services).

# Phase 4: Output Requirements
1. **Architecture Diagram:** Provide a Mermaid.js flowchart showing the "Janitor -> Parallel Run -> Critique -> YAML Mutation" sequence.
2. **Data Flow:** Define the API/Contract between the Rust Kernel and the TUI/Proxy layers.
3. **Rust Blueprint:** Provide idiomatic trait definitions for the `ModelProvider` and `ContextManager`.
4. **Risk Assessment:** Identify 3 technical failure modes (e.g., feedback-loop hallucinations) and provide specific mitigations.

# Final Verification
The response must prioritize first-principles engineering over generic summaries. Avoid persona-based reasoning; focus on structural integrity and system reliability.