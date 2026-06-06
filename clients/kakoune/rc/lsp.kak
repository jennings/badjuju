# kak-lsp glue and command dispatch for badjuju.
#
# Requires kak-lsp 0.14+. The server communicates via:
#   - workspace/executeCommand for user actions (returns a file:// URI)
#   - workspace/applyEdit for pushing state updates to open buffers

hook global WinSetOption filetype=jujutsu %{
    lsp-enable-window
}

# Execute a badjuju LSP command.
# Usage: badjuju-execute <server-command> [json-args]
#
# After execution, kak-lsp opens the returned file:// URI automatically
# (via its built-in execute-command response handling). Open buffers for
# already-loaded badjuju files auto-refresh via workspace/applyEdit.
#
# The describe.jujutsu protect logic: kak-lsp will not re-open a buffer
# that is already focused. Since badjuju-execute is never called from
# inside describe.jujutsu for state-changing ops, no special guard is
# needed here.
define-command badjuju-execute -params 1..2 %{
    evaluate-commands %sh{
        args="${2:-[]}"
        printf 'lsp-execute-command %%{%s} %%{%s}\n' "$1" "$args"
    }
}

# Execute a badjuju command with the cursor position as the first argument.
# Builds {"cursor":{"uri":"file://...","line":N}} from the current position.
define-command badjuju-execute-at-cursor -params 1 %{
    evaluate-commands %sh{
        cmd="$1"
        uri="file://$kak_buffile"
        line=$((kak_cursor_line - 1))
        cursor_json="{\"cursor\":{\"uri\":\"$uri\",\"line\":$line}}"
        printf 'lsp-execute-command %%{%s} %%{[%s]}\n' "$cmd" "$cursor_json"
    }
}

# Return the cursor JSON for the current buffer and position.
# Useful for building command arguments inline.
define-command -hidden badjuju-cursor-json -params 0 \
    -docstring "Print cursor JSON arg for current position" %{
    evaluate-commands %sh{
        uri="file://$kak_buffile"
        line=$((kak_cursor_line - 1))
        printf '{"cursor":{"uri":"%s","line":%d}}' "$uri" "$line"
    }
}
