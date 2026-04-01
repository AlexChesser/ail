/**
 * ail.runPipeline and ail.stopPipeline commands.
 *
 * Spawns `ail --once <prompt> --pipeline <path> --output-format json --headless`
 * and routes events to the Output Channel and (Phase 3) the Execution Monitor panel.
 */

import * as vscode from "vscode";
import * as path from "path";
import { spawn, ChildProcess } from "child_process";
import { parseNdjsonStream } from "../ndjson";
import { ResolvedBinary } from "../binary";
import { AilEvent, PipelineCompletedEvent, PipelineErrorEvent, StepStartedEvent } from "../types";
import { ExecutionPanel } from "../panels/ExecutionPanel";

let activeProcess: ChildProcess | undefined;
let outputChannel: vscode.OutputChannel | undefined;
let activePanel: ExecutionPanel | undefined;

function getOutputChannel(): vscode.OutputChannel {
  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel("ail");
  }
  return outputChannel;
}

/** Resolve the pipeline path to run. */
function resolvePipelinePath(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  if (editor) {
    const filePath = editor.document.uri.fsPath;
    if (filePath.endsWith(".ail.yaml") || filePath.endsWith(".ail.yml")) {
      return filePath;
    }
  }

  const config = vscode.workspace.getConfiguration("ail");
  const defaultPipeline = config.get<string>("defaultPipeline", "");
  if (defaultPipeline) {
    return defaultPipeline;
  }

  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (workspaceFolders?.[0]) {
    const candidate = path.join(workspaceFolders[0].uri.fsPath, ".ail.yaml");
    return candidate;
  }

  return undefined;
}

/** Format an AilEvent into a human-readable Output Channel line. */
function formatEvent(event: AilEvent): string | undefined {
  switch (event.type) {
    case "run_started":
      return `\n▶ Pipeline run ${event.run_id} started (${event.total_steps} step(s))`;
    case "step_started": {
      const e = event as StepStartedEvent;
      return `\n[${e.step_index + 1}/${e.total_steps}] ${e.step_id} — running...`;
    }
    case "step_completed":
      return `    ✓ ${event.step_id}${event.cost_usd != null ? ` ($${event.cost_usd.toFixed(4)})` : ""}`;
    case "step_skipped":
      return `    ⊘ ${event.step_id} (skipped)`;
    case "step_failed":
      return `    ✗ ${event.step_id}: ${event.error}`;
    case "hitl_gate_reached":
      return `\n⏸ HITL gate: ${event.step_id} — waiting for approval`;
    case "runner_event": {
      const re = event.event;
      switch (re.type) {
        case "stream_delta":
          return re.text; // streamed inline without newline
        case "tool_use":
          return `\n  → ${re.tool_name}`;
        case "tool_result":
          return `  ← ${re.tool_name}`;
        case "cost_update":
          return undefined; // suppress — shown in step_completed
        case "thinking":
          return undefined; // suppress thinking blocks in output channel
        default:
          return undefined;
      }
    }
    case "pipeline_completed": {
      const e = event as PipelineCompletedEvent;
      return e.outcome === "break"
        ? `\n✓ Pipeline completed (break at ${e.step_id})`
        : `\n✓ Pipeline completed`;
    }
    case "pipeline_error": {
      const e = event as PipelineErrorEvent;
      return `\n✗ Pipeline error [${e.error_type}]: ${e.error}`;
    }
    default:
      return undefined;
  }
}

function updateStatusBar(
  statusBarItem: vscode.StatusBarItem,
  running: boolean
): void {
  if (running) {
    statusBarItem.text = "$(loading~spin) ail: running";
    statusBarItem.tooltip = "ail pipeline is running — click to stop";
    statusBarItem.command = "ail.stopPipeline";
    statusBarItem.show();
  } else {
    statusBarItem.hide();
  }
}

export function registerRunCommands(
  context: vscode.ExtensionContext,
  binary: ResolvedBinary,
  statusBarItem: vscode.StatusBarItem
): void {
  const runDisposable = vscode.commands.registerCommand(
    "ail.runPipeline",
    async () => {
      if (activeProcess) {
        void vscode.window.showWarningMessage(
          "An ail pipeline is already running. Use 'Ail: Stop Pipeline' to cancel it first."
        );
        return;
      }

      const pipelinePath = resolvePipelinePath();
      if (!pipelinePath) {
        void vscode.window.showWarningMessage(
          "No .ail.yaml file found. Open a pipeline file or set ail.defaultPipeline."
        );
        return;
      }

      const prompt = await vscode.window.showInputBox({
        prompt: "Enter your prompt for ail",
        placeHolder: "e.g. refactor the auth module for DRY compliance",
        ignoreFocusOut: true,
      });

      if (!prompt) {
        return; // User cancelled
      }

      // Open Execution Monitor panel.
      activePanel = ExecutionPanel.create(context);

      const out = getOutputChannel();
      out.show(true);
      out.appendLine(`\n${"─".repeat(60)}`);
      out.appendLine(`ail run — ${new Date().toLocaleTimeString()}`);
      out.appendLine(`Pipeline: ${pipelinePath}`);
      out.appendLine(`Prompt: ${prompt}`);

      const args = [
        "--once",
        prompt,
        "--pipeline",
        pipelinePath,
        "--output-format",
        "json",
        "--headless",
      ];

      const proc = spawn(binary.path, args, {
        cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
      });

      activeProcess = proc;
      updateStatusBar(statusBarItem, true);

      let totalCost = 0;

      parseNdjsonStream(
        proc.stdout!,
        (event) => {
          // Route to Execution Monitor panel.
          activePanel?.onEvent(event);

          const line = formatEvent(event);
          if (line !== undefined) {
            // stream_delta has no newline prefix — append inline
            if (
              event.type === "runner_event" &&
              event.event.type === "stream_delta"
            ) {
              out.append(line);
            } else {
              out.appendLine(line);
            }
          }

          // Accumulate cost
          if (
            event.type === "runner_event" &&
            event.event.type === "cost_update"
          ) {
            totalCost = event.event.cost_usd;
          }
        },
        (err) => {
          out.appendLine(`\n[stream error] ${err.message}`);
        }
      );

      // Route stderr to output channel
      proc.stderr?.setEncoding("utf-8");
      proc.stderr?.on("data", (chunk: string) => {
        // ail writes tracing JSON to stderr — skip it in the output channel
        // (it's useful for debugging but noisy in the UI)
        void chunk;
      });

      proc.on("close", (code) => {
        activeProcess = undefined;
        activePanel = undefined;
        updateStatusBar(statusBarItem, false);

        if (code === 0) {
          const costStr = totalCost > 0 ? ` (total: $${totalCost.toFixed(4)})` : "";
          void vscode.window.showInformationMessage(
            `ail: Pipeline completed${costStr}`
          );
        } else if (code !== null) {
          void vscode.window.showErrorMessage(
            `ail: Pipeline exited with code ${code} — see Output panel.`
          );
        }
        // code === null means killed by signal (stop command) — no notification
      });

      proc.on("error", (err) => {
        activeProcess = undefined;
        updateStatusBar(statusBarItem, false);
        out.appendLine(`\n[error] Failed to spawn ail: ${err.message}`);
        void vscode.window.showErrorMessage(`ail: ${err.message}`);
      });
    }
  );

  const stopDisposable = vscode.commands.registerCommand(
    "ail.stopPipeline",
    () => {
      if (!activeProcess) {
        void vscode.window.showInformationMessage("No ail pipeline is running.");
        return;
      }

      const proc = activeProcess;
      const out = getOutputChannel();
      out.appendLine("\n⏹ Stop requested — sending SIGTERM...");

      proc.kill("SIGTERM");

      // Hard kill after 5 seconds if still running
      const timeout = setTimeout(() => {
        if (activeProcess === proc) {
          out.appendLine("  Process did not exit — sending SIGKILL.");
          proc.kill("SIGKILL");
        }
      }, 5000);

      proc.on("close", () => {
        clearTimeout(timeout);
        activeProcess = undefined;
        updateStatusBar(statusBarItem, false);
        out.appendLine("  Process stopped.");
      });
    }
  );

  const materializeDisposable = vscode.commands.registerCommand(
    "ail.materializePipeline",
    async () => {
      const pipelinePath = resolvePipelinePath();
      if (!pipelinePath) {
        void vscode.window.showWarningMessage("No .ail.yaml file found.");
        return;
      }

      const { execFile } = await import("child_process");
      execFile(
        binary.path,
        ["materialize", "--pipeline", pipelinePath],
        { timeout: 15000 },
        (err, stdout, stderr) => {
          if (err) {
            void vscode.window.showErrorMessage(
              `ail materialize failed: ${stderr || err.message}`
            );
            return;
          }
          const out = getOutputChannel();
          out.show(true);
          out.appendLine(`\n${"─".repeat(60)}`);
          out.appendLine(`Resolved pipeline: ${pipelinePath}`);
          out.appendLine(stdout);
        }
      );
    }
  );

  context.subscriptions.push(runDisposable, stopDisposable, materializeDisposable);
}
