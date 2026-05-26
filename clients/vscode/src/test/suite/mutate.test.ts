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

suite("E2E: mutating commands", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
    // Create a file so the working copy is non-empty.
    writeFileSync(join(ctx.repoPath, "mutate-test.txt"), "hello\n");
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  async function openStatusAndGetEditor() {
    await vscode.commands.executeCommand("badjuju.status.open");
    return waitForActiveEditorUri((u) => u.path.endsWith("/status.jujutsu"));
  }

  function assertStatusUri(uri: vscode.Uri | undefined, label: string) {
    assert.ok(uri, `${label}: expected a URI`);
    assert.strictEqual(
      uri.scheme,
      "badjuju-status",
      `${label}: expected badjuju-status scheme, got ${uri.scheme}`,
    );
    assert.ok(
      uri.path.endsWith("/status.jujutsu"),
      `${label}: expected status.jujutsu path, got ${uri.path}`,
    );
  }

  test("badjuju.new.open opens status after creating a new commit", async () => {
    const changeIdBefore = jj(ctx.repoPath, [
      "log",
      "-r@",
      "-T",
      "change_id.short()",
    ]);

    await vscode.commands.executeCommand("badjuju.new.open");
    const uri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/status.jujutsu"),
    );
    assertStatusUri(uri, "new.open");

    const changeIdAfter = jj(ctx.repoPath, [
      "log",
      "-r@",
      "-T",
      "change_id.short()",
    ]);
    assert.notStrictEqual(
      changeIdBefore,
      changeIdAfter,
      "jj new should produce a new change",
    );

    ctx.reset();
  });

  test("badjuju.undo.open reverses the last operation and opens status", async () => {
    // First create a new commit so there is something to undo.
    jj(ctx.repoPath, ["new", "-m", "temp commit for undo test"]);
    const changeIdBefore = jj(ctx.repoPath, [
      "log",
      "-r@",
      "-T",
      "change_id.short()",
    ]);

    await vscode.commands.executeCommand("badjuju.undo.open");
    const uri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/status.jujutsu"),
    );
    assertStatusUri(uri, "undo.open");

    const changeIdAfter = jj(ctx.repoPath, [
      "log",
      "-r@",
      "-T",
      "change_id.short()",
    ]);
    assert.notStrictEqual(
      changeIdBefore,
      changeIdAfter,
      "jj undo should restore the previous @",
    );
  });

  test("badjuju.next.open moves working copy forward", async () => {
    // Create a commit chain: root → A → B (@ = B).
    jj(ctx.repoPath, ["new", "-m", "commit A"]);
    const changeA = jj(ctx.repoPath, [
      "log",
      "-r@",
      "--no-graph",
      "-T",
      "change_id.short()",
    ]);
    jj(ctx.repoPath, ["new", "-m", "commit B"]);

    // Move @ back to A so we can go next.
    jj(ctx.repoPath, ["edit", "-r", changeA]);

    await openStatusAndGetEditor();
    await vscode.commands.executeCommand("badjuju.next.open");
    const uri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/status.jujutsu"),
    );
    assertStatusUri(uri, "next.open");

    // Restore.
    ctx.reset();
    ctx.reset();
    ctx.reset();
  });

  test("badjuju.prev.open moves working copy backward", async () => {
    // Create a commit: root → A, edit A, then prev should go to root.
    jj(ctx.repoPath, ["new", "-m", "commit A for prev test"]);

    await openStatusAndGetEditor();
    await vscode.commands.executeCommand("badjuju.prev.open");
    const uri = await waitForActiveEditorUri((u) =>
      u.path.endsWith("/status.jujutsu"),
    );
    assertStatusUri(uri, "prev.open");

    ctx.reset();
  });
});
