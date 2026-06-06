# Vim-profile keymap: double-letter chord bindings.
#
# Enable by setting before sourcing the plugin:
#   set-option global badjuju_keymap_profile vim
#
# Each action uses a doubled letter (nn = new, ss = squash, etc.)
# or a doubled prefix (bb = bookmark, rr = rebase, cc = commit).
# This keeps single keys free for normal Kakoune navigation.
#
# Implementation: each first letter enters a dedicated sub-mode, and the
# second letter in that sub-mode fires the action. This is the same pattern
# kak-lsp uses for its lsp mode chords.

# Declare the first-level sub-modes (one per distinct first letter)
declare-user-mode badjuju-vim
declare-user-mode badjuju-vim-n
declare-user-mode badjuju-vim-l
declare-user-mode badjuju-vim-s
declare-user-mode badjuju-vim-S
declare-user-mode badjuju-vim-u
declare-user-mode badjuju-vim-f
declare-user-mode badjuju-vim-p
declare-user-mode badjuju-vim-P
declare-user-mode badjuju-vim-U
declare-user-mode badjuju-vim-a
declare-user-mode badjuju-vim-e
declare-user-mode badjuju-vim-d
declare-user-mode badjuju-vim-D
declare-user-mode badjuju-vim-R
declare-user-mode badjuju-vim-q
declare-user-mode badjuju-vim-b          # bookmark sub-mode
declare-user-mode badjuju-vim-b2         # bb + action
declare-user-mode badjuju-vim-r          # rebase sub-mode
declare-user-mode badjuju-vim-r2         # rr + action
declare-user-mode badjuju-vim-c          # commit sub-mode
declare-user-mode badjuju-vim-c2         # cc + action

# --- Top-level vim mode: each key enters its letter sub-mode ----------------

map global badjuju-vim n ': enter-user-mode badjuju-vim-n<ret>'
map global badjuju-vim l ': enter-user-mode badjuju-vim-l<ret>'
map global badjuju-vim s ': enter-user-mode badjuju-vim-s<ret>'
map global badjuju-vim S ': enter-user-mode badjuju-vim-S<ret>'
map global badjuju-vim u ': enter-user-mode badjuju-vim-u<ret>'
map global badjuju-vim f ': enter-user-mode badjuju-vim-f<ret>'
map global badjuju-vim p ': enter-user-mode badjuju-vim-p<ret>'
map global badjuju-vim P ': enter-user-mode badjuju-vim-P<ret>'
map global badjuju-vim U ': enter-user-mode badjuju-vim-U<ret>'
map global badjuju-vim a ': enter-user-mode badjuju-vim-a<ret>'
map global badjuju-vim e ': enter-user-mode badjuju-vim-e<ret>'
map global badjuju-vim d ': enter-user-mode badjuju-vim-d<ret>'
map global badjuju-vim D ': enter-user-mode badjuju-vim-D<ret>'
map global badjuju-vim R ': enter-user-mode badjuju-vim-R<ret>'
map global badjuju-vim q ': enter-user-mode badjuju-vim-q<ret>'
map global badjuju-vim b ': enter-user-mode badjuju-vim-b<ret>'
map global badjuju-vim r ': enter-user-mode badjuju-vim-r<ret>'
map global badjuju-vim c ': enter-user-mode badjuju-vim-c<ret>'
map global badjuju-vim x ': JJCancel<ret>' -docstring 'cancel (also xx)'

# --- Second-letter bindings: double the key to fire the action --------------

map global badjuju-vim-n n ': JJNew<ret>'     -docstring 'new change'
map global badjuju-vim-l l ': JJLog<ret>'     -docstring 'open log'
map global badjuju-vim-s s ': JJSquashCommit<ret>' -docstring 'squash source/dest (two-step)'
map global badjuju-vim-S S ': JJSquash<ret>'  -docstring 'squash file at cursor'
map global badjuju-vim-u u ': JJUnsquash<ret>' -docstring 'unsquash file at cursor'
map global badjuju-vim-f f ': JJFetch<ret>'   -docstring 'git fetch'
map global badjuju-vim-p p ': JJPush<ret>'    -docstring 'git push'
map global badjuju-vim-P P ': JJPush !<ret>'  -docstring 'git push --force-with-lease'
map global badjuju-vim-U U ': JJUndo<ret>'    -docstring 'undo'
map global badjuju-vim-a a ': JJAbandon<ret>' -docstring 'abandon revision'
map global badjuju-vim-e e ': JJEdit<ret>'    -docstring 'edit commit (move @)'
map global badjuju-vim-d d ': JJDescribe<ret>' -docstring 'describe commit in a split'
map global badjuju-vim-D D ': JJDiffCommit<ret>' -docstring 'diff (commit, pinned)'
map global badjuju-vim-R R ': JJRefresh<ret>' -docstring 'refresh'
map global badjuju-vim-q q ': delete-buffer<ret>' -docstring 'close buffer'

# d (single) and D (single) fire diff without doubling — same as magit profile.
# These are in the second-letter sub-modes so 'd' followed by anything else
# cancels gracefully.
map global badjuju-vim-d '<esc>' '' -docstring ''

# --- Bookmark chord: b then b then action ------------------------------------

# First b → enter vim-b; second b → enter vim-b2 with bookmark actions
map global badjuju-vim-b b ': enter-user-mode badjuju-vim-b2<ret>'

map global badjuju-vim-b2 c ': prompt "Bookmark name: " %{ JJBookmark create %val{text} }<ret>' \
    -docstring 'create bookmark'
map global badjuju-vim-b2 m ': prompt "Bookmark name: " %{ JJBookmark move %val{text} }<ret>' \
    -docstring 'move bookmark'
map global badjuju-vim-b2 d ': prompt "Bookmark name: " %{ JJBookmark delete %val{text} }<ret>' \
    -docstring 'delete bookmark'
map global badjuju-vim-b2 t ': prompt "Bookmark name (e.g. main@origin): " %{ JJBookmark track %val{text} }<ret>' \
    -docstring 'track bookmark'
map global badjuju-vim-b2 f ': prompt "Bookmark name: " %{ JJBookmark forget %val{text} }<ret>' \
    -docstring 'forget bookmark'

# --- Rebase chord: r then r then action --------------------------------------

map global badjuju-vim-r r ': enter-user-mode badjuju-vim-r2<ret>'

map global badjuju-vim-r2 s ': JJRebaseSource source<ret>'    -docstring 'mark source (--source)'
map global badjuju-vim-r2 r ': JJRebaseSource revisions<ret>' -docstring 'mark source (--revisions)'
map global badjuju-vim-r2 b ': JJRebaseSource branch<ret>'    -docstring 'mark source (--branch)'
map global badjuju-vim-r2 o ': JJRebaseCommit onto<ret>'      -docstring 'rebase onto this commit'
map global badjuju-vim-r2 A ': JJRebaseCommit after<ret>'     -docstring 'insert after this commit'
map global badjuju-vim-r2 B ': JJRebaseCommit before<ret>'    -docstring 'insert before this commit'

# --- Commit chord: c then c then action --------------------------------------

map global badjuju-vim-c c ': enter-user-mode badjuju-vim-c2<ret>'

map global badjuju-vim-c2 w ': JJDescribe<ret>' -docstring 'reword (open describe.jujutsu)'
map global badjuju-vim-c2 n ': JJNew<ret>'      -docstring 'new child commit'

# --- Activation hook ---------------------------------------------------------
# Mirrors the magit hook: binds <space> to enter badjuju-vim mode.
# The buffer-type detection is the same as in keymap-magit.kak, except the
# top-level mode name is badjuju-vim and sub-modes are prefixed badjuju-vim-*.

hook global WinSetOption filetype=jujutsu %{
    evaluate-commands %sh{
        case "$kak_buffile" in
            */\.jj/badjuju/status.jujutsu|*/\.jj/badjuju/log.jujutsu| \
            */\.jj/badjuju/diff*.jujutsu|*/\.jj/badjuju/squash/*| \
            */\.jj/badjuju/hunk-edit-*.jujutsu| \
            */\.jj/badjuju/describe.jujutsu)
                echo 'map window normal <space> ": enter-user-mode badjuju-vim<ret>"'
                ;;
            *)
                echo 'map window normal <space> ": enter-user-mode badjuju-vim<ret>"'
                ;;
        esac
    }
    hook -once -always window WinSetOption filetype=.* %{
        unmap window normal <space>
    }
}
