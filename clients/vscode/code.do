if ! command -v code >/dev/null 2>&1; then
  echo "ERROR: 'code' CLI not found on PATH." >&2
  echo "       In VS Code: Cmd/Ctrl+Shift+P → 'Shell Command: Install code command in PATH'." >&2
  echo "       Or, to use Insiders edition, run: \`echo 'code-insiders \"$@\"' > code && chmod +x code\`" >&2
  exit 1
fi

echo 'code "$@"' > "$3"
chmod +x "$3"
