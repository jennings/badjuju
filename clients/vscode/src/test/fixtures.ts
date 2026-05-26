/**
 * E2E test fixtures for the Bad Juju VS Code extension.
 *
 * The jj repo used by E2E tests is pre-created in .vscode-test.mjs before
 * VS Code launches (so the LSP server picks it up at initialize time). The
 * path is passed via BADJUJU_E2E_WORKSPACE env variable.
 *
 * Usage pattern:
 *
 *   suite("My E2E suite", () => {
 *     let ctx: RepoContext;
 *
 *     suiteSetup(async () => { ctx = await getRepoContext(); });
 *     suiteTeardown(() => ctx.dispose());
 *
 *     test("something works", async () => {
 *       const uri = await waitForActiveEditorUri(u => u.scheme === "badjuju-status");
 *       assert.ok(uri);
 *     });
 *   });
 *
 * Tests that mutate the repo should call ctx.reset() or use jj undo to restore
 * state so subsequent tests start clean.
 */

import { spawnSync } from "node:child_process";
import * as vscode from "vscode";

export interface RepoContext {
  repoPath: string;
  /** Run jj undo to roll back the most recent operation. */
  reset(): void;
  /** No-op: the workspace is shared for the full test run. */
  dispose(): void;
}

const JJ_ENV = {
  ...process.env,
  JJ_USER: "Test User",
  JJ_EMAIL: "test@example.com",
};

/**
 * Get the shared E2E workspace context. Waits for the extension to fully
 * activate if called early in suiteSetup.
 */
export async function getRepoContext(): Promise<RepoContext> {
  const repoPath = process.env.BADJUJU_E2E_WORKSPACE;
  if (!repoPath) {
    throw new Error(
      "BADJUJU_E2E_WORKSPACE is not set. " +
        "Make sure tests run via vscode-test with the .vscode-test.mjs config.",
    );
  }

  // Give the extension time to activate and complete the LSP initialize
  // handshake if this is the first suite to call getRepoContext.
  const ext = vscode.extensions.getExtension("turbocharged.badjuju-vcs");
  if (ext && !ext.isActive) {
    await ext.activate();
  }
  // Extra settle time for the LSP client to complete its handshake.
  await new Promise((r) => setTimeout(r, 4_000));

  return {
    repoPath,
    reset() {
      jj(repoPath, ["op", "undo"]);
    },
    dispose() {
      // shared workspace — nothing to tear down here
    },
  };
}

/**
 * Execute a VS Code command and wait for the active editor to change, then
 * return the new active editor's URI. Polls for up to timeoutMs.
 */
export async function runCommandAndWaitForEditor(
  commandId: string,
  timeoutMs = 10_000,
): Promise<vscode.Uri | undefined> {
  await vscode.commands.executeCommand(commandId);
  return waitForActiveEditorUri((_uri) => true, timeoutMs);
}

/**
 * Poll until the active editor's URI satisfies predicate or timeoutMs elapses.
 */
export async function waitForActiveEditorUri(
  predicate: (uri: vscode.Uri) => boolean,
  timeoutMs = 10_000,
): Promise<vscode.Uri | undefined> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const uri = vscode.window.activeTextEditor?.document.uri;
    if (uri && predicate(uri)) return uri;
    await new Promise((r) => setTimeout(r, 150));
  }
  return vscode.window.activeTextEditor?.document.uri;
}

/**
 * Run a jj command in repoPath and return trimmed stdout. Used in tests to
 * assert repo state after a VS Code command mutates it.
 */
export function jj(repoPath: string, args: string[]): string {
  const result = spawnSync("jj", ["--no-pager", "--color=never", ...args], {
    cwd: repoPath,
    env: JJ_ENV,
    encoding: "utf-8",
  });
  if (result.status !== 0) {
    throw new Error(`jj ${args.join(" ")} failed:\n${result.stderr}`);
  }
  return result.stdout.trim();
}

/**
 * Close all editors (useful in suiteTeardown to reduce noise between suites).
 */
export async function closeAllEditors(): Promise<void> {
  await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  await new Promise((r) => setTimeout(r, 200));
}
