import * as assert from "node:assert";
import { DIFF_SCHEME } from "../../lib/uri";
import { windowTypeForUri } from "../../lib/windowType";

suite("windowTypeForUri", () => {
  const uri = (path: string, scheme = "file") => ({ path, scheme });

  test("undefined uri → 'status'", () => {
    assert.strictEqual(windowTypeForUri(undefined), "status");
  });

  test("status.jujutsu → 'status'", () => {
    assert.strictEqual(windowTypeForUri(uri("/status.jujutsu")), "status");
  });

  test("log.jujutsu → 'log'", () => {
    assert.strictEqual(windowTypeForUri(uri("/log.jujutsu")), "log");
  });

  test("diff.jujutsu → 'diff'", () => {
    assert.strictEqual(windowTypeForUri(uri("/diff.jujutsu")), "diff");
  });

  test("badjuju-diff:// scheme → 'diff'", () => {
    assert.strictEqual(
      windowTypeForUri(uri("/change/abc123", DIFF_SCHEME)),
      "diff",
    );
  });

  test("describe.jujutsu → 'describe'", () => {
    assert.strictEqual(windowTypeForUri(uri("/describe.jujutsu")), "describe");
  });

  test("unrecognised filename → 'status' default", () => {
    assert.strictEqual(windowTypeForUri(uri("/some-other.jujutsu")), "status");
  });

  test("log wins over status when path matches log", () => {
    assert.strictEqual(windowTypeForUri(uri("/foo/log.jujutsu")), "log");
  });
});
