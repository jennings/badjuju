# Magit-profile keymap: Kakoune user-mode bindings.
#
# Activation: press <space> in any *.jujutsu buffer to enter badjuju mode.
# The user can override the leader by re-binding in their config.
#
# Buffer-type-aware sub-modes are used to give different bindings in
# status, log, diff, squash, and describe buffers.

declare-user-mode badjuju
declare-user-mode badjuju-status
declare-user-mode badjuju-log
declare-user-mode badjuju-diff
declare-user-mode badjuju-squash
declare-user-mode badjuju-hunk-edit
declare-user-mode badjuju-describe

# Nested modes for chord prefixes (bookmark / rebase / commit)
declare-user-mode badjuju-bookmark
declare-user-mode badjuju-rebase
declare-user-mode badjuju-commit

# --- Universal bindings (all badjuju buffers) --------------------------------

# Bookmark chord prefix
map global badjuju-bookmark c ': prompt "Bookmark name: " %{ JJBookmark create %val{text} }<ret>' \
    -docstring 'create bookmark'
map global badjuju-bookmark m ': prompt "Bookmark name: " %{ JJBookmark move %val{text} }<ret>' \
    -docstring 'move bookmark'
map global badjuju-bookmark d ': prompt "Bookmark name: " %{ JJBookmark delete %val{text} }<ret>' \
    -docstring 'delete bookmark'
map global badjuju-bookmark t ': prompt "Bookmark name (e.g. main@origin): " %{ JJBookmark track %val{text} }<ret>' \
    -docstring 'track bookmark'
map global badjuju-bookmark f ': prompt "Bookmark name: " %{ JJBookmark forget %val{text} }<ret>' \
    -docstring 'forget bookmark'

# Rebase chord prefix
map global badjuju-rebase s ': JJRebaseSource source<ret>' \
    -docstring 'mark source (--source)'
map global badjuju-rebase r ': JJRebaseSource revisions<ret>' \
    -docstring 'mark source (--revisions)'
map global badjuju-rebase b ': JJRebaseSource branch<ret>' \
    -docstring 'mark source (--branch)'
map global badjuju-rebase o ': JJRebaseCommit onto<ret>' \
    -docstring 'rebase onto this commit'
map global badjuju-rebase A ': JJRebaseCommit after<ret>' \
    -docstring 'insert after this commit'
map global badjuju-rebase B ': JJRebaseCommit before<ret>' \
    -docstring 'insert before this commit'

# Commit chord prefix
map global badjuju-commit w ': JJDescribe<ret>' \
    -docstring 'reword (open describe.jujutsu)'
map global badjuju-commit n ': JJNew<ret>' \
    -docstring 'new child commit'

# Top-level badjuju mode: dispatches to the buffer-type sub-mode.
# The hook below re-binds <space> whenever a jujutsu filetype window is set.
hook global WinSetOption filetype=jujutsu %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            */\.jj/badjuju/status.jujutsu)
                echo 'map window normal <space> ": enter-user-mode badjuju-status<ret>"'
                ;;
            */\.jj/badjuju/log.jujutsu)
                echo 'map window normal <space> ": enter-user-mode badjuju-log<ret>"'
                ;;
            */\.jj/badjuju/squash/*)
                echo 'map window normal <space> ": enter-user-mode badjuju-squash<ret>"'
                ;;
            */\.jj/badjuju/hunk-edit-*.jujutsu)
                echo 'map window normal <space> ": enter-user-mode badjuju-hunk-edit<ret>"'
                ;;
            */\.jj/badjuju/diff-*.jujutsu|*/\.jj/badjuju/diff.jujutsu)
                echo 'map window normal <space> ": enter-user-mode badjuju-diff<ret>"'
                ;;
            */\.jj/badjuju/describe.jujutsu)
                echo 'map window normal <space> ": enter-user-mode badjuju-describe<ret>"'
                ;;
            *)
                echo 'map window normal <space> ": enter-user-mode badjuju<ret>"'
                ;;
        esac
    }
    hook -once -always window WinSetOption filetype=.* %{
        unmap window normal <space>
    }
}

# --- status.jujutsu ----------------------------------------------------------

map global badjuju-status R ': JJRefresh<ret>'         -docstring 'refresh'
map global badjuju-status n ': JJNew<ret>'             -docstring 'new change'
map global badjuju-status L ': JJLog<ret>'             -docstring 'open log'
map global badjuju-status f ': JJFetch<ret>'           -docstring 'git fetch'
map global badjuju-status p ': JJPush<ret>'            -docstring 'git push'
map global badjuju-status P ': JJPush !<ret>'          -docstring 'git push --force-with-lease'
map global badjuju-status U ': JJUndo<ret>'            -docstring 'undo'
map global badjuju-status a ': JJAbandon<ret>'         -docstring 'abandon revision'
map global badjuju-status e ': JJEdit<ret>'            -docstring 'edit commit (move @)'
map global badjuju-status d ': JJDiff<ret>'            -docstring 'diff (change, updates on amend)'
map global badjuju-status D ': JJDiffCommit<ret>'      -docstring 'diff (commit, pinned)'
map global badjuju-status s ': JJSquashCommit<ret>'    -docstring 'squash source/dest (two-step)'
map global badjuju-status S ': JJSquash<ret>'          -docstring 'squash file at cursor'
map global badjuju-status u ': JJUnsquash<ret>'        -docstring 'unsquash file at cursor'
map global badjuju-status x ': JJCancel<ret>'          -docstring 'cancel pending operation'
map global badjuju-status q ': delete-buffer<ret>'     -docstring 'close buffer'
map global badjuju-status b ': enter-user-mode badjuju-bookmark<ret>' \
    -docstring 'bookmark…'
map global badjuju-status r ': enter-user-mode badjuju-rebase<ret>' \
    -docstring 'rebase…'
map global badjuju-status c ': enter-user-mode badjuju-commit<ret>' \
    -docstring 'commit…'
map global badjuju-status '?' ': badjuju-help status<ret>' \
    -docstring 'show help'

# --- log.jujutsu -------------------------------------------------------------

map global badjuju-log R ': JJRefresh<ret>'         -docstring 'refresh'
map global badjuju-log n ': JJNew<ret>'             -docstring 'new change'
map global badjuju-log U ': JJUndo<ret>'            -docstring 'undo'
map global badjuju-log a ': JJAbandon<ret>'         -docstring 'abandon revision'
map global badjuju-log e ': JJEdit<ret>'            -docstring 'edit commit (move @)'
map global badjuju-log d ': JJDiff<ret>'            -docstring 'diff (change, updates on amend)'
map global badjuju-log D ': JJDiffCommit<ret>'      -docstring 'diff (commit, pinned)'
map global badjuju-log s ': JJSquashCommit<ret>'    -docstring 'squash source/dest (two-step)'
map global badjuju-log x ': JJCancel<ret>'          -docstring 'cancel pending operation'
map global badjuju-log q ': delete-buffer<ret>'     -docstring 'close buffer'
map global badjuju-log b ': enter-user-mode badjuju-bookmark<ret>' \
    -docstring 'bookmark…'
map global badjuju-log r ': enter-user-mode badjuju-rebase<ret>' \
    -docstring 'rebase…'
map global badjuju-log c ': enter-user-mode badjuju-commit<ret>' \
    -docstring 'commit…'
map global badjuju-log '?' ': badjuju-help log<ret>' \
    -docstring 'show help'

# --- diff.jujutsu (and diff-change / diff-commit variants) -------------------

map global badjuju-diff R ': JJRefresh<ret>'     -docstring 'refresh'
map global badjuju-diff q ': delete-buffer<ret>' -docstring 'close buffer'
map global badjuju-diff '?' ': badjuju-help diff<ret>' \
    -docstring 'show help'

# --- squash window -----------------------------------------------------------

map global badjuju-squash s ': JJSquashToggle<ret>'    -docstring 'toggle hunk/file at cursor'
map global badjuju-squash e ': JJSquashEditHunk<ret>'  -docstring 'edit hunk before squashing'
map global badjuju-squash a ': JJSquashSelectAll<ret>' -docstring 'select all'
map global badjuju-squash A ': JJSquashSelectNone<ret>' -docstring 'deselect all'
map global badjuju-squash q ': delete-buffer<ret>'     -docstring 'close buffer'
map global badjuju-squash '?' ': badjuju-help squash<ret>' \
    -docstring 'show help'

# --- hunk-edit-*.jujutsu -----------------------------------------------------

map global badjuju-hunk-edit q ': delete-buffer<ret>' -docstring 'close buffer'
map global badjuju-hunk-edit '?' ': badjuju-help hunk-edit<ret>' \
    -docstring 'show help'

# --- describe.jujutsu --------------------------------------------------------

# Save-and-close / abort bindings for describe.jujutsu.
# These are inserted directly on the window, not via the user-mode, so they
# work without pressing <space> first.
hook global WinSetOption filetype=jujutsu %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            */\.jj/badjuju/describe.jujutsu)
                printf 'map window normal <c-c><c-c> ": write; delete-buffer<ret>"\n'
                printf 'map window normal <c-c><c-k> ": delete-buffer!<ret>"\n'
                ;;
        esac
    }
}

map global badjuju-describe '?' ': badjuju-help describe<ret>' \
    -docstring 'show help'
