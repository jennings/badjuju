import * as assert from "node:assert";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  jj,
  type RepoContext,
  waitForActiveEditorUri,
  waitForDocContent,
} from "../fixtures";

suite("E2E: prompted commands and config propagation", () => {
  let ctx: RepoContext;
  // biome-ignore lint/suspicious/noExplicitAny: patching vscode.window properties to stub prompts in tests
  const win = vscode.window as any;
  let originalShowInputBox: typeof vscode.window.showInputBox;
  let originalShowQuickPick: typeof vscode.window.showQuickPick;

  suiteSetup(async () => {
    ctx = await getRepoContext();
    originalShowInputBox = vscode.window.showInputBox;
    originalShowQuickPick = vscode.window.showQuickPick;
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  teardown(() => {
    win.showInputBox = originalShowInputBox;
    win.showQuickPick = originalShowQuickPick;
  });

  // --- badjuju.log.prompt ---

  test("badjuju.log.prompt: user-supplied revset is forwarded to server", async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    win.showInputBox = async () => "@";
    await vscode.commands.executeCommand("badjuju.log.prompt");
    const uri = await waitForActiveEditorUri(
      (u) => u.scheme === "badjuju-status" && u.path.endsWith("/log.jujutsu"),
    );
    assert.ok(uri, "Expected log URI to become active after log.prompt");
    const text = await waitForDocContent(
      uri,
      (t) => t.startsWith("REVSET: @"),
      5_000,
    );
    assert.ok(
      text,
      `Expected log to start with 'REVSET: @' (timed out waiting for refresh)`,
    );
  });

  test("badjuju.log.prompt: cancellation does not open a log editor", async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    win.showInputBox = async () => undefined;
    await vscode.commands.executeCommand("badjuju.log.prompt");
    await new Promise((r) => setTimeout(r, 1_000));
    const activeUri = vscode.window.activeTextEditor?.document.uri;
    assert.ok(
      !activeUri || !activeUri.path.endsWith("/log.jujutsu"),
      `Expected no log editor after cancellation, but found: ${activeUri?.toString()}`,
    );
  });

  // --- badjuju.rebase.prompt ---

  test("badjuju.rebase.prompt: cancellation does not execute rebase", async () => {
    const changeIdBefore = jj(ctx.repoPath, [
      "log",
      "-r@",
      "--no-graph",
      "-T",
      "change_id.short()",
    ]);
    win.showInputBox = async () => undefined;
    await vscode.commands.executeCommand("badjuju.rebase.prompt");
    await new Promise((r) => setTimeout(r, 500));
    const changeIdAfter = jj(ctx.repoPath, [
      "log",
      "-r@",
      "--no-graph",
      "-T",
      "change_id.short()",
    ]);
    assert.strictEqual(
      changeIdBefore,
      changeIdAfter,
      "Cancelling rebase prompt must not mutate the repo",
    );
  });

  // --- badjuju.bookmark.prompt ---

  test("badjuju.bookmark.prompt create: bookmark is created at @ revision", async () => {
    const bookmarkName = `e2e-bm-${Date.now()}`;
    win.showQuickPick = async () => ({
      label: "create",
      description: "Create a new bookmark at cursor revision",
    });
    win.showInputBox = async () => bookmarkName;
    await vscode.commands.executeCommand("badjuju.bookmark.prompt");
    await new Promise((r) => setTimeout(r, 1_000));
    const bookmarks = jj(ctx.repoPath, ["bookmark", "list"]);
    assert.ok(
      bookmarks.includes(bookmarkName),
      `Expected bookmark '${bookmarkName}' to exist; got:\n${bookmarks}`,
    );
    jj(ctx.repoPath, ["bookmark", "delete", bookmarkName]);
  });

  test("badjuju.bookmark.prompt cancellation at QuickPick: no mutation occurs", async () => {
    win.showQuickPick = async () => undefined;
    const changeIdBefore = jj(ctx.repoPath, [
      "log",
      "-r@",
      "--no-graph",
      "-T",
      "change_id.short()",
    ]);
    await vscode.commands.executeCommand("badjuju.bookmark.prompt");
    await new Promise((r) => setTimeout(r, 500));
    const changeIdAfter = jj(ctx.repoPath, [
      "log",
      "-r@",
      "--no-graph",
      "-T",
      "change_id.short()",
    ]);
    assert.strictEqual(
      changeIdBefore,
      changeIdAfter,
      "Cancelling at QuickPick must not mutate the repo",
    );
  });

  test("badjuju.bookmark.prompt delete: does not require a revision arg", async () => {
    // delete sub-action should send revision = '' to the server.
    // Create a bookmark first so delete has something to target.
    const bookmarkName = `e2e-del-${Date.now()}`;
    jj(ctx.repoPath, ["bookmark", "create", bookmarkName, "-r@"]);

    win.showQuickPick = async () => ({
      label: "delete",
      description: "Delete a bookmark",
    });
    win.showInputBox = async () => bookmarkName;
    await vscode.commands.executeCommand("badjuju.bookmark.prompt");
    await new Promise((r) => setTimeout(r, 1_000));
    const bookmarks = jj(ctx.repoPath, ["bookmark", "list"]);
    assert.ok(
      !bookmarks.includes(bookmarkName),
      `Expected bookmark '${bookmarkName}' to be deleted; got:\n${bookmarks}`,
    );
  });

  // --- config propagation ---

  test("keymapProfile default is not 'none', so keymapsActive is true", () => {
    const config = vscode.workspace.getConfiguration("badjuju");
    const profile = config.get<string>("keymapProfile") ?? "magit";
    assert.notStrictEqual(
      profile,
      "none",
      `keymapProfile is '${profile}'; expected any active profile (not 'none')`,
    );
  });
});
