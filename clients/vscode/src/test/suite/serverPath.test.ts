import * as assert from "node:assert";
import { resolveServerCommand } from "../../lib/serverPath";

suite("resolveServerCommand", () => {
  const existsYes = (_p: string) => true;
  const existsNo = (_p: string) => false;

  test("SERVER_PATH env var wins over all else", () => {
    const result = resolveServerCommand({
      envServerPath: "/custom/badjuju",
      extensionPath: "/ext",
      platform: "linux",
      existsSync: existsYes,
    });
    assert.strictEqual(result, "/custom/badjuju");
  });

  test("bundled binary used when env unset and file exists", () => {
    const result = resolveServerCommand({
      envServerPath: undefined,
      extensionPath: "/ext",
      platform: "linux",
      existsSync: (p) => p.includes("out/bin"),
    });
    assert.ok(
      result.includes("out/bin"),
      `Expected bundled path, got: ${result}`,
    );
    assert.ok(result.endsWith("badjuju"), `Expected 'badjuju', got: ${result}`);
  });

  test("bundled binary on win32 uses .exe suffix", () => {
    const result = resolveServerCommand({
      envServerPath: undefined,
      extensionPath: "/ext",
      platform: "win32",
      existsSync: existsYes,
    });
    assert.ok(
      result.endsWith("badjuju.exe"),
      `Expected .exe suffix, got: ${result}`,
    );
  });

  test("falls back to PATH 'badjuju' when env unset and bundled missing", () => {
    const result = resolveServerCommand({
      envServerPath: undefined,
      extensionPath: "/ext",
      platform: "linux",
      existsSync: existsNo,
    });
    assert.strictEqual(result, "badjuju");
  });

  test("env var wins even when bundled binary exists", () => {
    const result = resolveServerCommand({
      envServerPath: "/override/badjuju",
      extensionPath: "/ext",
      platform: "linux",
      existsSync: existsYes,
    });
    assert.strictEqual(result, "/override/badjuju");
  });

  test("existsSync is called with the expected bundled path", () => {
    const checked: string[] = [];
    resolveServerCommand({
      envServerPath: undefined,
      extensionPath: "/my/ext",
      platform: "darwin",
      existsSync: (p) => {
        checked.push(p);
        return false;
      },
    });
    assert.ok(
      checked.some((p) => p.includes("my/ext") && p.includes("out/bin")),
      `Expected existsSync called with bundled path under /my/ext, got: ${checked}`,
    );
  });
});
