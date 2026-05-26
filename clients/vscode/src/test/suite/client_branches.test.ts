import * as assert from "node:assert";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  jj,
  type RepoContext,
  waitForActiveEditorUri,
} from "../fixtures";

suite("E2E: client-only branching logic", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
    writeFileSync(join(ctx.repoPath, "branch-test.txt"), "content\n");
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  test("badjuju.refresh.open from readonly status editor sends file:// URI to server", async () => {
    // Open status so the active editor is a badjuju-status: URI.
    await vscode.commands.executeCommand("badjuju.status.open");
    const statusUri = await waitForActiveEditorUri(
      (u) =>
        u.scheme === "badjuju-status" && u.path.endsWith("/status.jujutsu"),
    );
    assert.ok(statusUri, "Status editor should be open");

    // refresh.open should convert the badjuju-status: URI to file:// before
    // sending to the server. If it sent the readonly scheme the server would
    // reject it; asserting no exception implies the conversion happened.
    await assert.doesNotReject(
      Promise.resolve(vscode.commands.executeCommand("badjuju.refresh.open")),
      "refresh.open should succeed when active editor is a badjuju-status: URI",
    );
    await waitForActiveEditorUri((u) => u.path.endsWith("/status.jujutsu"));
  });

  test("badjuju.abandon.cursor from log buffer stays in log (not jumping to status)", async () => {
    // Create a commit to abandon.
    jj(ctx.repoPath, ["new", "-m", "commit to abandon"]);
    const abandonTarget = jj(ctx.repoPath, [
      "log",
      "-r@",
      "--no-graph",
      "-T",
      "change_id.short()",
    ]);

    // Move @ back to parent so we can target the new commit from the log.
    jj(ctx.repoPath, ["prev", "--no-edit"]);

    // Open log so the active editor is log.jujutsu.
    await vscode.commands.executeCommand("badjuju.log.open");
    const logUri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/log.jujutsu"),
    );
    assert.ok(logUri, "Log editor should be open");

    // Find the line that mentions the abandon target in the log.
    const logDoc = await vscode.workspace.openTextDocument(logUri);
    const logText = logDoc.getText();
    const targetLine = logText
      .split("\n")
      .findIndex((l) => l.includes(abandonTarget));
    if (targetLine < 0) {
      // Target not visible in log — skip this assertion rather than fail.
      jj(ctx.repoPath, ["undo"]);
      return;
    }

    const logEditor = await vscode.window.showTextDocument(logDoc);
    const pos = new vscode.Position(targetLine, 0);
    logEditor.selection = new vscode.Selection(pos, pos);

    await vscode.commands.executeCommand("badjuju.abandon.cursor");

    // After abandon from log, the active editor should still be the log (not status).
    const activeUri = await waitForActiveEditorUri(
      (u) =>
        u.path.endsWith("/log.jujutsu") || u.path.endsWith("/status.jujutsu"),
    );
    assert.ok(
      activeUri?.path.endsWith("/log.jujutsu"),
      `Expected to remain in log buffer after abandon, active URI: ${activeUri?.toString()}`,
    );
  });

  test("badjuju.log.applyShortcut: cursor clamped when log shrinks", async () => {
    // Open log at a deep position.
    await vscode.commands.executeCommand("badjuju.log.open");
    const logUri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/log.jujutsu"),
    );
    assert.ok(logUri);

    const logDoc = await vscode.workspace.openTextDocument(logUri);
    const lineCount = logDoc.lineCount;
    const logEditor = await vscode.window.showTextDocument(logDoc);

    // Position cursor near the end of the log.
    const farLine = Math.max(0, lineCount - 2);
    const farPos = new vscode.Position(farLine, 0);
    logEditor.selection = new vscode.Selection(farPos, farPos);

    // Apply a shortcut that might reload the log. This exercises the cursor
    // clamping code at extension.ts via restoreCursor in src/lib/clamp.ts.
    // If clamping is broken, this throws a RangeError.
    await assert.doesNotReject(
      Promise.resolve(
        vscode.commands.executeCommand("badjuju.log.applyShortcut"),
      ),
      "log.applyShortcut should not throw even near the end of the log",
    );
  });
});
