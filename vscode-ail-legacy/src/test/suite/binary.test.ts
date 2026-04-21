/**
 * Tests for binary.ts utilities.
 * Pure functions — no VS Code API needed.
 */

import * as assert from "assert";

// Inline the tested functions so this file has no VS Code API dependency.
// These are extracted from binary.ts for testability.

function platformTriple(platform: string, arch: string): string {
  const archMap: Record<string, string> = {
    x64: "x86_64",
    arm64: "aarch64",
    arm: "arm",
  };
  const rustArch = archMap[arch] ?? arch;
  switch (platform) {
    case "linux":  return `${rustArch}-unknown-linux-musl`;
    case "darwin": return `${rustArch}-apple-darwin`;
    case "win32":  return `${rustArch}-pc-windows-msvc`;
    default:       return `${rustArch}-unknown-${platform}`;
  }
}

function meetsMinVersion(current: string, minimum: string): boolean {
  const parse = (v: string) => v.replace(/^v/, "").split(".").map(Number);
  const [cMaj, cMin, cPat] = parse(current);
  const [mMaj, mMin, mPat] = parse(minimum);
  if (cMaj !== mMaj) return cMaj > mMaj;
  if (cMin !== mMin) return cMin > mMin;
  return cPat >= mPat;
}

suite("binary", () => {
  test("platformTriple — linux x64", () => {
    assert.strictEqual(platformTriple("linux", "x64"), "x86_64-unknown-linux-musl");
  });
  test("platformTriple — darwin arm64", () => {
    assert.strictEqual(platformTriple("darwin", "arm64"), "aarch64-apple-darwin");
  });
  test("platformTriple — win32 x64", () => {
    assert.strictEqual(platformTriple("win32", "x64"), "x86_64-pc-windows-msvc");
  });
  test("platformTriple — unknown platform", () => {
    assert.strictEqual(platformTriple("freebsd", "x64"), "x86_64-unknown-freebsd");
  });
  test("meetsMinVersion — exact match", () => {
    assert.ok(meetsMinVersion("0.1.0", "0.1.0"));
  });
  test("meetsMinVersion — newer patch", () => {
    assert.ok(meetsMinVersion("0.1.1", "0.1.0"));
  });
  test("meetsMinVersion — older version fails", () => {
    assert.ok(!meetsMinVersion("0.0.9", "0.1.0"));
  });
});
