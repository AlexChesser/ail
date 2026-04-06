Analyze the user's request and the project context provided. Create a detailed implementation plan following these steps:

1. **Scope validation** — Ensure the request covers one cohesive subsystem. If it spans multiple independent systems, recommend splitting before proceeding.

2. **File mapping** — Design focused, single-responsibility files with clear boundaries. Identify existing files that need modification vs new files to create.

3. **Task decomposition** — Break the work into bite-sized steps (2-5 minutes each). Each step should be one action: write a test, run it, implement, verify, commit.

4. **Dependency ordering** — Arrange tasks so each builds on the previous. No task should require work from a later task.

5. **Plan documentation** — Generate the plan as a structured markdown document with:
   - A header section (goal, architecture decisions, tech stack)
   - Numbered tasks, each with: target files, step-by-step instructions, expected verification commands
   - Complete code blocks (no placeholders)

6. **Self-review** — Before finishing, verify:
   - Every requirement from the user's request is covered
   - No placeholders or TBD items remain
   - File paths are specific and correct
   - Test steps actually test the right behavior
   - No step takes longer than 5 minutes
