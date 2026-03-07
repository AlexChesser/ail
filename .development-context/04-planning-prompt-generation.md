OK I think I'm ready to move to real execution for a v.0.0.1 proof of concept level MVP implementation of the @RUNNER-SPEC.md. Please output a well structured prompt that I can use to initiate the workflow for this work.

Break the work into small, atomic commits and phases that are human review-able. Between each phase we should take a moment to reflect in order to extract any learnings and do any refactorings and cleanup.

Reference the ARCHITECTURE.md document in order to make decisions about how we need to implement.

Reference the SPEC.md in order to ensure we are building in the correct direction.

Because this is a zero to one build and based on informed speculation we are not sure what our unknown-unnkowns are. There is every possibility that we will learn things as we progress and realize things that we couldn't conceptualize previously.  In order to support a clean build and because we do not want to find ourselves painted into a corner, we must make time for reflection, learnings and course correction between each stage of the defelopment flow.

Each phase should provide a working testable artifact that can be verified by running some aspect of the CLI tool. While we fully intend to have a full suite of automated tests, we also want to make sure there's an observable milestone on each phase.  The first phase could be a hello world, while a subsequent phase could be building out the "materialize-pipeline" command (and reading the YAML) writing to the pipeline logger, or gathering context out of pipeline history.

Phases should be ordered to build upon each other in dependency order - so - a CLI is required before reading a pipeline file, reading a pipeline is required before materializing it, logging is required before materializing, and gathering pipeline log context is required before executing steps (or at least technically a second step)

ensure the prompt uses all  research backed best practices for prompting - in particular ensure that we do not assign a persona to help prevent hallucinations. 

take inspiration and the best parts of the /writing-plans skill by anthropic.
 