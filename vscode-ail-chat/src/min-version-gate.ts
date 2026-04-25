/**
 * Minimum-version gate — refuses to start ail sessions when the resolved
 * binary's version is below the minimum declared in package.json#config.ailMinVersion,
 * unless the user has opted into the "Use Anyway" override.
 *
 * The override is persisted in globalState; users toggle it via the
 * `ail-chat.useAnyway` command (with a modal confirmation).
 *
 * A status bar item with theme-respecting error / warning colours surfaces
 * the state at all times when the binary is below minimum.
 */

import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { meetsMinVersion } from "./binary";

const USE_ANYWAY_KEY = "ail-chat.useAnywayBelowMinVersion";

export interface MinVersionState {
  /** Minimum version declared in package.json#config.ailMinVersion (e.g. "0.4.0"). */
  minVersion: string;
  /** Version of the resolved ail binary (e.g. "0.4.1"). */
  resolvedVersion: string;
  /** True if resolvedVersion >= minVersion. */
  meetsMin: boolean;
  /** True if the user has opted into the override. */
  useAnywayActive: boolean;
}

/** Read the declared minVersion from the extension's own package.json. */
export function readDeclaredMinVersion(extensionPath: string): string {
  try {
    const pkg = JSON.parse(
      fs.readFileSync(path.join(extensionPath, "package.json"), "utf-8")
    ) as { config?: { ailMinVersion?: string } };
    return pkg.config?.ailMinVersion ?? "0.0.0";
  } catch {
    return "0.0.0";
  }
}

/** Compute the current min-version state from a resolved binary version. */
export function getMinVersionState(
  context: vscode.ExtensionContext,
  resolvedVersion: string
): MinVersionState {
  const minVersion = readDeclaredMinVersion(context.extensionPath);
  return {
    minVersion,
    resolvedVersion,
    meetsMin: meetsMinVersion(resolvedVersion, minVersion),
    useAnywayActive: context.globalState.get<boolean>(USE_ANYWAY_KEY, false),
  };
}

/** True when the binary satisfies the gate (either meets minimum or override is active). */
export function isAcceptable(state: MinVersionState): boolean {
  return state.meetsMin || state.useAnywayActive;
}

/** Persist the override flag. */
export async function setUseAnyway(
  context: vscode.ExtensionContext,
  enabled: boolean
): Promise<void> {
  await context.globalState.update(USE_ANYWAY_KEY, enabled);
}

/**
 * A status bar item that mirrors the gate state.
 *
 *   - Hidden when meetsMin is true.
 *   - Error-themed when below min and override inactive.
 *   - Warning-themed when below min and override active (so users don't forget).
 *
 * Click action runs `ail-chat.installBinary` to surface the recovery path.
 */
export class MinVersionStatusItem implements vscode.Disposable {
  private item: vscode.StatusBarItem;

  constructor() {
    this.item = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
      100
    );
    this.item.command = "ail-chat.installBinary";
  }

  update(state: MinVersionState): void {
    if (state.meetsMin) {
      this.item.hide();
      return;
    }
    if (state.useAnywayActive) {
      this.item.text = `$(warning) ail v${state.resolvedVersion} (override)`;
      this.item.tooltip = `ail v${state.resolvedVersion} is below the minimum supported version (${state.minVersion}). "Use Anyway" override is active. Click to install update.`;
      this.item.backgroundColor = new vscode.ThemeColor(
        "statusBarItem.warningBackground"
      );
    } else {
      this.item.text = `$(error) ail v${state.resolvedVersion} < required ${state.minVersion}`;
      this.item.tooltip = `ail v${state.resolvedVersion} is below the minimum supported version (${state.minVersion}). Click to install update.`;
      this.item.backgroundColor = new vscode.ThemeColor(
        "statusBarItem.errorBackground"
      );
    }
    this.item.show();
  }

  dispose(): void {
    this.item.dispose();
  }
}
