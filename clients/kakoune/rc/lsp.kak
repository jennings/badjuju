# kak-lsp glue for the badjuju LSP server.
# Filled out in sub-issue 2 (commands.kak / command dispatch).

hook global WinSetOption filetype=jujutsu %{
    lsp-enable-window
}
