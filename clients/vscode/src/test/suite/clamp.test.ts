import * as assert from "node:assert";
import { restoreCursor } from "../../lib/clamp";

suite("restoreCursor", () => {
  const fixedLength = (len: number) => (_i: number) => len;

  test("preserves cursor when within bounds", () => {
    const result = restoreCursor({
      cursorLine: 5,
      cursorChar: 10,
      lineCount: 20,
      lineLength: fixedLength(80),
    });
    assert.deepStrictEqual(result, { line: 5, char: 10 });
  });

  test("clamps line to lineCount - 1 when buffer shrank", () => {
    const result = restoreCursor({
      cursorLine: 15,
      cursorChar: 0,
      lineCount: 10,
      lineLength: fixedLength(80),
    });
    assert.deepStrictEqual(result, { line: 9, char: 0 });
  });

  test("clamps char to line length when line is shorter", () => {
    const result = restoreCursor({
      cursorLine: 3,
      cursorChar: 50,
      lineCount: 10,
      lineLength: fixedLength(20),
    });
    assert.deepStrictEqual(result, { line: 3, char: 20 });
  });

  test("clamps both line and char simultaneously", () => {
    const result = restoreCursor({
      cursorLine: 99,
      cursorChar: 99,
      lineCount: 5,
      lineLength: (_i) => (_i === 4 ? 3 : 80),
    });
    assert.deepStrictEqual(result, { line: 4, char: 3 });
  });

  test("lineCount 0 returns {line: 0, char: 0}", () => {
    const result = restoreCursor({
      cursorLine: 5,
      cursorChar: 10,
      lineCount: 0,
      lineLength: () => {
        throw new Error("should not be called");
      },
    });
    assert.deepStrictEqual(result, { line: 0, char: 0 });
  });

  test("cursor at exact boundary (lineCount - 1) is preserved", () => {
    const result = restoreCursor({
      cursorLine: 4,
      cursorChar: 5,
      lineCount: 5,
      lineLength: fixedLength(10),
    });
    assert.deepStrictEqual(result, { line: 4, char: 5 });
  });

  test("lineLength 0 clamps char to 0", () => {
    const result = restoreCursor({
      cursorLine: 0,
      cursorChar: 5,
      lineCount: 1,
      lineLength: fixedLength(0),
    });
    assert.deepStrictEqual(result, { line: 0, char: 0 });
  });

  test("passes correct lineIndex to lineLength after line clamp", () => {
    const visited: number[] = [];
    restoreCursor({
      cursorLine: 99,
      cursorChar: 0,
      lineCount: 3,
      lineLength: (i) => {
        visited.push(i);
        return 10;
      },
    });
    assert.deepStrictEqual(visited, [2]);
  });
});
