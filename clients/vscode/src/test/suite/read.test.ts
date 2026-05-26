import * as assert from "node:assert";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  type RepoContext,
  waitForActiveEditorUri,
} from "../fixtures";

suite("E2E: read commands", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  test("badjuju.status.open opens a badjuju-status: URI ending in /status.jujutsu", async () => {
    await vscode.commands.executeCommand("badjuju.status.open");
    const uri = await waitForActiveEditorUri(
      (u) =>
        u.scheme === "badjuju-status" && u.path.endsWith("/status.jujutsu"),
    );
    assert.ok(uri, "Expected status URI to become the active editor");
    assert.strictEqual(uri.scheme, "badjuju-status");
    assert.ok(uri.path.endsWith("/status.jujutsu"));
  });

  test("badjuju.log.open opens a badjuju-status: URI ending in /log.jujutsu", async () => {
    await vscode.commands.executeCommand("badjuju.log.open");
    const uri = await waitForActiveEditorUri(
      (u) => u.scheme === "badjuju-status" && u.path.endsWith("/log.jujutsu"),
    );
    assert.ok(uri, "Expected log URI to become the active editor");
    assert.ok(uri.path.endsWith("/log.jujutsu"));
    const doc = await vscode.workspace.openTextDocument(uri);
    assert.ok(
      doc.getText().startsWith("REVSET:"),
      `Expected log to start with REVSET: header, got:\n${doc.getText().substring(0, 100)}`,
    );
  });

  test("badjuju.describe.open opens a file: URI (editable, not readonly)", async () => {
    // Ensure a status editor (not log) is active so cursor args resolve to @.
    await vscode.commands.executeCommand("badjuju.status.open");
    await waitForActiveEditorUri((u) => u.path.endsWith("/status.jujutsu"));

    await vscode.commands.executeCommand("badjuju.describe.open");
    const uri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/describe.jujutsu"),
    );
    assert.ok(uri, "Expected describe URI to become the active editor");
    assert.strictEqual(
      uri.scheme,
      "file",
      "describe.jujutsu should open as a file:// URI",
    );
  });
});
