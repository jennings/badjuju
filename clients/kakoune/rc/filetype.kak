hook global BufCreate .*\.jujutsu %{
    set-option buffer filetype jujutsu
}
