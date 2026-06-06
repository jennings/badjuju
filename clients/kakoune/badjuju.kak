# Bad Juju — Kakoune plugin entry point.
#
# Manual install: source this file from your kakrc, or symlink it into
# ~/.config/kak/autoload/.
#
# plug.kak:
#   plug "jennings/bad-juju" subset [clients/kakoune/badjuju.kak] config %{
#       set-option global badjuju_keymap_profile magit
#   }

declare-option str badjuju_keymap_profile "magit"

evaluate-commands %sh{
    dir=$(dirname "$kak_source")/rc
    printf 'source %%"%s/filetype.kak"\n' "$dir"
    printf 'source %%"%s/syntax.kak"\n'   "$dir"
    printf 'source %%"%s/lsp.kak"\n'      "$dir"
    printf 'source %%"%s/commands.kak"\n' "$dir"
    printf 'source %%"%s/help.kak"\n'     "$dir"
    if [ "$kak_opt_badjuju_keymap_profile" = "vim" ]; then
        printf 'source %%"%s/keymap-vim.kak"\n' "$dir"
    else
        printf 'source %%"%s/keymap-magit.kak"\n' "$dir"
    fi
}
