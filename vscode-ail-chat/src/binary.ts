/**
 * Binary resolution — finds the ail executable to use.
 *
 * Resolution order:
 *   1. ail-chat.binaryPath setting (if set and file exists)         → source: 'config'
 *   2. ail on PATH (via `which`/`where`)                            → source: 'path'
 *   3. Bundled binary in dist/ail-{platform-triple} (never-stuck)   → source: 'bundled'
 *
 * Bundled is the safety net so the extension always launches with a working
 * binary — even on a fresh install with no network. PATH wins over bundled
 * because users who installed `ail` deliberately should keep using their
 * chosen version; the activation-time update check tells them when the
 * bundled / latest release is newer.
 *
 * The minimum-version gate (refusal + "Use Anyway" override + status bar)
 * lives in `min-version-gate.ts`; this module is purely about resolution.
 */

import * as fs from "fs";
import * as path from "path";
import { execFile } from "child_process";
import * as vscode from "vscode";
import { createPlatform } from "./platforms";
import { isAilOnPath } from "./path-detection";

export type BinarySource = "config" | "path" | "bundled";

export interface ResolvedBinary {
  path: string;
  version: string;
  source: BinarySource;
}

let cached: ResolvedBinary | undefined;

/** Returns the platform triple used for bundled binary naming. */
export function platformTriple(): string {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  const platformMap: Record<string, string> = {
    darwin: "apple-darwin",
    linux: "unknown-linux-musl",
    win32: "pc-windows-msvc",
  };
  const plat = platformMap[process.platform] ?? "unknown-linux-musl";
  return `${arch}-${plat}`;
}

/** Returns the bundled binary filename for this platform. */
function bundledBinaryName(): string {
  return createPlatform().binary.bundledBinaryName(platformTriple());
}

/** Run `ail --version` and return the version string. */
async function getVersion(binaryPath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(binaryPath, ["--version"], { timeout: 5000 }, (err, stdout) => {
      if (err) {
        reject(err);
      } else {
        // Output formats:
        //   "ail 0.4.0"
        //   "ail 0.4.0 (rev abc123, built 2026-04-25)"
        // Take the second whitespace-separated token (the SemVer).
        const tokens = stdout.trim().split(/\s+/);
        const version = tokens[1] ?? "unknown";
        resolve(version);
      }
    });
  });
}

/** Compare two semver strings. Returns true if actual >= minimum. */
export function meetsMinVersion(actual: string, minimum: string): boolean {
  const parse = (v: string) => v.split(".").map((n) => parseInt(n, 10) || 0);
  const [maj1, min1, pat1] = parse(actual);
  const [maj2, min2, pat2] = parse(minimum);
  if (maj1 !== maj2) return maj1 > maj2;
  if (min1 !== min2) return min1 > min2;
  return pat1 >= pat2;
}

/**
 * Resolve the ail binary. Caches the result for the lifetime of the extension.
 * Call this once during activation and pass the result around.
 */
export async function resolveBinary(
  context: vscode.ExtensionContext
): Promise<ResolvedBinary> {
  if (cached) {
    return cached;
  }

  const config = vscode.workspace.getConfiguration("ail-chat");
  const configuredPath = config.get<string>("binaryPath", "");

  let binaryPath: string | undefined;
  let source: BinarySource | undefined;

  // 1. User-configured path
  if (configuredPath && fs.existsSync(configuredPath)) {
    binaryPath = configuredPath;
    source = "config";
  }

  // 2. PATH
  if (!binaryPath) {
    if (await isAilOnPath()) {
      binaryPath = createPlatform().binary.pathBinaryName();
      source = "path";
    }
  }

  // 3. Bundled (never-stuck fallback)
  if (!binaryPath) {
    const bundled = path.join(
      context.extensionPath,
      "dist",
      bundledBinaryName()
    );
    if (fs.existsSync(bundled)) {
      binaryPath = bundled;
      source = "bundled";
      // Ensure executable
      try {
        fs.chmodSync(bundled, 0o755);
      } catch {
        // May already be executable; ignore
      }
    }
  }

  if (!binaryPath || !source) {
    const msg =
      "ail binary not found: no configured path, not on PATH, no bundled fallback. Set ail-chat.binaryPath to override.";
    void vscode.window.showErrorMessage(msg);
    throw new Error(msg);
  }

  // Verify binary works and get version
  let version: string;
  try {
    version = await getVersion(binaryPath);
  } catch (err) {
    const msg = `ail binary not executable at '${binaryPath}'. Set ail-chat.binaryPath to override.`;
    void vscode.window.showErrorMessage(msg);
    throw new Error(msg);
  }

  cached = { path: binaryPath, version, source };
  return cached;
}

/** Clear the cached binary (call on configuration change). */
export function clearBinaryCache(): void {
  cached = undefined;
}
