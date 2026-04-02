/**
 * VS Code integration test runner.
 * Downloads VS Code and runs the test suite inside it.
 *
 * Run with: npm run test:integration
 * Requires: a display server (xvfb or real display) or a headless environment.
 */

import * as path from "path";
import { runTests } from "@vscode/test-electron";

async function main(): Promise<void> {
  const extensionDevelopmentPath = path.resolve(__dirname, "../../");
  const extensionTestsPath = path.resolve(__dirname, "./suite/index");

  await runTests({ extensionDevelopmentPath, extensionTestsPath });
}

main().catch((err) => {
  console.error("VS Code integration tests failed:", err);
  process.exit(1);
});
