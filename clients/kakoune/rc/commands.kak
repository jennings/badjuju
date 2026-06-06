# User-facing :JJ* commands.
# Command args in %sh{} blocks are accessible as $kak_arg1, $kak_arg2, etc.
# Cursor arguments are sent only from status.jujutsu and log.jujutsu.

define-command -override JJStatus \
    -docstring "Open the jujutsu status buffer" %{
    lsp-execute-command badjuju.status %{[]}
}

define-command -override JJLog \
    -params 0..1 \
    -docstring "Open the jujutsu log buffer (optional revset argument)" %{
    evaluate-commands %sh{
        if [ -n "$kak_arg1" ]; then
            printf 'lsp-execute-command badjuju.log %%{["%s"]}\n' "$kak_arg1"
        else
            printf 'lsp-execute-command badjuju.log %%{[]}\n'
        fi
    }
}

define-command -override JJLogFile \
    -docstring "Open per-file log for the file under cursor (or current buffer)" %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            */\.jj/badjuju/status.jujutsu)
                line=$((kak_cursor_line - 1))
                printf 'lsp-execute-command badjuju.log.file %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                    "$kak_buffile" "$line"
                ;;
            *)
                printf 'lsp-execute-command badjuju.log.file %%{["%s"]}\n' "$kak_buffile"
                ;;
        esac
    }
}

define-command -override JJDescribe \
    -params 0..1 \
    -docstring "Open describe.jujutsu for the revision at cursor (or given revision)" %{
    evaluate-commands %sh{
        if [ -n "$kak_arg1" ]; then
            printf 'lsp-execute-command badjuju.describe %%{["%s"]}\n' "$kak_arg1"
        else
            line=$((kak_cursor_line - 1))
            printf 'lsp-execute-command badjuju.describe %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                "$kak_buffile" "$line"
        fi
    }
}

define-command -override JJDiff \
    -params 0..1 \
    -docstring "Open the change diff for the revision at cursor (or given revision)" %{
    evaluate-commands %sh{
        if [ -n "$kak_arg1" ]; then
            printf 'lsp-execute-command badjuju.diff %%{["%s"]}\n' "$kak_arg1"
        else
            line=$((kak_cursor_line - 1))
            printf 'lsp-execute-command badjuju.diff %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                "$kak_buffile" "$line"
        fi
    }
}

define-command -override JJDiffCommit \
    -params 0..1 \
    -docstring "Open a pinned commit diff for the revision at cursor (or given revision)" %{
    evaluate-commands %sh{
        if [ -n "$kak_arg1" ]; then
            printf 'lsp-execute-command badjuju.diff.commit %%{["%s"]}\n' "$kak_arg1"
        else
            line=$((kak_cursor_line - 1))
            printf 'lsp-execute-command badjuju.diff.commit %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                "$kak_buffile" "$line"
        fi
    }
}

define-command -override JJNew \
    -docstring "Create a new change (child of revision at cursor, or @)" %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            */\.jj/badjuju/status.jujutsu|*/\.jj/badjuju/log.jujutsu)
                line=$((kak_cursor_line - 1))
                printf 'lsp-execute-command badjuju.new %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                    "$kak_buffile" "$line"
                ;;
            *)
                printf 'lsp-execute-command badjuju.new %%{[]}\n'
                ;;
        esac
    }
}

define-command -override JJNext \
    -docstring "Move @ to the next child" %{
    lsp-execute-command badjuju.next %{[]}
}

define-command -override JJPrev \
    -docstring "Move @ to the previous parent" %{
    lsp-execute-command badjuju.prev %{[]}
}

define-command -override JJRefresh \
    -docstring "Refresh the current badjuju buffer" %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            *.jujutsu)
                printf 'lsp-execute-command badjuju.refresh %%{["file://%s"]}\n' "$kak_buffile"
                ;;
            *)
                printf 'lsp-execute-command badjuju.refresh %%{[""]}\n'
                ;;
        esac
    }
}

define-command -override JJSquash \
    -docstring "Squash the file under the cursor into its parent" %{
    evaluate-commands %sh{
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.squash %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$kak_buffile" "$line"
    }
}

define-command -override JJUnsquash \
    -docstring "Unsquash the file under the cursor from parent into child" %{
    evaluate-commands %sh{
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.unsquash %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$kak_buffile" "$line"
    }
}

define-command -override JJUndo \
    -docstring "Undo the last jj operation" %{
    lsp-execute-command badjuju.undo %{[]}
}

define-command -override JJAbandon \
    -params 0..1 \
    -docstring "Abandon the revision at cursor (or given revision)" %{
    evaluate-commands %sh{
        if [ -n "$kak_arg1" ]; then
            printf 'lsp-execute-command badjuju.abandon %%{["%s"]}\n' "$kak_arg1"
        else
            line=$((kak_cursor_line - 1))
            printf 'lsp-execute-command badjuju.abandon %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                "$kak_buffile" "$line"
        fi
    }
}

define-command -override JJEdit \
    -params 0..1 \
    -docstring "Move @ to the revision at cursor (or given revision)" %{
    evaluate-commands %sh{
        if [ -n "$kak_arg1" ]; then
            printf 'lsp-execute-command badjuju.edit %%{["%s"]}\n' "$kak_arg1"
        else
            line=$((kak_cursor_line - 1))
            printf 'lsp-execute-command badjuju.edit %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                "$kak_buffile" "$line"
        fi
    }
}

define-command -override JJFetch \
    -docstring "Run jj git fetch" %{
    lsp-execute-command badjuju.fetch %{[]}
}

define-command -override JJPush \
    -params 0..1 \
    -docstring "Run jj git push; pass '!' as argument for --force-with-lease" %{
    evaluate-commands %sh{
        if [ "$kak_arg1" = "!" ]; then
            printf 'lsp-execute-command badjuju.push %%{[{"forceWithLease":true}]}\n'
        else
            printf 'lsp-execute-command badjuju.push %%{[{"forceWithLease":false}]}\n'
        fi
    }
}

define-command -override JJBookmark \
    -params 2..3 \
    -docstring "Bookmark: sub-action (create|move|delete|track|forget) name [revision]" %{
    evaluate-commands %sh{
        sub="$kak_arg1"
        name="$kak_arg2"
        if [ "$sub" = "create" ] || [ "$sub" = "move" ]; then
            line=$((kak_cursor_line - 1))
            printf 'lsp-execute-command badjuju.bookmark %%{["%s","%s",{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
                "$sub" "$name" "$kak_buffile" "$line"
        else
            printf 'lsp-execute-command badjuju.bookmark %%{["%s","%s",""]}\n' "$sub" "$name"
        fi
    }
}

define-command -override JJRebaseSource \
    -params 1 \
    -docstring "Mark rebase source: mode is 'source', 'revisions', or 'branch'" %{
    evaluate-commands %sh{
        mode="$kak_arg1"
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.rebase.source %%{["%s",{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$mode" "$kak_buffile" "$line"
    }
}

define-command -override JJRebaseCommit \
    -params 1 \
    -docstring "Complete rebase: insert is 'onto', 'after', or 'before'" %{
    evaluate-commands %sh{
        insert="$kak_arg1"
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.rebase.commit %%{["%s",{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$insert" "$kak_buffile" "$line"
    }
}

define-command -override JJCancel \
    -docstring "Cancel any pending squash or rebase operation" %{
    evaluate-commands %sh{
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.cancel %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$kak_buffile" "$line"
    }
}

define-command -override JJSquashToggle \
    -docstring "Toggle hunk or file under cursor between SELECTED and REMAINING" %{
    evaluate-commands %sh{
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.squash.toggle %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$kak_buffile" "$line"
    }
}

define-command -override JJSquashSelectAll \
    -docstring "Move all changes into SELECTED" %{
    lsp-execute-command badjuju.squash.select_all %{[]}
}

define-command -override JJSquashSelectNone \
    -docstring "Move all changes back to REMAINING" %{
    lsp-execute-command badjuju.squash.select_none %{[]}
}

define-command -override JJSquashCommit \
    -docstring "Select squash source or destination (two-step commit-to-commit squash)" %{
    evaluate-commands %sh{
        line=$((kak_cursor_line - 1))
        printf 'lsp-execute-command badjuju.squash.commit %%{[{"cursor":{"uri":"file://%s","line":%d}}]}\n' \
            "$kak_buffile" "$line"
    }
}
