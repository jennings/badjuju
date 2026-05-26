import * as assert from "node:assert";
import {
  DIFF_SCHEME,
  isDescribeFile,
  isDiffFile,
  isLogFile,
  isReadonlyOutput,
  isStatusFile,
  READONLY_SCHEME,
} from "../../lib/uri";

suite("uri helpers", () => {
  // --- isStatusFile ---

  test("isStatusFile: true for /status.jujutsu", () => {
    assert.ok(
      isStatusFile({ path: "/foo/bar/status.jujutsu", scheme: "file" }),
    );
  });

  test("isStatusFile: false for /log.jujutsu", () => {
    assert.ok(!isStatusFile({ path: "/foo/log.jujutsu", scheme: "file" }));
  });

  test("isStatusFile: false for /xstatus.jujutsu", () => {
    assert.ok(!isStatusFile({ path: "/xstatus.jujutsu", scheme: "file" }));
  });

  // --- isLogFile ---

  test("isLogFile: true for /log.jujutsu", () => {
    assert.ok(isLogFile({ path: "/foo/log.jujutsu", scheme: "file" }));
  });

  test("isLogFile: false for /status.jujutsu", () => {
    assert.ok(!isLogFile({ path: "/foo/status.jujutsu", scheme: "file" }));
  });

  // --- isDiffFile ---

  test("isDiffFile: true for badjuju-diff scheme", () => {
    assert.ok(isDiffFile({ path: "/change/abc123", scheme: DIFF_SCHEME }));
  });

  test("isDiffFile: true for /diff.jujutsu", () => {
    assert.ok(isDiffFile({ path: "/foo/diff.jujutsu", scheme: "file" }));
  });

  test("isDiffFile: true for diff-change-<id>.jujutsu", () => {
    assert.ok(
      isDiffFile({
        path: "/foo/.jj/badjuju/diff-change-abc1234567890.jujutsu",
        scheme: "file",
      }),
    );
  });

  test("isDiffFile: true for diff-commit-<id>.jujutsu", () => {
    assert.ok(
      isDiffFile({
        path: "/foo/.jj/badjuju/diff-commit-abc1234567890.jujutsu",
        scheme: "file",
      }),
    );
  });

  test("isDiffFile: false for /status.jujutsu", () => {
    assert.ok(!isDiffFile({ path: "/status.jujutsu", scheme: "file" }));
  });

  test("isDiffFile: false for partial match notdiff.jujutsu", () => {
    assert.ok(!isDiffFile({ path: "/notdiff.jujutsu", scheme: "file" }));
  });

  // --- isDescribeFile ---

  test("isDescribeFile: true for /describe.jujutsu", () => {
    assert.ok(
      isDescribeFile({ path: "/foo/describe.jujutsu", scheme: "file" }),
    );
  });

  test("isDescribeFile: false for /status.jujutsu", () => {
    assert.ok(!isDescribeFile({ path: "/foo/status.jujutsu", scheme: "file" }));
  });

  // --- isReadonlyOutput ---

  test("isReadonlyOutput: true for status file", () => {
    assert.ok(isReadonlyOutput({ path: "/status.jujutsu", scheme: "file" }));
  });

  test("isReadonlyOutput: true for log file", () => {
    assert.ok(isReadonlyOutput({ path: "/log.jujutsu", scheme: "file" }));
  });

  test("isReadonlyOutput: true for diff URI", () => {
    assert.ok(isReadonlyOutput({ path: "/change/abc", scheme: DIFF_SCHEME }));
  });

  test("isReadonlyOutput: false for describe file", () => {
    assert.ok(!isReadonlyOutput({ path: "/describe.jujutsu", scheme: "file" }));
  });

  // --- constants ---

  test("READONLY_SCHEME is badjuju-status", () => {
    assert.strictEqual(READONLY_SCHEME, "badjuju-status");
  });

  test("DIFF_SCHEME is badjuju-diff", () => {
    assert.strictEqual(DIFF_SCHEME, "badjuju-diff");
  });
});
