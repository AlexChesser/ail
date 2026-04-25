/**
 * Update checker — polls GitHub for the latest `ail-v*` release.
 *
 * Throttled via globalState. Default interval: 1 day, configurable up to 30.
 * Manual checks (force: true) bypass the throttle and overwrite the cache.
 *
 * Failures are silent — a flaky network shouldn't alarm the user.
 */

import * as vscode from "vscode";

const REPO = "AlexChesser/ail";
const LAST_CHECK_KEY = "ail-chat.lastUpdateCheckMs";
const LAST_RESULT_KEY = "ail-chat.lastUpdateResult";

export interface ReleaseInfo {
  /** SemVer without the `ail-v` prefix (e.g. "0.4.1"). */
  version: string;
  /** Full tag name (e.g. "ail-v0.4.1"). */
  tag: string;
  /** GitHub release page URL. */
  url: string;
}

/** Compare two semver strings. Returns negative if a<b, 0 if equal, positive if a>b. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => v.split(".").map((n) => parseInt(n, 10) || 0);
  const [aMaj = 0, aMin = 0, aPat = 0] = parse(a);
  const [bMaj = 0, bMin = 0, bPat = 0] = parse(b);
  if (aMaj !== bMaj) return aMaj - bMaj;
  if (aMin !== bMin) return aMin - bMin;
  return aPat - bPat;
}

/**
 * Check for the latest `ail-v*` release on GitHub.
 *
 * Returns the latest release info, or null if the check is throttled or
 * the network call fails. Updates `globalState` with the new last-check
 * timestamp and result on success.
 */
export async function checkForLatestRelease(
  context: vscode.ExtensionContext,
  options: { force?: boolean } = {}
): Promise<ReleaseInfo | null> {
  const cfg = vscode.workspace.getConfiguration("ail-chat");
  const intervalDays = Math.max(0, cfg.get<number>("updateCheckIntervalDays", 1));

  if (!options.force) {
    const lastCheck = context.globalState.get<number>(LAST_CHECK_KEY, 0);
    const elapsedMs = Date.now() - lastCheck;
    const intervalMs = intervalDays * 24 * 60 * 60 * 1000;
    if (elapsedMs < intervalMs) {
      // Throttled — return cached result if any.
      return context.globalState.get<ReleaseInfo>(LAST_RESULT_KEY) ?? null;
    }
  }

  try {
    const url = `https://api.github.com/repos/${REPO}/releases?per_page=20`;
    const response = await fetch(url, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!response.ok) {
      throw new Error(`GitHub API returned HTTP ${response.status}`);
    }
    const releases = (await response.json()) as Array<{
      tag_name?: string;
      html_url?: string;
      draft?: boolean;
      prerelease?: boolean;
    }>;

    const ailRelease = releases.find(
      (r) =>
        typeof r.tag_name === "string" &&
        r.tag_name.startsWith("ail-v") &&
        !r.draft &&
        !r.prerelease
    );

    if (!ailRelease || !ailRelease.tag_name) {
      // No matching release — record the check timestamp anyway so we don't
      // hammer the API; null out the stored result.
      await context.globalState.update(LAST_CHECK_KEY, Date.now());
      await context.globalState.update(LAST_RESULT_KEY, undefined);
      return null;
    }

    const info: ReleaseInfo = {
      version: ailRelease.tag_name.replace(/^ail-v/, ""),
      tag: ailRelease.tag_name,
      url: ailRelease.html_url ?? `https://github.com/${REPO}/releases/tag/${ailRelease.tag_name}`,
    };

    await context.globalState.update(LAST_CHECK_KEY, Date.now());
    await context.globalState.update(LAST_RESULT_KEY, info);
    return info;
  } catch (err) {
    // Silent failure — log to console but don't bother the user.
    console.warn("ail update check failed:", err);
    return null;
  }
}
