;; Stub of `helix/static.scm` (keybinding-eligible static commands) — see
;; editor.scm's header.
(provide selection->primary-range current-selection-object range->from
         insert_string goto_definition code_action new)
(define (current-selection-object) 'stub-selection)
(define (selection->primary-range sel) 'stub-range)
(define (range->from r) 0)
(define (insert_string s) (list 'INSERT s))
(define (goto_definition) (list 'GOTO-DEFINITION))
(define (code_action) (list 'CODE-ACTION))
(define (new) (list 'NEW))
