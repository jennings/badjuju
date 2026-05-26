import * as assert from "node:assert";
import { existsSync } from "node:fs";
import { join } from "node:path";
import * as vscode from "vscode";
import {
  closeAllEditors,
  getRepoContext,
  jj,
  type RepoContext,
} from "../fixtures";

suite("E2E fixtures canary", () => {
  let ctx: RepoContext;

  suiteSetup(async () => {
    ctx = await getRepoContext();
  });

  suiteTeardown(async () => {
    await closeAllEditors();
    ctx.dispose();
  });

  test("BADJUJU_E2E_WORKSPACE points to a real .jj directory", () => {
    assert.ok(
      existsSync(join(ctx.repoPath, ".jj")),
      `.jj directory should exist at ${ctx.repoPath}`,
    );
  });

  test("extension is active", () => {
    const ext = vscode.extensions.getExtension("turbocharged.badjuju-vcs");
    assert.ok(ext?.isActive, "Extension should be active");
  });

  test("jj helper reads repo state", () => {
    const out = jj(ctx.repoPath, [
      "log",
      "--limit=1",
      "-T",
      "change_id.short()",
    ]);
    assert.ok(out.length > 0, `Expected a change id, got: ${out}`);
  });
});
