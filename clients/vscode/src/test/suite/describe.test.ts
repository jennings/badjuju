import * as assert from "node:assert";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  jj,
  type RepoContext,
  waitForActiveEditorUri,
} from "../fixtures";

suite("E2E: describe save flow", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  test("badjuju.describe.open opens a file:// URI", async () => {
    await vscode.commands.executeCommand("badjuju.describe.open");
    const uri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/describe.jujutsu"),
    );
    assert.ok(uri, "Expected describe URI");
    assert.strictEqual(
      uri.scheme,
      "file",
      "describe.jujutsu must open as file:// so it is editable",
    );
  });

  test("editing and saving describe.jujutsu updates the commit description", async () => {
    const uniqueDesc = `Test description ${Date.now()}`;

    await vscode.commands.executeCommand("badjuju.describe.open");
    const descUri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/describe.jujutsu"),
    );
    assert.ok(descUri);

    const doc = await vscode.workspace.openTextDocument(descUri);
    await vscode.window.showTextDocument(doc, { preview: false });

    // Replace only the description body (before the "JJ: editing this…" line).
    const text = doc.getText();
    const separatorLine = text
      .split("\n")
      .findIndex((l) => l.startsWith("JJ:"));
    const endPos =
      separatorLine >= 0
        ? new vscode.Position(separatorLine, 0)
        : new vscode.Position(doc.lineCount, 0);

    const edit = new vscode.WorkspaceEdit();
    edit.replace(
      descUri,
      new vscode.Range(new vscode.Position(0, 0), endPos),
      `${uniqueDesc}\n`,
    );
    await vscode.workspace.applyEdit(edit);
    await doc.save();

    // Wait for the server to process did_save and run jj describe.
    await new Promise((r) => setTimeout(r, 2_000));

    const actualDesc = jj(ctx.repoPath, ["log", "-r@", "-T", "description"]);
    assert.ok(
      actualDesc.includes(uniqueDesc),
      `Expected description to contain '${uniqueDesc}', got: ${actualDesc}`,
    );

    // Restore: remove description.
    jj(ctx.repoPath, ["describe", "-r@", "-m", ""]);
  });

  test("saving describe.jujutsu triggers status buffer refresh", async () => {
    const marker = `Describe refresh test ${Date.now()}`;
    jj(ctx.repoPath, ["describe", "-r@", "-m", marker]);

    // Open status — triggers server regeneration with the new description.
    // Subscribe to content changes before opening so we can wait for refresh.
    await vscode.commands.executeCommand("badjuju.status.open");
    const statusUri = await waitForActiveEditorUri(
      (u) =>
        u.scheme === "badjuju-status" && u.path.endsWith("/status.jujutsu"),
    );
    assert.ok(statusUri);

    // Give VS Code time to process the onDidChange event from statusProvider
    // and re-fetch document content via provideTextDocumentContent.
    await new Promise((r) => setTimeout(r, 1_000));

    const statusDoc = await vscode.workspace.openTextDocument(statusUri);
    assert.ok(
      statusDoc.getText().includes(marker),
      "Status should show the commit description",
    );

    // Clean up description.
    jj(ctx.repoPath, ["describe", "-r@", "-m", ""]);
  });
});
