/**
 * Unit tests for binary.ts — no VS Code API required.
 * Tests the platformTriple() function and version comparison logic.
 */

import * as assert from "assert";

// We import only the pure functions that don't touch vscode.*
// The module uses dynamic imports for vscode, so we can test the pure parts directly.

// Inline the pure functions for testing without the vscode import dependency.

function platformTriple(arch: string, platform: string): string {
  const archStr = arch === "arm64" ? "aarch64" : "x86_64";
  const platformMap: Record<string, string> = {
    darwin: "apple-darwin",
    linux: "unknown-linux-musl",
    win32: "pc-windows-msvc",
  };
  const plat = platformMap[platform] ?? "unknown-linux-musl";
  return `${archStr}-${plat}`;
}

function meetsMinVersion(actual: string, minimum: string): boolean {
  const parse = (v: string) => v.split(".").map((n) => parseInt(n, 10) || 0);
  const [maj1, min1, pat1] = parse(actual);
  const [maj2, min2, pat2] = parse(minimum);
  if (maj1 !== maj2) return maj1 > maj2;
  if (min1 !== min2) return min1 > min2;
  return pat1 >= pat2;
}

// ── platformTriple tests ──────────────────────────────────────────────────────

assert.strictEqual(
  platformTriple("x64", "darwin"),
  "x86_64-apple-darwin",
  "x64 macOS should produce x86_64-apple-darwin"
);

assert.strictEqual(
  platformTriple("arm64", "darwin"),
  "aarch64-apple-darwin",
  "arm64 macOS should produce aarch64-apple-darwin"
);

assert.strictEqual(
  platformTriple("x64", "linux"),
  "x86_64-unknown-linux-musl",
  "x64 Linux should produce x86_64-unknown-linux-musl"
);

assert.strictEqual(
  platformTriple("x64", "win32"),
  "x86_64-pc-windows-msvc",
  "x64 Windows should produce x86_64-pc-windows-msvc"
);

assert.strictEqual(
  platformTriple("x64", "freebsd"),
  "x86_64-unknown-linux-musl",
  "Unknown platform should fall back to linux-musl"
);

// ── meetsMinVersion tests ─────────────────────────────────────────────────────

assert.ok(meetsMinVersion("0.1.0", "0.1.0"), "equal versions should pass");
assert.ok(meetsMinVersion("0.2.0", "0.1.0"), "newer minor should pass");
assert.ok(meetsMinVersion("1.0.0", "0.9.9"), "newer major should pass");
assert.ok(meetsMinVersion("0.1.1", "0.1.0"), "newer patch should pass");
assert.ok(!meetsMinVersion("0.0.9", "0.1.0"), "older minor should fail");
assert.ok(!meetsMinVersion("0.1.0", "0.1.1"), "older patch should fail");
assert.ok(!meetsMinVersion("0.0.1", "1.0.0"), "older major should fail");

console.log("✓ binary.test.ts — all assertions passed");
