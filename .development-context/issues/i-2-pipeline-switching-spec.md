# TASK 2: Spec Impact Analysis - Pipeline-Level Switching Capability
 
## Core Question
 
Does a pipeline need the ability to query or request a pipeline switch as a top-level artifact in the spec?
 
In other words: Should `.ail.yaml` pipelines be able to programmatically emit directives to switch to another pipeline?
 
### Scope Discipline Context
 
This question is grounded in a fundamental thesis from cognitive science:
 
> Named, inheritable routines are "scripts" in the 1977 Schank and Abelson sense: pre-compiled procedural schemas that an intelligent system activates rather than derives. **Intelligence requires knowing which script to run.**
 
Pipelines are scripts in this sense. The ability to dynamically select and switch between pipelines directly addresses this core principle of intelligence. Therefore, **the scope decision for this feature is not purely optional** — it determines whether `ail` is architected to support script selection as a first-class capability or relegates it to external control only.
 
- **If Scenario A** (no spec change): Script selection is external to the system; pipelines cannot participate in the decision
- **If Scenario B** (spec extension): Script selection becomes part of the system's internal reasoning; pipelines can evaluate conditions and choose their successor
 
---
 
## Scenario Analysis
 
### Scenario A: No Spec Change
 
**Definition**: Pipeline switching is not a spec-level concern.
 
**Constraints**:
- Pipelines cannot programmatically request a switch
- Pipelines are leaf nodes; they run, produce output, and don't redirect control flow
 
**Use case**: 
- External agent (user or system) switches when it decides to
- Pipeline completes its work; control flow remains external
 
**Implication**: No spec extension needed.
 
---
 
### Scenario B: Pipeline-Initiated Switching (Spec Extension)
 
**Definition**: Pipelines can emit a "switch pipeline" directive.
 
**Mechanism**: New `.ail.yaml` artifact or `on_result` action
 
**Example**:
```yaml
on_result:
  - action: switch_pipeline
    target: incident-debugging
```
 
**Use case**: 
- Pipeline evaluates its task and decides autonomously: "I need to hand off to incident-debugging"
- Suitable for meta-pipelines or self-routing workflows
 
**Implication**:
- Spec needs extension
- Runtime must handle switch directives
- Pipelines become agents in meta-control-flow system
 
---
 
## Trade-off Comparison
 
| Aspect | Scenario A (No Spec Change) | Scenario B (Spec Extension) |
|--------|-------|---------|
| **Complexity** | Lower | Higher (spec + runtime) |
| **User control** | External only | Hybrid (external or program) |
| **Control flow model** | No redirection | Pipelines can redirect |
| **Implementation scope** | Runtime only | Spec + runtime |
| **Flexibility** | Reactive (external only) | Proactive (programmatic) |
| **Coupling** | Minimal | Tighter (spec to runtime) |
 
---
 
## Questions for Deep Analysis
 
### 1. Current `.ail.yaml` Capabilities
 
- Does the current `.ail.yaml` spec support any form of control flow redirection?
- If yes: What patterns exist? Can we extend them?
- If no: Is pipeline-switching a good first extension, or should we design a broader control-flow model?
 
### 2. Scope Decision
 
Is Scenario B (pipeline-initiated switching) in scope?
- If no: Document the decision and close the question
- If yes: Proceed to questions 3–5
 
### 3. Soft vs. Hard Directives (if Scenario B is chosen)
 
Can a pipeline emit:
- **Soft recommendation**: Structured output that external agents interpret and act on?
- **Hard directive**: Spec-level artifact that forces a switch without external confirmation?
- **Both**: Context-dependent behavior?
 
### 4. Syntax & Integration (if Scenario B is chosen)
 
What does the `.ail.yaml` syntax look like?
- Extension of `on_result`?
- New top-level directive?
- Separate artifact type?
 
How does this integrate with existing `.ail.yaml` patterns?
 
### 5. Error & Rollback (if Scenario B is chosen)
 
- If a pipeline switches to another pipeline and that fails, what happens?
  - Rollback to the original pipeline?
  - Fail hard?
  - Return to external control for intervention?
- Is the switch terminal or reversible?
 
### 6. LLM-Based Pipeline Evaluation (if Scenario B is chosen)
 
Should pipelines be able to use LLM prompts to evaluate which pipeline is the appropriate next step based on context and conditions?
 
**Examples**:
- **Prompt interceptor**: An initial pipeline receives a user prompt. It uses an LLM to analyze the linguistic intent. If the LLM determines the user is asking a planning/design question rather than requesting execution of a specific feature, the pipeline routes to a planning pipeline. If the user is asking something that fits a known workflow, it routes to the feature-evaluation pipeline.
- **Test runner conditional routing**: A linter-and-test-runner pipeline produces results. If test failures are detected, it routes to a testing-diagnostics pipeline. If no failures are found, it routes directly to a prepare-pull-request pipeline.
 
**Considerations**:
- Does the switch directive support conditional logic (e.g., `if tests_failed then route_to(diagnostics) else route_to(pr_prep)`)?
- Can a pipeline query or introspect results from the previous step to make routing decisions?
- Should the LLM evaluation be part of the switch directive syntax, or separate?
- What's the risk/benefit of allowing pipelines to dynamically choose their successor based on LLM reasoning vs. explicit logic?
 
---
 
## Expected Deliverable
 
### Minimum (All Paths)
- **Decision document**: Which scenario (A or B)?
- **Rationale**: Why this choice?
 
### If Scenario A
- **Explicit constraint documentation**: "Pipeline switching is not a spec-level concern"
- Any design notes on implications for the runtime
 
### If Scenario B
- **Draft `.ail.yaml` extension syntax**: Examples of what a switch directive looks like
- **Runtime integration model**: How does the runtime process switch directives?
- **Integration with `on_result`**: How does this fit with existing patterns?
- **Error handling strategy**: What happens if a switch fails?
- **Implementation roadmap**: Steps to add this to the spec and runtime
 
---
 
## Success Criteria
 
- [ ] Scenario decision is made (A or B)
- [ ] Rationale for the decision is documented
- [ ] If Scenario A: Design constraint is clearly stated
- [ ] If Scenario B: `.ail.yaml` syntax is drafted with examples
- [ ] If Scenario B: Runtime integration model is specified
- [ ] If Scenario B: Error handling strategy is defined
- [ ] Trade-offs are clear and defensible