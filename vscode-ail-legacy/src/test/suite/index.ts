/**
 * Mocha test suite root — loads all *.test.ts files in this directory.
 * Used by both the mocha CLI runner (unit tests) and @vscode/test-electron (integration).
 */

import * as path from "path";
import Mocha from "mocha";
import * as glob from "glob";

export function run(): Promise<void> {
  const mocha = new Mocha({ ui: "tdd", color: true, timeout: 10000 });

  const testsRoot = path.resolve(__dirname);
  const files = glob.sync("**/*.test.js", { cwd: testsRoot });

  for (const f of files) {
    mocha.addFile(path.resolve(testsRoot, f));
  }

  return new Promise((resolve, reject) => {
    mocha.run((failures) => {
      if (failures > 0) {
        reject(new Error(`${failures} test(s) failed.`));
      } else {
        resolve();
      }
    });
  });
}
