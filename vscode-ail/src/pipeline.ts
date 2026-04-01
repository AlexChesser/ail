/**
 * Pipeline file utilities — discovery and step parsing.
 *
 * Shared between PipelineTreeProvider and StepsTreeProvider.
 */

import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";

// ── Discovery ────────────────────────────────────────────────────────────────

/** Find all .ail.yaml / .ail.yml files across workspace folders. */
export function discoverPipelines(): string[] {
  const results: string[] = [];
  const seen = new Set<string>();

  function add(filePath: string): void {
    if (!seen.has(filePath) && fs.existsSync(filePath)) {
      seen.add(filePath);
      results.push(filePath);
    }
  }

  const folders = vscode.workspace.workspaceFolders ?? [];
  for (const folder of folders) {
    const root = folder.uri.fsPath;

    // Root-level candidates
    add(path.join(root, ".ail.yaml"));
    add(path.join(root, ".ail.yml"));

    // Scan one level deep for *.ail.yaml and .ail/ subdirectory
    try {
      const entries = fs.readdirSync(root, { withFileTypes: true });
      for (const entry of entries) {
        if (entry.isFile()) {
          if (entry.name.endsWith(".ail.yaml") || entry.name.endsWith(".ail.yml")) {
            add(path.join(root, entry.name));
          }
        } else if (entry.isDirectory() && entry.name === ".ail") {
          const subdir = path.join(root, ".ail");
          try {
            for (const sub of fs.readdirSync(subdir, { withFileTypes: true })) {
              if (sub.isFile() && (sub.name.endsWith(".yaml") || sub.name.endsWith(".yml"))) {
                add(path.join(subdir, sub.name));
              }
            }
          } catch { /* ignore unreadable subdir */ }
        }
      }
    } catch { /* ignore unreadable root */ }
  }

  return results;
}

/** Display label for a pipeline file path (relative to nearest workspace folder). */
export function pipelineLabel(filePath: string): string {
  const folders = vscode.workspace.workspaceFolders ?? [];
  for (const folder of folders) {
    const rel = path.relative(folder.uri.fsPath, filePath);
    if (!rel.startsWith("..")) return rel;
  }
  return path.basename(filePath);
}

// ── Step parsing ─────────────────────────────────────────────────────────────

export interface ParsedStep {
  id: string;
  type: string;
  line: number;
}

/**
 * Extract pipeline steps from a .ail.yaml file.
 * Line-based parser — no YAML library dependency.
 */
export function parseStepsFromYaml(filePath: string): ParsedStep[] {
  let content: string;
  try {
    content = fs.readFileSync(filePath, "utf-8");
  } catch {
    return [];
  }

  const steps: ParsedStep[] = [];
  const lines = content.split("\n");
  let inPipeline = false;
  let currentId: string | undefined;
  let currentLine = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (trimmed === "pipeline:") {
      inPipeline = true;
      continue;
    }

    if (inPipeline) {
      if (/^\S/.test(line) && trimmed !== "") {
        inPipeline = false;
        continue;
      }

      const idMatch = trimmed.match(/^-?\s*id:\s*(.+)/);
      if (idMatch) {
        currentId = idMatch[1].trim().replace(/['"]/g, "");
        currentLine = i;
      }

      if (currentId) {
        let type: string | undefined;
        if (/^prompt:/.test(trimmed)) type = "prompt";
        else if (/^context:/.test(trimmed)) type = "context";
        else if (/^action:/.test(trimmed)) type = "action";
        else if (/^pipeline:/.test(trimmed)) type = "pipeline";

        if (type) {
          steps.push({ id: currentId, type, line: currentLine });
          currentId = undefined;
        }
      }
    }
  }

  return steps;
}

// ── Step type icon ────────────────────────────────────────────────────────────

export function stepTypeIcon(stepType: string): vscode.ThemeIcon {
  switch (stepType) {
    case "prompt":   return new vscode.ThemeIcon("comment");
    case "context":  return new vscode.ThemeIcon("terminal");
    case "action":   return new vscode.ThemeIcon("debug-pause");
    case "pipeline": return new vscode.ThemeIcon("references");
    default:         return new vscode.ThemeIcon("symbol-misc");
  }
}
