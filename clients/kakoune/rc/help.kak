# Help system: badjuju-help <window-type>
#
# Calls badjuju.help via LSP, receives a [{key, description}] list, and
# renders it in an info popup. Falls back to a scratch buffer for long lists.

define-command badjuju-help -params 1 \
    -docstring "Show keymap help for the given window type (status|log|diff|squash|hunk-edit|describe)" %{
    evaluate-commands %sh{
        wtype="$kak_arg1"
        printf 'lsp-execute-command badjuju.help %%{["%s"]}\n' "$wtype"
    }
}

# Determine the window type from the current buffer's filename.
define-command badjuju-help-for-buffer \
    -docstring "Show keymap help appropriate for the current badjuju buffer" %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            */\.jj/badjuju/status.jujutsu)      echo 'badjuju-help status'    ;;
            */\.jj/badjuju/log.jujutsu)         echo 'badjuju-help log'       ;;
            */\.jj/badjuju/squash/*)            echo 'badjuju-help squash'    ;;
            */\.jj/badjuju/hunk-edit-*.jujutsu) echo 'badjuju-help hunk-edit' ;;
            */\.jj/badjuju/describe.jujutsu)    echo 'badjuju-help describe'  ;;
            */\.jj/badjuju/diff*.jujutsu)       echo 'badjuju-help diff'      ;;
            *)                                  echo 'badjuju-help status'    ;;
        esac
    }
}
