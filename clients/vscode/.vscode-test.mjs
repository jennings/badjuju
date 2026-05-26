import { execSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig } from "@vscode/test-cli";

// Pre-create a jj git repo so the extension's LSP server initializes with a
// real workspace root. Must happen before VS Code launches since vscode.openFolder
// reloads the window and kills the running test suite.
const e2eWorkspace = mkdtempSync(join(tmpdir(), "badjuju-e2e-"));
execSync("jj git init --quiet", {
  cwd: e2eWorkspace,
  env: { ...process.env, JJ_USER: "Test User", JJ_EMAIL: "test@example.com" },
});

export default defineConfig({
  files: "out/test/suite/**/*.test.js",
  extensionDevelopmentPath: ".",
  workspaceFolder: e2eWorkspace,
  mocha: {
    ui: "tdd",
    timeout: 30_000,
    color: true,
  },
  env: { BADJUJU_E2E_WORKSPACE: e2eWorkspace },
});
