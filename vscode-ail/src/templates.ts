/**
 * Pipeline template definitions for the "Create Pipeline from Template" QuickPick.
 */

export interface PipelineTemplate {
  label: string;
  description: string;
  detail: string;
  content: string;
}

export const PIPELINE_TEMPLATES: PipelineTemplate[] = [
  {
    label: '$(play) Minimal',
    description: 'Single invocation step',
    detail: 'Run your prompt through the agent with no post-processing. Good starting point.',
    content: `version: "0.0.1"
pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"
`,
  },
  {
    label: '$(check) Review',
    description: 'Invoke then self-review',
    detail: 'Agent responds, then a second step reviews and refines the output.',
    content: `version: "0.0.1"
pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"
  - id: review
    prompt: >
      Review your previous response for correctness and unnecessary complexity.
      Fix any issues you find. If the response is already correct, output it unchanged.
`,
  },
  {
    label: '$(search) Context + Prompt',
    description: 'Gather context, then invoke',
    detail: 'Runs a shell command to gather project context before passing to the agent.',
    content: `version: "0.0.1"
pipeline:
  - id: gather_context
    context:
      shell: "find . -name '*.ts' -not -path '*/node_modules/*' | head -30"
  - id: invocation
    prompt: >
      Project files:
      {{ step.gather_context.result }}

      Task: {{ step.invocation.prompt }}
`,
  },
  {
    label: '$(shield) Guarded',
    description: 'Read-only tools, then review',
    detail: 'Restricts the agent to read-only tools on the first pass, then reviews.',
    content: `version: "0.0.1"
pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"
    tools:
      allow:
        - Read
        - Glob
        - Grep
      deny:
        - Bash
        - Edit
        - Write
  - id: review
    prompt: >
      Review the above response. If file changes are needed, implement them now.
`,
  },
];
