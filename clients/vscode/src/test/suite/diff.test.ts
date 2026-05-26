import * as assert from "node:assert";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  type RepoContext,
  waitForActiveEditorUri,
  waitForDocContent,
  waitForVisibleEditorUri,
} from "../fixtures";

suite("E2E: virtual diff URI scheme", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
    // Create a file in the working copy so the diff is non-empty and the
    // server has something concrete to resolve to a change-id.
    writeFileSync(join(ctx.repoPath, "hello.txt"), "hello world\n");
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  test("badjuju.diff.cursor opens a badjuju-diff:///change/<id> URI", async () => {
    await vscode.commands.executeCommand("badjuju.status.open");
    const statusUri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/status.jujutsu"),
    );
    assert.ok(statusUri, "Status editor must be active before diff test");

    await vscode.commands.executeCommand("badjuju.diff.cursor");
    // Diff opens ViewColumn.Beside so it may not be the active editor; check
    // all visible editors instead.
    const uri = await waitForVisibleEditorUri(
      (u) => u.scheme === "badjuju-diff" && u.path.startsWith("/change/"),
    );
    assert.ok(uri, "Expected badjuju-diff:///change/<id> URI");
    assert.strictEqual(uri.scheme, "badjuju-diff");
    assert.ok(
      uri.path.startsWith("/change/"),
      `Expected /change/ path, got ${uri.path}`,
    );
  });

  test("change-mode diff content includes CHANGE_ID header", async () => {
    await vscode.commands.executeCommand("badjuju.diff.cursor");
    const uri = await waitForVisibleEditorUri(
      (u) => u.scheme === "badjuju-diff" && u.path.startsWith("/change/"),
    );
    assert.ok(uri);
    // Poll: DiffContentProvider fetches content via async LSP request and
    // silently returns "" on first-load race.
    const text = await waitForDocContent(uri, (t) =>
      t.startsWith("CHANGE_ID:"),
    );
    assert.ok(text, "Expected CHANGE_ID: header, content remained empty");
  });

  test("change-mode diff document language is jujutsu", async () => {
    await vscode.commands.executeCommand("badjuju.diff.cursor");
    const uri = await waitForVisibleEditorUri(
      (u) => u.scheme === "badjuju-diff" && u.path.startsWith("/change/"),
    );
    assert.ok(uri);
    const doc = await vscode.workspace.openTextDocument(uri);
    assert.strictEqual(doc.languageId, "jujutsu");
  });

  test("badjuju.diff.cursor.commit opens a badjuju-diff:///commit/<id> URI", async () => {
    await vscode.commands.executeCommand("badjuju.status.open");
    await waitForActiveEditorUri((u) => u.path.endsWith("/status.jujutsu"));

    await vscode.commands.executeCommand("badjuju.diff.cursor.commit");
    const uri = await waitForVisibleEditorUri(
      (u) => u.scheme === "badjuju-diff" && u.path.startsWith("/commit/"),
    );
    assert.ok(uri, "Expected badjuju-diff:///commit/<id> URI");
    assert.ok(
      uri.path.startsWith("/commit/"),
      `Expected /commit/ path, got ${uri.path}`,
    );
  });

  test("commit-mode diff content includes COMMIT_ID header", async () => {
    await vscode.commands.executeCommand("badjuju.diff.cursor.commit");
    const uri = await waitForVisibleEditorUri(
      (u) => u.scheme === "badjuju-diff" && u.path.startsWith("/commit/"),
    );
    assert.ok(uri);
    const text = await waitForDocContent(uri, (t) =>
      t.startsWith("COMMIT_ID:"),
    );
    assert.ok(text, "Expected COMMIT_ID: header, content remained empty");
  });

  test("no diff-change-*.jujutsu files written to disk (virtualDiffs is true)", () => {
    const { readdirSync, existsSync } = require("node:fs");
    const badjujuDir = join(ctx.repoPath, ".jj", "badjuju");
    if (!existsSync(badjujuDir)) return;
    const files = readdirSync(badjujuDir) as string[];
    const diskDiffs = files.filter(
      (f: string) =>
        f.startsWith("diff-change-") || f.startsWith("diff-commit-"),
    );
    assert.deepStrictEqual(
      diskDiffs,
      [],
      `Unexpected disk diff files (virtualDiffs should prevent these): ${diskDiffs}`,
    );
  });
});
