/**
 * ail VSCode extension — entry point.
 *
 * Activates on workspaces containing .ail.yaml files.
 * Resolves the ail binary, wires DI, registers commands, and sets up lifecycle management.
 */

import * as vscode from "vscode";
import { execFile } from "child_process";
import { resolveBinary, clearBinaryCache } from "./binary";
import { registerPipelineExplorer } from "./views/PipelineTreeProvider";
import { registerStepsView } from "./views/StepsTreeProvider";
import { registerHistoryView } from "./views/HistoryTreeProvider";
import { ChatViewProvider } from "./views/ChatViewProvider";
import { registerCompletions } from "./language/completions";
import { initState } from "./state";
import { AilProcess } from "./infrastructure/AilProcess";
import { createServiceContext } from "./application/ServiceContext";
import { EventBus } from "./application/EventBus";
import { RunnerService } from "./application/RunnerService";
import { HistoryService } from "./application/HistoryService";
import { UnifiedPanel } from "./panels/UnifiedPanel";
import { RunCommand } from "./commands/RunCommand";
import { ValidateCommand } from "./commands/ValidateCommand";
import { resolvePipelinePath } from "./utils/pipelinePath";

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Persistent active pipeline state
  initState(context);

  // Status bar item — shows running state.
  const statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
  );
  context.subscriptions.push(statusBarItem);

  // Clear binary cache when ail.binaryPath changes.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("ail.binaryPath")) {
        clearBinaryCache();
      }
    })
  );

  // Resolve binary. Errors are surfaced by resolveBinary() itself.
  let binary;
  try {
    binary = await resolveBinary(context);
  } catch {
    // resolveBinary() already showed an error message. Extension is degraded but not crashed.
    return;
  }

  // ── DI wiring ──────────────────────────────────────────────────────────────

  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const client = new AilProcess(binary.path, cwd);
  const services = createServiceContext(context, client, binary.path, cwd);
  const bus = new EventBus();
  const runnerService = new RunnerService(services, bus);
  const historyService = new HistoryService(context, cwd ?? process.cwd());

  // Wire HistoryService into the singleton UnifiedPanel so it can load run
  // details when the user clicks a historical run in column 1.
  UnifiedPanel.setHistoryService(historyService);

  // ── Views ──────────────────────────────────────────────────────────────────

  const chatProvider = new ChatViewProvider();
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(ChatViewProvider.viewId, chatProvider)
  );

  registerPipelineExplorer(context, binary);
  const stepsProvider = registerStepsView(context);
  const historyProvider = registerHistoryView(context, historyService);

  // Inject views into the runner service
  runnerService.setViews(statusBarItem, chatProvider, stepsProvider);

  // Refresh history sidebar tree and unified panel column 1 after each run completes
  runnerService.setOnRunComplete(() => {
    historyProvider.refresh();
    void UnifiedPanel.refreshHistory();
  });

  // ── Commands ───────────────────────────────────────────────────────────────

  const runCommand = new RunCommand(runnerService);
  const validateCommand = new ValidateCommand(services);

  // Register the diagnostic collection for cleanup
  context.subscriptions.push(validateCommand.getDiagnosticCollection());

  context.subscriptions.push(
    vscode.commands.registerCommand("ail.openHistoryRun", async (runId: string) => {
      const record = await historyService.getRunDetail(runId);
      if (!record) {
        void vscode.window.showWarningMessage(`ail: Run ${runId} not found in history.`);
        return;
      }
      UnifiedPanel.openReview(context, record);
    }),

    vscode.commands.registerCommand("ail.forkHistoryRun", async (runId: string) => {
      const record = await historyService.getRunDetail(runId);
      if (!record || !record.invocationPrompt) {
        void vscode.window.showWarningMessage("ail: No prompt available for this run.");
        return;
      }
      // Focus the chat view and populate the textarea with the original prompt
      await vscode.commands.executeCommand("ail.chatView.focus");
      chatProvider.populate(record.invocationPrompt);
    }),

    vscode.commands.registerCommand("ail.openUnifiedPanel", async () => {
      const records = await historyService.getHistory();
      if (records.length === 0) {
        void vscode.window.showInformationMessage("ail: No run history found. Run a pipeline first.");
        return;
      }
      UnifiedPanel.openReview(context, records[0]);
    }),

    vscode.commands.registerCommand("ail.runPipeline", async () => {
      await runCommand.execute();
    }),

    vscode.commands.registerCommand("ail.runWithPrompt", async (prompt: string) => {
      await runCommand.execute(prompt);
    }),

    vscode.commands.registerCommand("ail.stopPipeline", () => {
      runnerService.stopRun();
    }),

    vscode.commands.registerCommand("ail.validatePipeline", async () => {
      await validateCommand.execute();
    }),

    vscode.commands.registerCommand("ail.materializePipeline", async () => {
      const pipelinePath = resolvePipelinePath();
      if (!pipelinePath) {
        void vscode.window.showWarningMessage("No .ail.yaml file found.");
        return;
      }

      const finalPath = pipelinePath;
      execFile(
        binary.path,
        ["materialize", "--pipeline", finalPath],
        { timeout: 15000 },
        (err, stdout, stderr) => {
          if (err) {
            void vscode.window.showErrorMessage(
              `ail materialize failed: ${stderr || err.message}`
            );
            return;
          }
          const out = services.outputChannel;
          out.show(true);
          out.appendLine(`\n${"─".repeat(60)}`);
          out.appendLine(`Resolved pipeline: ${finalPath}`);
          out.appendLine(stdout);
        }
      );
    })
  );

  // Save-time validation for .ail.yaml files
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(async (document) => {
      const filePath = document.uri.fsPath;
      if (!filePath.endsWith(".ail.yaml") && !filePath.endsWith(".ail.yml")) {
        return;
      }
      await validateCommand.execute(filePath);
    })
  );

  registerCompletions(context);

  console.log(`ail extension activated — binary: ${binary.path} (${binary.version})`);
}

export function deactivate(): void {
  // VS Code disposes all context.subscriptions automatically.
}
