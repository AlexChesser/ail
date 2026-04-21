/**
 * Template variable completions for .ail.yaml files.
 * Triggers inside {{ }} blocks and suggests valid variable paths.
 */

import * as vscode from "vscode";

// All valid template variables from SPEC §11
const TEMPLATE_VARIABLES: Array<{ label: string; detail: string; documentation: string }> = [
  {
    label: "step.invocation.prompt",
    detail: "string",
    documentation: "The original user prompt (triggering invocation).",
  },
  {
    label: "step.invocation.response",
    detail: "string",
    documentation: "The runner's response before any pipeline steps ran.",
  },
  {
    label: "last_response",
    detail: "string",
    documentation: "Most recent step response.",
  },
  {
    label: "pipeline.run_id",
    detail: "uuid",
    documentation: "UUID for this pipeline run.",
  },
  {
    label: "session.tool",
    detail: "string",
    documentation: "Runner name (e.g. 'claude').",
  },
  {
    label: "session.cwd",
    detail: "path",
    documentation: "Working directory for this run.",
  },
  {
    label: "session.invocation_prompt",
    detail: "string (deprecated)",
    documentation:
      "Alias for step.invocation.prompt. Deprecated — prefer the canonical form.",
  },
  {
    label: "env.",
    detail: "string",
    documentation:
      "Environment variable. Usage: {{ env.MY_VAR }}. Aborts with error if unset.",
  },
  {
    label: "env.AIL_SELECTION",
    detail: "string",
    documentation:
      "Text selected in the active VS Code editor when the run was started. Set automatically by the ail extension.",
  },
  {
    label: "step.",
    detail: "step reference",
    documentation:
      "Access a named step's output. E.g. {{ step.my_step.response }} or {{ step.my_step.result }}",
  },
];

const STEP_SUFFIXES = [
  { suffix: ".response", doc: "Response text from a prompt: step." },
  { suffix: ".result", doc: "Output of a context: step (stdout+stderr), or response for prompts." },
  { suffix: ".stdout", doc: "stdout of a context: shell: step." },
  { suffix: ".stderr", doc: "stderr of a context: shell: step." },
  { suffix: ".exit_code", doc: "Exit code of a context: shell: step (string)." },
];

export function registerCompletions(context: vscode.ExtensionContext): void {
  const provider = vscode.languages.registerCompletionItemProvider(
    [
      { language: "yaml", pattern: "**/*.ail.yaml" },
      { language: "yaml", pattern: "**/*.ail.yml" },
      { language: "ail-pipeline" },
    ],
    {
      provideCompletionItems(
        document: vscode.TextDocument,
        position: vscode.Position
      ): vscode.CompletionItem[] {
        const linePrefix = document
          .lineAt(position)
          .text.slice(0, position.character);

        // Only trigger inside {{ ... }}
        const inTemplate = /\{\{[^}]*$/.test(linePrefix);
        if (!inTemplate) return [];

        // Extract what's been typed after {{
        const templateContent = linePrefix.match(/\{\{([^}]*)$/)?.[1]?.trim() ?? "";

        const items: vscode.CompletionItem[] = [];

        for (const variable of TEMPLATE_VARIABLES) {
          if (variable.label.toLowerCase().startsWith(templateContent.toLowerCase()) ||
              templateContent === "") {
            const item = new vscode.CompletionItem(
              variable.label,
              vscode.CompletionItemKind.Variable
            );
            item.detail = variable.detail;
            item.documentation = new vscode.MarkdownString(variable.documentation);
            // Insert the full variable wrapped in closing brace
            item.insertText = variable.label + " }}";
            // Replace from the start of the template content
            items.push(item);
          }
        }

        // If typing `step.<something>.`, suggest suffixes
        const stepAccessMatch = templateContent.match(/^step\.(\w+)\.?$/);
        if (stepAccessMatch) {
          for (const s of STEP_SUFFIXES) {
            const label = `step.${stepAccessMatch[1]}${s.suffix}`;
            const item = new vscode.CompletionItem(
              label,
              vscode.CompletionItemKind.Property
            );
            item.documentation = new vscode.MarkdownString(s.doc);
            item.insertText = label + " }}";
            items.push(item);
          }
        }

        return items;
      },
    },
    ".", // Trigger on `.` for step.x. access
    " " // Trigger on space after {{
  );

  context.subscriptions.push(provider);
}
