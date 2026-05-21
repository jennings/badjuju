; Highlight captures for the jujutsu tree-sitter grammar.
; Eight grammar nodes (counting both fields of section_header) map to the
; standard Neovim capture names so .jujutsu buffers render in the user's
; colorscheme without any plugin-specific highlight groups.

(jj_comment) @comment

(section_header
  header: (_) @keyword
  trailing: (_) @string)

(file_status) @type

(empty_marker) @comment.note

(bookmark) @tag

(commit_id) @number
(change_id) @number

(graph_char) @punctuation.special
