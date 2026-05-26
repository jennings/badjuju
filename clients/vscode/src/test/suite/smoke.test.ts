import * as assert from "node:assert";
import { existsSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import * as vscode from "vscode";

const EXT_ID = "turbocharged.badjuju-vcs";

suite("Smoke", () => {
  let tmpFile: string;

  suiteSetup(async () => {
    tmpFile = join(tmpdir(), `badjuju-smoke-${process.pid}.jujutsu`);
    writeFileSync(tmpFile, "smoke test content\n");
    const doc = await vscode.workspace.openTextDocument(
      vscode.Uri.file(tmpFile),
    );
    await vscode.window.showTextDocument(doc);
    // Wait for the extension to activate and start the LSP client.
    await new Promise((r) => setTimeout(r, 5_000));
  });

  suiteTeardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    if (existsSync(tmpFile)) rmSync(tmpFile);
  });

  test("Extension is installed", () => {
    const ext = vscode.extensions.getExtension(EXT_ID);
    assert.ok(ext, `Extension '${EXT_ID}' should be installed`);
  });

  test("Extension activates on jujutsu file", () => {
    const ext = vscode.extensions.getExtension(EXT_ID);
    assert.ok(
      ext?.isActive,
      `Extension should be active after opening a .jujutsu file`,
    );
  });

  test("Commands are registered", async () => {
    const cmds = await vscode.commands.getCommands(true);
    for (const cmd of [
      "badjuju.status.open",
      "badjuju.log.open",
      "badjuju.describe.open",
      "badjuju.diff.cursor",
      "badjuju.diff.cursor.commit",
      "badjuju.version.open",
    ]) {
      assert.ok(cmds.includes(cmd), `Command '${cmd}' should be registered`);
    }
  });
});
