# TASK: Structural and Architectural Audit of the "ail" TUI

You are to perform a rigorous architectural analysis of the current TUI implementation within the "ail" (Alexander's Impressive Loops) repository. Base your evaluation on the current codebase and the standards defined in `ARCHITECTURE.md`.

## CONTEXT & REFERENCE MATERIAL
1. **Primary Document:** Read `ARCHITECTURE.md` to establish the ground-truth principles of the project.
2. **Current Implementation:** Analyze the TUI modules, specifically focusing on the integration of the "Claude runner."

## ANALYSIS CRITERIA
Evaluate the codebase against the following technical benchmarks:
* **SOLID Conformity:** Identify specific violations of Single Responsibility, Open/Closed, Liskov Substitution, Interface Segregation, and Dependency Inversion.
* **Responsibility Mapping:** Pinpoint modules or classes currently handling more than one primary concern.
* **Dependency Injection (DI):** Verify if all external services and runners are injected or if they are instantiated internally (tightly coupled).
* **Coupling & Cohesion:** - Identify any cyclic dependencies.
    - Specifically analyze the "Claude" integration. Determine if the system is "Claude-locked" or if the Claude runner functions as a swappable module.
* **Interface Abstraction:** Evaluate the strength of the interface layer between the core logic and the "runners." Ensure the architecture supports the requirement that `ail` can call disparate runners.

## OUTPUT SPECIFICATION
Generate a valid JSON object that could serve as the data source for a frontend table. 

* **File Name:** `qwen-architectural-analysis.json`
* **Location:** `/demo` folder.
* **Schema Requirements:** Each entry must include:
{
  "id": "STRICT-ID-001",
  "principle": "Name of Principle (e.g., SOLID-D)",
  "category": "Descriptive Category",
  "severity": "low/medium/high",
  "component": "File :: Function/Struct name",
  "location": "Relative path and line numbers",
  "summary": "One sentence overview of the issue.",
  "detail": "Technical explanation of the drift from ARCHITECTURE.md.",
  "recommendation": "Specific code-level fix (e.g., Change fn signature to...)."
}

## EXECUTION STEPS
1. Compare the existing TUI code against the "ideal state" described in `ARCHITECTURE.md`.
2. Trace the data flow from the TUI to the Claude runner to detect hidden dependencies.
3. Validate the JSON structure for parsability before finalizing.