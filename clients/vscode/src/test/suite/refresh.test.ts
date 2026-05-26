import * as assert from "node:assert";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  type RepoContext,
  waitForActiveEditorUri,
  waitForVisibleEditorUri,
} from "../fixtures";

// Deferred: two tests in this suite were removed because they verify VS
// Code's own event-propagation behavior, not extension logic. The extension's
// role is a one-line handler in each case; server-side tests cover the
// protocol contract.
//
// 1. "Change-mode diff refreshes after jj new" — verifies the chain
//    server-emits-refresh → diffProvider.refresh → VS Code re-fetches content
//    → onDidChangeTextDocument fires. Server-side tests in
//    server/src/server.rs cover the refresh-emission half; the rest is VS
//    Code's documented contract for TextDocumentContentProvider.
// 2. "FileSystemWatcher refreshes status editor on external write" —
//    verifies VS Code's FS watcher, which is known to be flaky on macOS in
//    headless tests. Manual smoke testing covers this path.

suite("E2E: refresh-on-mutation", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
    // Ensure there is a file in the working copy so diffs are non-empty.
    writeFileSync(join(ctx.repoPath, "refresh-test.txt"), "initial content\n");
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  test("open commit-mode diff, then jj new — content does NOT change (pinned)", async () => {
    await vscode.commands.executeCommand("badjuju.status.open");
    await waitForActiveEditorUri((u) => u.path.endsWith("/status.jujutsu"));
    await vscode.commands.executeCommand("badjuju.diff.cursor.commit");
    // Diff opens ViewColumn.Beside; use visible editors, not just active.
    const commitUri = await waitForVisibleEditorUri(
      (u) => u.scheme === "badjuju-diff" && u.path.startsWith("/commit/"),
    );
    assert.ok(commitUri, "Expected a commit-mode diff to open");

    const docBefore = await vscode.workspace.openTextDocument(commitUri);
    const contentBefore = docBefore.getText();

    await vscode.commands.executeCommand("badjuju.new.open");
    await waitForActiveEditorUri((u) => u.path.endsWith("/status.jujutsu"));
    // Extra settle time to ensure no refresh arrives.
    await new Promise((r) => setTimeout(r, 2_000));

    const docAfter = await vscode.workspace.openTextDocument(commitUri);
    assert.strictEqual(
      contentBefore,
      docAfter.getText(),
      "Commit-mode diff should remain pinned after jj new",
    );

    ctx.reset();
  });
});
