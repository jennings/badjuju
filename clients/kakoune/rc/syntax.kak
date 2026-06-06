add-highlighter shared/jujutsu regions
add-highlighter shared/jujutsu/code default-region group

# JJ: comment lines
add-highlighter shared/jujutsu/code/ regex '^(JJ:.*)$' 1:comment

# 8- or 12-character hex commit-id prefixes (e.g. "xxxxxxxx" or "xxxxxxxxxxxx ")
add-highlighter shared/jujutsu/code/ regex '\b([0-9a-f]{12})\b' 1:type
add-highlighter shared/jujutsu/code/ regex '\b([0-9a-f]{8})\b' 1:type

hook global WinSetOption filetype=jujutsu %{
    add-highlighter window/jujutsu ref jujutsu
    hook -once -always window WinSetOption filetype=.* %{
        remove-highlighter window/jujutsu
    }
}
