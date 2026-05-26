export function restoreCursor(opts: {
  cursorLine: number;
  cursorChar: number;
  lineCount: number;
  lineLength: (lineIndex: number) => number;
}): { line: number; char: number } {
  if (opts.lineCount === 0) return { line: 0, char: 0 };
  const line = Math.min(opts.cursorLine, opts.lineCount - 1);
  const char = Math.min(opts.cursorChar, opts.lineLength(line));
  return { line, char };
}
